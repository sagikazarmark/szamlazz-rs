//! An end-to-end test harness for [`restate_sdk`] endpoints against a real
//! `restate-server`: building blocks a test suite composes, not a suite.
//!
//! - [`gate`]: where the server comes from (the *server gate*, decided once
//!   from the environment: a running server reused through
//!   `RESTATE_ADMIN_URL` / `RESTATE_INGRESS_URL`, or a `restate-server`
//!   binary named by `RESTATE_SERVER_BIN` and spawned on the loopback; with
//!   neither the suite skips, and fails under `CI`), the [`Launcher`] and the
//!   shape of a server ([`ServerSpec`]: its name and its `NAME=value` flags,
//!   checked against `/version` at launch).
//! - [`server`]: the [`Restate`] handle: the spawned process (free ports, a
//!   process group of its own, log and data under a temp dir kept when the
//!   test fails, stopped on drop and on SIGINT/SIGTERM), an endpoint served
//!   in-process and registered ([`Restate::deploy`]), `set_public`, `drain`,
//!   and the ingress ([`Restate::invoke`]).
//! - [`ingress`]: a [`Reply`] and the fault inside Restate's error envelope
//!   ([`Reply::fault`], into the caller's own type).
//! - [`admin`]: the admin API ([`Admin`]): SQL introspection, journals and
//!   `sys_invocation` rows, kill / cancel / purge, and the sampler
//!   ([`Watch`]) over an invocation's run retries while it is in flight.
//! - [`introspection`]: `sys_journal` and `sys_invocation` rows
//!   ([`JournalEntry`], [`Invocation`], [`run_result`]).
//! - [`run_names`]: the run-name matcher ([`RunPath`], [`RunPatterns`],
//!   [`is_prefix_of_path`]) a consumer holds its handlers' `ctx.run` names
//!   to: the sequence an in-place redeploy replays and a pause-and-resume
//!   onto new code needs.
//!
//! The crate knows nothing of any particular endpoint: what it deploys is a
//! `restate_sdk` [`Endpoint`](restate_sdk::prelude::Endpoint), and what it
//! decodes a fault into is the caller's type. A consumer composes its own
//! harness over these (its endpoint, its mocks, its scenarios) and keeps its
//! own table of run names.
//!
//! **Unix only**: the spawned server leads a process group killed with
//! `killpg(2)`, and the stop signals are SIGINT and SIGTERM. The crate does
//! not build elsewhere, by design.
//!
//! # Getting a `restate-server`
//!
//! Either reuse a running one (`RESTATE_ADMIN_URL=http://127.0.0.1:9070
//! RESTATE_INGRESS_URL=http://127.0.0.1:8080`; a container of the Restate
//! image with the flags the suite expects) or point `RESTATE_SERVER_BIN` at
//! the binary, which the Restate image carries at
//! `/usr/local/bin/restate-server`:
//!
//! ```sh
//! id=$(docker create docker.restate.dev/restatedev/restate:1.7.8)
//! docker cp "$id:/usr/local/bin/restate-server" ./restate-server
//! docker rm "$id"
//! RESTATE_SERVER_BIN=$PWD/restate-server cargo test -- --ignored
//! ```
//!
//! `RESTATE_ENDPOINT_HOST` overrides the host the server reaches the
//! in-process endpoint at (`127.0.0.1` for a spawned server,
//! `host.docker.internal` for a reused one).
//!
//! # Example
//!
//! ```no_run
//! use restate_e2e_harness::{Launcher, Reuse, ServerSpec, launcher_or_skip};
//! use restate_e2e_harness::gate::{FLAG_PROTOCOL_V7, FLAG_SCOPED_VIRTUAL_OBJECTS, FLAG_VQUEUES};
//! use restate_sdk::prelude::Endpoint;
//!
//! const SERVER: ServerSpec = ServerSpec {
//!     name: "main",
//!     flags: &[FLAG_VQUEUES, FLAG_PROTOCOL_V7, FLAG_SCOPED_VIRTUAL_OBJECTS],
//! };
//!
//! # async fn run() {
//! let Some(launcher) = launcher_or_skip(Reuse::Allowed) else { return };
//! let restate = launcher.launch(&SERVER).await;
//! let endpoint = Endpoint::builder() /* .bind(MyService) */ .build();
//! restate.deploy(endpoint).await;
//! let reply = restate.invoke("/restate/call/MyService/handler", None, None).await;
//! assert_eq!(reply.status, 200, "{}", reply.body);
//! let runs = restate.admin().runs(reply.invocation_id()).await;
//! # }
//! ```

// A harness fails a test by panicking: every helper asserts what it observes
// of the server, and a `# Panics` section on each would say the same thing
// two hundred times. The panics that carry information (a server that did not
// come up, a reply without an id) are documented where they are.
#![allow(clippy::missing_panics_doc)]

#[cfg(not(unix))]
compile_error!(
    "restate-e2e-harness is unix only: the spawned restate-server leads a process group killed \
     with killpg(2), and the stop signals are SIGINT and SIGTERM"
);

// The two modules that spawn and signal are unix; gated so that a non-unix
// build's one diagnostic is the `compile_error!` above, not a page of
// unresolved `std::os::unix` and `nix` paths beside it.
pub mod admin;
#[cfg(unix)]
pub mod gate;
pub mod ingress;
pub mod introspection;
pub mod run_names;
#[cfg(unix)]
pub mod server;

pub use admin::{Admin, Retries, Target, Watch, sql_literal};
#[cfg(unix)]
pub use gate::{Launcher, Reuse, ServerSpec, launcher_or_skip, server_gate};
pub use ingress::Reply;
pub use introspection::{Invocation, JournalEntry, run_result};
pub use run_names::{RunPath, RunPatterns, is_prefix_of_path};
#[cfg(unix)]
pub use server::{Deployment, Restate};

/// A [`reqwest::ClientBuilder`] for a harness's own traffic, all of it plain
/// `http://` on the loopback (the Restate admin and ingress APIs, a raw post
/// at a mock): **no root certificates**, so building it never parses the
/// system CA store (about 28 ms of CPU per client through reqwest's platform
/// verifier, and a failure on a host without a store). What the harness
/// speaks to the server through; a consumer's own loopback clients are built
/// from it too.
pub fn plain_http() -> reqwest::ClientBuilder {
    reqwest::Client::builder().tls_certs_only(std::iter::empty())
}
