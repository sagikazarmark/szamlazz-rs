//! An end-to-end test harness for [`restate_sdk`] endpoints against a real
//! `restate-server`: building blocks a test suite composes, not a suite.
//!
//! - [`gate`]: where the server comes from (the *server gate*, decided once
//!   from the environment: a running server reused through
//!   `RESTATE_ADMIN_URL` / `RESTATE_INGRESS_URL`, or a `restate-server`
//!   binary named by `RESTATE_SERVER_BIN` and spawned on the loopback; with
//!   neither the suite skips, and fails under `CI`), the [`Launcher`] and the
//!   shape of a server ([`ServerSpec`]: its name, the experimental
//!   [`Feature`]s it needs on or off, checked against `/version` at launch,
//!   and any other environment).
//! - [`server`]: the [`Restate`] handle: the spawned process (free ports, a
//!   process group of its own, log and data under a temp dir kept when the
//!   test fails, stopped on drop and on SIGINT/SIGTERM), an endpoint served
//!   in-process and registered ([`Restate::deploy`]), `set_public`, `drain`,
//!   and the ingress ([`Restate::invoke`]).
//! - [`ingress`]: a [`Call`] (Restate's URL grammar, written once: a
//!   service or an object, under a scope, called or sent), its [`Reply`] and
//!   the fault inside Restate's error envelope ([`Reply::fault`], into the
//!   caller's own type).
//! - [`admin`]: the admin API ([`Admin`]): SQL introspection, journals and
//!   `sys_invocation` rows, the registered handlers, kill / cancel / purge.
//! - [`watch`]: object-wide sampling ([`Watch`]) of run retries while matching
//!   invocations are in flight ([`Retries`]), selected by [`Target`] and
//!   [`ScopeSelection`].
//! - [`introspection`]: `sys_journal` and `sys_invocation` rows
//!   ([`JournalEntry`], [`Invocation`], [`run_result`]) and a handler as
//!   `GET /services` lists it ([`Handler`]).
//! - [`run_names`]: the step-name table ([`Table`] over [`RunPath`] rows,
//!   [`Table::check`] over supplied observations): named-run sequence
//!   conformance and observed path coverage against the current rows. The
//!   module describes its matching rules and limits as supporting evidence
//!   for deployment review, not proof of replay compatibility. Immutable
//!   deployments remain the normal execution model.
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
//! image with the features the suite expects) or point `RESTATE_SERVER_BIN` at
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
//! The README's standalone quick start includes dev-dependencies, a bound
//! service and a complete async test: the gate, deployment, call and retained
//! run. It is compiled as a doctest and executed by `e2e_quick_start` in an
//! isolated consumer crate; the example is kept in the README, once.
//!
//! # Evolvability
//!
//! **Plain data.** Every row and result type ([`Reply`], [`Invocation`],
//! [`JournalEntry`], [`Handler`], [`Retries`], [`Deployment`], [`Walked`],
//! [`Violations`], [`RunPath`], [`ServerSpec`], [`Feature`], [`Target`],
//! [`Call`], [`Launcher`]) has public fields and none is `#[non_exhaustive]`:
//! a spec is `const`-constructible in a consumer, a row is destructured and
//! compared whole in an assertion, and a struct literal in a test reads as
//! the row it stands for. The cost is stated and accepted: a header added to
//! `Reply` or a column to `Invocation` is a breaking release of this crate.
//! It is a test-support crate versioned on its own, its consumers are test
//! suites whose `Cargo.lock` holds the version, and a breaking minor that a
//! compiler error names is cheaper for them than constructors and getters on
//! every row.

// A harness fails a test by panicking: every helper asserts what it observes
// of the server, and a `# Panics` section on each would say the same thing
// two hundred times. The panics that carry information (a server that did not
// come up, a reply without an id) are documented where they are.
#![allow(clippy::missing_panics_doc)]

/// The README's example, compiled as a doctest: the one copy of it.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;

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
pub mod watch;

pub use admin::{Admin, ScopeSelection, Target, sql_literal};
#[cfg(unix)]
pub use gate::{Feature, Launcher, ReusePolicy, ServerSpec, launcher_or_skip, server_gate};
pub use ingress::{Call, Mode, Reply};
pub use introspection::{Handler, Invocation, JournalEntry, run_result, run_result_at};
pub use run_names::{RunPath, Table, Violations, Walked};
#[cfg(unix)]
pub use server::{Deployment, Restate};
pub use watch::{Retries, Watch};

/// A [`reqwest::ClientBuilder`] for the harness's own traffic, all of it
/// plain `http://` on the loopback (the Restate admin and ingress APIs):
/// **no root certificates**, so building it never parses the system CA store
/// (about 28 ms of CPU per client through reqwest's platform verifier, and a
/// failure on a host without a store). Crate-private: a consumer's own
/// loopback clients (at its mocks, over its own `reqwest`) carry their own
/// settings, and one adapter for a hypothetical caller was a seam nothing
/// used.
pub(crate) fn plain_http() -> reqwest::ClientBuilder {
    reqwest::Client::builder().tls_certs_only(std::iter::empty())
}
