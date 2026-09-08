//! The Restate server as the harness holds it ([`Restate`]): a running one
//! reused from the environment or a `restate-server` process the harness
//! spawned on the loopback (on ports chosen free at launch, so nothing is
//! fixed and two runs on one host collide with nothing), its admin API
//! ([`Restate::admin`]), the ingress ([`Restate::invoke`]) and the deployment
//! of a `restate_sdk` endpoint served in-process ([`Restate::deploy`]).
//!
//! A server the harness starts is stopped when the handle drops, and by a
//! SIGINT or SIGTERM to the test process (a handler installed with the first
//! server started), which unwinds
//! nothing: the process leads a process group of its own and the group is
//! killed.

use std::fmt;
use std::fs;
use std::net::TcpListener;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, Once, PoisonError};
use std::time::{Duration, Instant};

use restate_sdk::prelude::{Endpoint, HttpServer};
use serde_json::Value;

use crate::admin::Admin;
use crate::gate::{FEATURES, ServerSpec};
use crate::ingress::Reply;
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
        let status = self.child.try_wait().ok().flatten()?;
        let log = fs::read_to_string(self.base_dir.join("restate-server.log")).unwrap_or_default();
        let tail: Vec<&str> = log.lines().rev().take(30).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        Some(format!(
            "restate-server exited with {status} before its admin API came up, on ports {} \
             (chosen free at launch; one may have been taken since). The last lines of its \
             log:\n{}",
            self.ports,
            tail.join("\n")
        ))
    }
}

/// A Restate server: an existing one (from the environment) or a
/// `restate-server` process the harness spawned (stopped on drop, and on a
/// stop signal to the test process).
pub struct Restate {
    admin: Admin,
    ingress: String,
    http: reqwest::Client,
    /// The flags the server runs with: what `/version` must report.
    flags: &'static [&'static str],
    /// The host name under which the server reaches this process's endpoint.
    endpoint_host: String,
    process: Option<Process>,
}

/// Every server this test process started and has not stopped yet, by the
/// process group that stops it: what [`stop_on_signal`] kills when the process
/// is told to stop.
static STARTED: Mutex<Vec<u32>> = Mutex::new(Vec::new());

