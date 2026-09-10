//! The Restate server as the harness holds it ([`Restate`]): a running one
//! reused from the environment or a `restate-server` process the harness
//! spawned on the loopback (on ports chosen free at launch, so nothing is
//! fixed and two runs on one host collide with nothing), its admin API
//! ([`Restate::admin`]), the ingress ([`Restate::invoke`]) and the deployment
//! of a `restate_sdk` endpoint served in-process ([`Restate::deploy`]).
//!
//! A server the harness starts is stopped when the handle drops, and by a
//! SIGINT or SIGTERM to the test process, which unwinds nothing: the process
//! leads a process group of its own and the group is killed. Both signals are
//! registered before the first spawn; initialization failure prevents launch.
//! Spawn and registration share a lock with shutdown, which closes admission
//! before killing the groups, covering concurrent launches too. Signal exits
//! use statuses 130 / 143; normal drop also reaps the child.

use std::fmt;
use std::fs;
use std::net::TcpListener;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, Once, PoisonError};
use std::time::{Duration, Instant};

use restate_sdk::prelude::{Endpoint, HttpServer};
use serde_json::Value;

use crate::admin::{Admin, poll_until};
use crate::gate::{Feature, SPAWNED_ENDPOINT_HOST, ServerSpec};
use crate::ingress::{Call, Reply};
use crate::plain_http;

/// How long [`Restate::ready`] waits for the admin API.
const READY_DEADLINE: Duration = Duration::from_secs(90);

/// The timeout of the harness's own HTTP client: an ingress call waits for
/// the invocation's answer, which a run retry policy may hold for a while.
const HTTP_TIMEOUT: Duration = Duration::from_secs(120);

/// The loopback ports of a server the harness started, chosen free at launch:
/// a listener on port 0 for each (bound, read, released and passed through
/// `RESTATE_*`). Nothing here is fixed, so a second run on the host, another
/// Restate or anything else on a port collides with nothing; and nothing is
/// bound beyond the loopback, where the harness connects: the admin API has
/// no authentication, and a suite is no reason to offer it to the network.
#[derive(Debug, Clone, Copy)]
struct Ports {
    ingress: u16,
    admin: u16,
    node: u16,
}

impl fmt::Display for Ports {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "admin {} / ingress {} / node {}",
            self.admin, self.ingress, self.node
        )
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

/// Claim a fresh directory, even if a previous process with this pid left
/// evidence behind. The counter selects candidates; exclusive creation owns
/// them. Port reuse has no bearing on storage identity.
fn launch_directory(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    loop {
        let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
        let candidate = std::env::temp_dir().join(format!(
            "restate-e2e-{}-{name}-{sequence}",
            std::process::id(),
        ));
        #[cfg(test)]
        directory_tests::collide(&candidate);
        match fs::create_dir(&candidate) {
            Ok(()) => return candidate,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => panic!(
                "allocate the server's base dir {}: {error}",
                candidate.display()
            ),
        }
    }
}

/// A `restate-server` process the harness spawned: its ports, the child (kept
/// to be reaped after its group is killed) and its base directory, removed on
/// drop unless the test is failing; then it stays, with `restate-server.log`
/// in it.
struct Process {
    ports: Ports,
    child: Child,
    base_dir: PathBuf,
}

impl Process {
    /// The process group's id: the child's pid, since it was spawned with
    /// `process_group(0)`.
    fn group(&self) -> u32 {
        self.child.id()
    }

    /// Why the process is gone, if it is: its exit status and the tail of
    /// its log, naming the ports it was to bind.
    fn exited(&mut self) -> Option<String> {
        let status = exit_status(&self.child)?;
        let log = fs::read_to_string(self.base_dir.join("restate-server.log")).unwrap_or_default();
        let tail: Vec<&str> = log.lines().rev().take(30).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        Some(format!(
            "restate-server exited with {status}, on ports {} \
             (chosen free at launch; one may have been taken since). The last lines of its \
             log:\n{}",
            self.ports,
            tail.join("\n")
        ))
    }
}