fn started() -> std::sync::MutexGuard<'static, Vec<u32>> {
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
/// it) holding its ports. Installed once, on the first server started; its
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
                         Restate servers this run starts behind (`pkill restate-server`)"
                    );
                    return;
                };
                runtime.block_on(async {
                    let (signal, status) = match stop_signal().await {
                        Ok(stopped) => stopped,
                        Err(error) => {
                            eprintln!(
                                "WARNING: the stop-signal handler could not register ({error}); a \
                                 Ctrl-C will leave the Restate servers this run starts behind \
                                 (`pkill restate-server`)"
                            );
                            return;
                        }
                    };
                    let groups = started().clone();
                    eprintln!(
                        "{signal}: stopping {} Restate server(s) the suite started, then exiting",
                        groups.len()
                    );
                    for group in &groups {
                        kill_group(*group);
                    }
                    std::process::exit(status);
                });
            });
        if let Err(error) = handler {
            eprintln!(
                "WARNING: no thread for the stop-signal handler ({error}); a Ctrl-C will leave the \
                 Restate servers this run starts behind (`pkill restate-server`)"
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

impl Restate {
    fn new(
        admin: String,
        ingress: String,
        spec: &ServerSpec,
        endpoint_host: String,
        process: Option<Process>,
    ) -> Self {
        let http = plain_http()
            .timeout(HTTP_TIMEOUT)
            .build()
            .expect("the harness's HTTP client");
        Self {
            admin: Admin::new(admin, http.clone()),
            ingress,
            http,
            flags: spec.flags,
            endpoint_host,
            process,
        }
    }

    /// A running server at `admin` / `ingress`, taken as it is: nothing is
    /// started or stopped; `spec`'s flags are what [`Self::ready`] expects
    /// `/version` to report.
    pub(crate) fn reuse(
        admin: String,
        ingress: String,
        spec: &ServerSpec,
        endpoint_host: String,
    ) -> Self {
        Self::new(admin, ingress, spec, endpoint_host, None)
    }

    /// A `restate-server` process from `binary` with `spec`'s flags, bound to
    /// the loopback on three ports chosen free ([`free_ports`]), its data and
    /// log under a directory of its own in the temp dir
    /// (`restate-e2e-{pid}-{name}-{admin port}`: the port is unique per
    /// launch on the host, so two launches of one spec in one test binary
    /// share nothing), leading a process group of its own so
    /// that the group is what gets killed. Configured through Restate's
    /// environment (`RESTATE_<SECTION>__<KEY>`), so no config file is written.
    pub(crate) fn spawn(binary: &Path, spec: &ServerSpec, endpoint_host: String) -> Self {
        let [ingress, admin, node] = free_ports::<3>();
        let ports = Ports {
            ingress,
            admin,
            node,
        };
        let base_dir = std::env::temp_dir().join(format!(
            "restate-e2e-{}-{}-{admin}",
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
            let (name, value) = flag
                .split_once('=')
                .unwrap_or_else(|| panic!("a server flag is NAME=value: {flag:?}"));
            command.env(name, value);
        }
        let child = command
            .spawn()
            .unwrap_or_else(|error| panic!("spawn {}: {error}", binary.display()));
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
        started().push(process.group());
        stop_on_signal();
        Self::new(
            format!("http://127.0.0.1:{admin}"),
            format!("http://127.0.0.1:{ingress}"),
            spec,
            endpoint_host,
            Some(process),
        )
    }

    /// Waits for the admin API (failing at once, with the server's own account
    /// of it, when a server the harness started is gone before then) and
    /// checks that `/version` reports exactly the [`FEATURES`] the server's
    /// flags enable, no other.
    pub(crate) async fn ready(mut self) -> Self {
        // Not `poll_until`: this wait has a second way out, the spawned
        // process gone, read off `&mut self.process` between probes.
        let deadline = Instant::now() + READY_DEADLINE;
        loop {
            if let Ok(response) = self
                .http
                .get(format!("{}/health", self.admin.base()))
                .send()
                .await
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

        let version: Value = self
            .http
            .get(format!("{}/version", self.admin.base()))
            .send()
            .await
            .expect("GET /version")
            .json()
            .await
            .expect("the /version body is JSON");
        for (feature, flag) in FEATURES {
            let expected = self.flags.contains(&flag);
            assert_eq!(
                version["features"][feature],
                Value::Bool(expected),
                "the Restate server must run with {feature} {}: {version}",
                if expected { "enabled" } else { "disabled" }
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

    /// Serves `endpoint` on a free port of this host (a task of the current
    /// runtime, for the runtime's life) and registers it with the server as
    /// `http://{endpoint_host}:{port}` with `force: true`, retried until the
    /// admin API accepts it. Repeatable: a new URI is a new revision of the
    /// services it binds, and new invocations route to it, so a redeploy is a
    /// second call.
    ///
    /// Served with `serve_with_cancel` over a future that never completes
    /// rather than the SDK's `serve`, whose shutdown future is `ctrl_c()`: the
    /// harness owns SIGINT (it stops the servers it started and exits), and an
    /// endpoint that installed its own handler per deployment would race it.
    pub async fn deploy(&self, endpoint: Endpoint) -> Deployment {
        let listener = TcpListener::bind("0.0.0.0:0").expect("bind the endpoint");
        let port = listener
            .local_addr()
            .expect("the endpoint's address")
            .port();
        listener
            .set_nonblocking(true)
            .expect("a non-blocking listener");
        let listener = tokio::net::TcpListener::from_std(listener).expect("a tokio listener");
        tokio::spawn(async move {
            HttpServer::new(endpoint)
                .serve_with_cancel(listener, std::future::pending::<()>())
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

    /// `POST {ingress}{path}`: `body` as JSON when given, `idempotency` as the
    /// `Idempotency-Key` header when given. The reply, whatever its status:
    /// the body parsed as JSON when it is, kept as a string otherwise.
    pub async fn invoke(
        &self,
        path: &str,
        body: Option<&Value>,
        idempotency: Option<&str>,
    ) -> Reply {
        let mut request = self.http.post(format!("{}{path}", self.ingress));
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
            .field("flags", &self.flags)
            .field(
                "spawned",
                &self.process.as_ref().map(|process| process.ports),
            )
            .finish_non_exhaustive()
    }
}

impl Drop for Restate {
    fn drop(&mut self) {
        let Some(mut process) = self.process.take() else {
            return;
        };
        let group = process.group();
        kill_group(group);
        started().retain(|started| *started != group);
        // The group is killed above; this reaps the leader.
        let _ = process.child.kill();
        let _ = process.child.wait();
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
    /// The port the endpoint listens on, on every interface of this host.
    pub port: u16,
}