/// Observe without reaping: the zombie leader reserves its pid until Drop
/// signals the group, reaps and unregisters under the lifecycle lock.
#[cfg(not(any(
    target_os = "openbsd",
    target_os = "redox",
    target_os = "cygwin",
    target_os = "horizon"
)))]
fn exit_status(child: &Child) -> Option<String> {
    use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
    let pid = Pid::from_raw(i32::try_from(child.id()).expect("child pid fits i32"))
        .expect("child pid is positive");
    loop {
        match waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        ) {
            Ok(status) => return status.map(|status| format!("{status:?}")),
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => panic!(
                "inspect restate-server {} without reaping: {error}",
                child.id()
            ),
        }
    }
}

// These targets have no waitid wrapper. Readiness remains bounded by its
// deadline; never substitute try_wait, which would release the reserved pid.
#[cfg(any(
    target_os = "openbsd",
    target_os = "redox",
    target_os = "cygwin",
    target_os = "horizon"
))]
fn exit_status(_child: &Child) -> Option<String> {
    None
}

/// A Restate server: an existing one (from the environment) or a
/// `restate-server` process the harness spawned (stopped on drop, and on a
/// stop signal to the test process).
pub struct Restate {
    admin: Admin,
    ingress: String,
    http: reqwest::Client,
    /// The features the server runs with, on or off: what `/version` must
    /// report.
    features: &'static [(Feature, bool)],
    /// The host name under which the server reaches this process's endpoint.
    endpoint_host: String,
    process: Option<Process>,
    /// Dropping the senders requests shutdown of every local endpoint,
    /// including endpoints registered with a reused server.
    endpoints: Mutex<Vec<tokio::sync::oneshot::Sender<()>>>,
}

/// Every server this test process started and has not stopped yet, by the
/// process group that stops it: what [`stop_on_signal`] kills when the process
/// is told to stop.
static STARTED: Mutex<Started> = Mutex::new(Started {
    groups: Vec::new(),
    stopping: false,
});

struct Started {
    groups: Vec<u32>,
    stopping: bool,
}

fn started() -> std::sync::MutexGuard<'static, Started> {
    STARTED.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `SIGKILL` to the process group `pid` leads; a group already gone
/// (`ESRCH`) is the wanted state, any other failure is reported on stderr,
/// never swallowed (a process group is the operator's to find).
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

/// Stops every started server on SIGINT or SIGTERM to the test process, then
/// exits with the signal's conventional status. A signal ends the process
/// without unwinding, so nothing's `Drop` runs; without this, a Ctrl-C leaves
/// the server (in a group of its own, so the terminal's SIGINT does not reach
/// it) holding its ports. Installed once, before the first server can spawn;
/// the caller waits for both registrations and panics if initialization fails.
/// Its own thread and runtime outlive the test that started the first server.
fn stop_on_signal() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        // Concurrent first launches all wait for this handshake. A failed
        // initialization poisons INSTALL, preventing later launches as well.
        let (ready, registered) = std::sync::mpsc::sync_channel(0);
        std::thread::Builder::new()
            .name("e2e-stop-on-signal".to_owned())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("a runtime for the stop-signal handler before spawning any server");
                runtime.block_on(async {
                    use tokio::signal::unix::{SignalKind, signal};
                    #[cfg(test)]
                    lifecycle_tests::pause("register");
                    let mut interrupt = signal(SignalKind::interrupt())
                        .expect("register SIGINT before spawning any server");
                    let mut terminate = signal(SignalKind::terminate())
                        .expect("register SIGTERM before spawning any server");
                    ready.send(()).expect("the launching thread waits for registration");
                    let (signal, status) = tokio::select! {
                        _ = interrupt.recv() => ("SIGINT", 130),
                        _ = terminate.recv() => ("SIGTERM", 143),
                    };
                    #[cfg(test)]
                    lifecycle_tests::pause("signal");
                    {
                        let mut started = started();
                        started.stopping = true;
                        eprintln!(
                            "{signal}: stopping {} Restate server(s) the suite started, then exiting",
                            started.groups.len()
                        );
                        // Keep teardown from reaping a leader (and allowing
                        // its pid to be reused) before we signal its group.
                        for group in &started.groups {
                            kill_group(*group);
                        }
                    }
                    #[cfg(test)]
                    lifecycle_tests::pause("shutdown");
                    std::process::exit(status);
                });
            })
            .expect("a stop-signal thread before spawning any server");
        registered.recv().expect("both stop signals registered before spawning any server");
    });
}

impl Restate {
    fn new(
        admin: String,
        mut ingress: String,
        spec: &ServerSpec,
        endpoint_host: String,
        process: Option<Process>,
    ) -> Self {
        let http = plain_http()
            .timeout(HTTP_TIMEOUT)
            .build()
            .expect("the harness's HTTP client");
        ingress.truncate(ingress.trim_end_matches('/').len());
        Self {
            admin: Admin::new(admin, http.clone()),
            ingress,
            http,
            features: spec.features,
            endpoint_host,
            process,
            endpoints: Mutex::new(Vec::new()),
        }
    }

    /// A running server at `admin` / `ingress`, taken as it is: nothing is
    /// started or stopped; `spec`'s features are what [`Self::ready`] expects
    /// `/version` to report.
    pub(crate) fn reuse(
        admin: String,
        ingress: String,
        spec: &ServerSpec,
        endpoint_host: String,
    ) -> Self {
        Self::new(admin, ingress, spec, endpoint_host, None)
    }

    /// A `restate-server` process from `binary` with `spec`'s features and
    /// environment, bound to the loopback on three ports chosen free
    /// ([`free_ports`]), its data and log under a directory of its own in the
    /// temp dir (`restate-e2e-{pid}-{name}-{sequence}`, claimed by exclusive
    /// directory creation, skipping existing candidates). Repeated and concurrent
    /// launches share no storage, even when ports are reused or an earlier
    /// failure left its directory behind. The process leads a group of its own,
    /// which is what gets killed. Configured through Restate's environment
    /// (`RESTATE_<SECTION>__<KEY>`), so no config file is written; `spec.env`
    /// is set first, the features next and the harness's own values last, so
    /// a pair cannot move the base dir out of the temp directory, a bind
    /// address off the loopback (the admin API has no authentication) or a
    /// feature off what the spec says.
    pub(crate) fn spawn(binary: &Path, spec: &ServerSpec, endpoint_host: String) -> Self {
        let [ingress, admin, node] = free_ports::<3>();
        let ports = Ports {
            ingress,
            admin,
            node,
        };
        let base_dir = launch_directory(spec.name);
        eprintln!(
            "restate-server's base dir: {} (kept if launch fails)",
            base_dir.display()
        );
        let log =
            fs::File::create_new(base_dir.join("restate-server.log")).expect("the server log");
        let mut command = Command::new(binary);
        command.arg("--no-logo");
        for pair in spec.env {
            let (name, value) = pair
                .split_once('=')
                .unwrap_or_else(|| panic!("a server environment pair is NAME=value: {pair:?}"));
            command.env(name, value);
        }
        for (feature, on) in spec.features {
            let (name, value) = feature.env(*on);
            command.env(name, value);
        }
        command
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
        #[cfg(test)]
        lifecycle_tests::pause("install");
        stop_on_signal();
        // A signal cannot miss a child between spawn and registration.
        let mut registry = started();
        assert!(
            !registry.stopping,
            "cannot launch a Restate server during signal shutdown"
        );
        let child = command
            .spawn()
            .unwrap_or_else(|error| panic!("spawn {}: {error}", binary.display()));
        #[cfg(test)]
        lifecycle_tests::spawned(child.id());
        eprintln!(
            "restate-server (pid {}) on {ports}, base dir {}",
            child.id(),
            base_dir.display()
        );
        let process = Process {
            ports,
            child,
            base_dir,
        };
        registry.groups.push(process.group());
        drop(registry);
        #[cfg(test)]
        lifecycle_tests::pause("registered");
        Self::new(
            format!("http://127.0.0.1:{admin}"),
            format!("http://127.0.0.1:{ingress}"),
            spec,
            endpoint_host,
            Some(process),
        )
    }

    /// Waits for the admin API (failing at once, with the server's own account
    /// of it, when a server the harness started is gone before then), then
    /// for the SQL introspection API to answer (`/health` is up before the
    /// partition store behind `sys_invocation` is provisioned; a suite's first
    /// read would otherwise meet a 500), and checks that `/version` reports
    /// each of the spec's features as the spec has it.
    pub(crate) async fn ready(mut self) -> Self {
        // Not `poll_until`: this wait has a second way out, the spawned
        // process gone, read off `&mut self.process` between probes.
        let deadline = Instant::now() + READY_DEADLINE;
        loop {
            // Bounded by the deadline, not the client's 120 s timeout: a
            // server that accepts the connection and stalls does not defer
            // the liveness check below.
            let health = tokio::time::timeout_at(
                deadline.into(),
                self.http
                    .get(format!("{}/health", self.admin.base()))
                    .send(),
            )
            .await;
            if let Ok(Ok(response)) = health
                && response.status().is_success()
            {
                break;
            }
            if let Some(reason) = self.exited() {
                panic!("{reason}");
            }
            assert!(
                Instant::now() < deadline,
                "the Restate admin API at {} did not come up",
                self.admin.base()
            );
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        poll_until(
            deadline.saturating_duration_since(Instant::now()),
            Duration::from_millis(200),
            || self.admin.sql("SELECT id FROM sys_invocation LIMIT 1"),
            |error| {
                format!(
                    "the SQL introspection API at {} did not come up: {error}",
                    self.admin.base()
                )
            },
        )
        .await;

        let version: Value = self
            .http
            .get(format!("{}/version", self.admin.base()))
            .send()
            .await
            .expect("GET /version")
            .json()
            .await
            .expect("the /version body is JSON");
        for (feature, expected) in self.features {
            assert_eq!(
                version["features"][feature.name],
                Value::Bool(*expected),
                "the Restate server must run with {} {} ({}={}): {version}",
                feature.name,
                if *expected { "enabled" } else { "disabled" },
                feature.flag,
                expected
            );
        }
        self
    }

    /// Why the server the harness started is gone, if it is: the spawned
    /// process exited, with its status and the tail of its log. Polled while
    /// waiting for the admin API, so a server that cannot start (a port chosen
    /// free and taken since, most likely) is reported at once, naming its
    /// ports, rather than waited on until the deadline. `None` for a reused
    /// server, and for one still running.
    /// Inspection leaves the child unreaped until group cleanup on Drop.
    /// On Unix targets without `waitid` (OpenBSD, Redox, Cygwin, Horizon),
    /// exit inspection is unavailable and this returns `None`; readiness still
    /// fails at its deadline.
    pub fn exited(&mut self) -> Option<String> {
        self.process.as_mut().and_then(Process::exited)
    }

    /// The admin API: SQL introspection and the invocation operations.
    #[must_use]
    pub fn admin(&self) -> &Admin {
        &self.admin
    }

    /// The admin API's base URL.
    #[must_use]
    pub fn admin_url(&self) -> &str {
        self.admin.base()
    }

    /// The ingress base URL. Combine with [`Call::path`] and a consumer-owned
    /// HTTP client for raw bodies, custom headers or a different timeout.
    /// For example, malformed JSON must be sent as raw bytes rather than a
    /// JSON string through [`Self::invoke`].
    #[must_use]
    pub fn ingress_url(&self) -> &str {
        &self.ingress
    }

    /// Serves `endpoint` on a free port of this host (a task of the current
    /// runtime, owned by this handle) and registers it with the server as
    /// `http://{endpoint_host}:{port}` with `force: true`, retried until the
    /// admin API accepts it. Repeatable: a new URI is a new revision of the
    /// services it binds, and new invocations route to it, so a redeploy is a
    /// second call. Bound to the loopback when the server reaches this process
    /// there (a spawned server without a `RESTATE_ENDPOINT_HOST` override); on
    /// every interface otherwise (a reused server, which may be a container
    /// reaching back to this host, or a spawned one told to reach the endpoint
    /// by another address). The endpoint has no identity key, so it is offered
    /// to the network only where the server needs it.
    ///
    /// Dropping `Restate` requests shutdown of every endpoint it deployed,
    /// including on a reused server. The runtime must keep running to execute
    /// shutdown; the SDK allows up to ten seconds for active connections to drain.
    /// Dropping the returned [`Deployment`] only discards its URI/port descriptor.
    /// Earlier endpoints remain available while this handle lives.
    ///
    /// Served with `serve_with_cancel` over this handle's shutdown signal
    /// rather than the SDK's `serve`, whose shutdown future is `ctrl_c()`: the
    /// harness owns SIGINT (it stops the servers it started and exits), and an
    /// endpoint that installed its own handler per deployment would race it.
    pub async fn deploy(&self, endpoint: Endpoint) -> Deployment {
        let bind = if self.endpoint_host == SPAWNED_ENDPOINT_HOST {
            "127.0.0.1:0"
        } else {
            "0.0.0.0:0"
        };
        let listener = TcpListener::bind(bind).expect("bind the endpoint");
        let port = listener
            .local_addr()
            .expect("the endpoint's address")
            .port();
        listener
            .set_nonblocking(true)
            .expect("a non-blocking listener");
        let listener = tokio::net::TcpListener::from_std(listener).expect("a tokio listener");
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        self.endpoints
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(stop);
        tokio::spawn(async move {
            HttpServer::new(endpoint)
                .serve_with_cancel(listener, stopped)
                .await;
        });

        let uri = format!("http://{}:{port}", self.endpoint_host);
        self.admin.register(&uri).await;
        Deployment { uri, port }
    }

    /// `PATCH /services/{service}` with `public`, asserting success
    /// ([`Admin::set_public`]).
    pub async fn set_public(&self, service: &str, public: bool) {
        self.admin.set_public(service, public).await;
    }

    /// Waits until `sys_invocation` holds no invocation that is not completed
    /// ([`Admin::drain`]).
    pub async fn drain(&self) {
        self.admin.drain().await;
    }

    /// `POST {ingress}{call.path()}`: `body` as JSON when given,
    /// `idempotency` as the `Idempotency-Key` header when given. The reply,
    /// whatever its status: the body parsed as JSON when it is, kept as a
    /// string otherwise.
    pub async fn invoke(
        &self,
        call: &Call<'_>,
        body: Option<&Value>,
        idempotency: Option<&str>,
    ) -> Reply {
        let mut request = self.http.post(format!("{}{}", self.ingress, call.path()));
        if let Some(idempotency) = idempotency {
            request = request.header("idempotency-key", idempotency);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().await.expect("the ingress call");
        let status = response.status().as_u16();
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        let invocation_id = header("x-restate-id");
        let error_source = header("x-restate-error-source");
        let text = response.text().await.expect("the reply body");
        let body = serde_json::from_str(&text).unwrap_or(Value::String(text));
        Reply {
            status,
            body,
            invocation_id,
            error_source,
        }
    }
}

impl fmt::Debug for Restate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Restate")
            .field("admin", &self.admin.base())
            .field("ingress", &self.ingress)
            .field("endpoint_host", &self.endpoint_host)
            .field("features", &self.features)
            .field(
                "spawned",
                &self.process.as_ref().map(|process| process.ports),
            )
            .finish_non_exhaustive()
    }
}

impl Drop for Restate {
    fn drop(&mut self) {
        self.endpoints
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        let Some(mut process) = self.process.take() else {
            return;
        };
        let group = process.group();
        // Serialize normal teardown with shutdown and launches too: a group
        // stays registered until it has been killed and its leader reaped.
        let mut registry = started();
        kill_group(group);
        // The group is killed above; this reaps the leader.
        let _ = process.child.kill();
        let _ = process.child.wait();
        registry.groups.retain(|started| *started != group);
        drop(registry);
        if std::thread::panicking() {
            eprintln!(
                "restate-server's base dir is kept for inspection: {}",
                process.base_dir.display()
            );
        } else {
            let _ = fs::remove_dir_all(&process.base_dir);
        }
    }
}

/// An endpoint served in-process and registered with the server
/// ([`Restate::deploy`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deployment {
    /// The URI the server was given (`http://{endpoint_host}:{port}`).
    pub uri: String,
    /// The port the endpoint listens on: on the loopback when the server
    /// reaches it there, on every interface of this host otherwise.
    pub port: u16,
}

#[cfg(test)]
mod lifecycle_tests;

#[cfg(test)]
mod directory_tests;
