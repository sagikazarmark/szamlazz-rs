//! Where the Restate server comes from: the *server gate* ([`server_gate`],
//! decided once from the environment before anything starts, behind
//! [`launcher_or_skip`]), the [`Launcher`] it yields, and the shape of a
//! server the harness starts ([`ServerSpec`]).
//!
//! Two sources: `RESTATE_ADMIN_URL` / `RESTATE_INGRESS_URL` reuse a running
//! server, `RESTATE_SERVER_BIN` names a `restate-server` binary the harness
//! spawns on the loopback ([`Restate`]). With neither the suite
//! **skips** on a developer machine and **fails** when `CI` is set: a run
//! that passed by skipping proves nothing.

use std::ffi::OsStr;
use std::path::PathBuf;

use crate::server::Restate;

/// Where the suite's Restate server comes from, decided from the environment
/// before anything starts ([`server_gate`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launcher {
    /// `RESTATE_ADMIN_URL` / `RESTATE_INGRESS_URL`: a running server, expected
    /// to run with the flags of the spec it is launched for; nothing is
    /// started or stopped.
    Reuse {
        /// The admin API's base URL (`http://127.0.0.1:9070`).
        admin: String,
        /// The ingress's base URL (`http://127.0.0.1:8080`).
        ingress: String,
    },
    /// `RESTATE_SERVER_BIN`: a `restate-server` binary the harness spawns on
    /// this host, on ports chosen free at launch.
    Binary(PathBuf),
}

/// The server gate, the decision behind [`launcher_or_skip`]: `reuse` first,
/// then `binary`; with neither `Ok(None)` (a skip) unless `ci` is set
/// (non-empty), in which case the suite must not pass by skipping and the
/// answer is the failure message, naming both ways to provide a server.
///
/// # Errors
///
/// Under `CI` with no server: the message the suite fails with.
pub fn server_gate(
    reuse: Option<(String, String)>,
    binary: Option<PathBuf>,
    ci: Option<&OsStr>,
) -> Result<Option<Launcher>, String> {
    if let Some((admin, ingress)) = reuse {
        return Ok(Some(Launcher::Reuse { admin, ingress }));
    }
    if let Some(binary) = binary {
        return Ok(Some(Launcher::Binary(binary)));
    }
    if ci.is_some_and(|value| !value.is_empty()) {
        return Err(
            "no Restate server to run the end-to-end suite against, and CI is set: a skipped run \
             proves nothing. Provide one by setting RESTATE_SERVER_BIN to a restate-server binary \
             (spawned on this host), or by setting RESTATE_ADMIN_URL and RESTATE_INGRESS_URL to a \
             running server with the flags the suite expects."
                .to_owned(),
        );
    }
    Ok(None)
}

/// Whether the suite may reuse a server from the environment: a suite whose
/// server shape is the one a reused server is expected to have may, a suite
/// that needs a server of its own shape (a flag off) may not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reuse {
    /// `RESTATE_ADMIN_URL` / `RESTATE_INGRESS_URL` are honoured.
    Allowed,
    /// The environment's running server is ignored; only a binary launches.
    Never,
}

/// The launcher the environment provides, or `None` after printing why the
/// suite skips; panics with the gate's message under `CI`.
///
/// Reads `RESTATE_ADMIN_URL` / `RESTATE_INGRESS_URL` (when `reuse` allows),
/// `RESTATE_SERVER_BIN` and `CI`.
pub fn launcher_or_skip(reuse: Reuse) -> Option<Launcher> {
    let reusable = match reuse {
        Reuse::Allowed => std::env::var("RESTATE_ADMIN_URL")
            .ok()
            .zip(std::env::var("RESTATE_INGRESS_URL").ok()),
        Reuse::Never => None,
    };
    let binary = std::env::var_os("RESTATE_SERVER_BIN")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    match server_gate(reusable, binary, std::env::var_os("CI").as_deref()) {
        Ok(Some(launcher)) => Some(launcher),
        Ok(None) => {
            eprintln!(
                "skipping: no Restate server (no RESTATE_SERVER_BIN, no RESTATE_ADMIN_URL / \
                 RESTATE_INGRESS_URL)"
            );
            None
        }
        Err(message) => panic!("{message}"),
    }
}

/// The experimental server features `/version` reports, each with the
/// environment flag that enables it: vqueues, protocol v7 (below it the SDK
/// sees no scope) and scoped Virtual Objects. A server is checked at launch
/// to report each of the three on **iff** its flag is in the
/// [`ServerSpec`]; a caller whose suite needs them puts the flags in its spec
/// ([`FLAG_VQUEUES`], [`FLAG_PROTOCOL_V7`], [`FLAG_SCOPED_VIRTUAL_OBJECTS`]).
pub const FEATURES: [(&str, &str); 3] = [
    ("vqueues", FLAG_VQUEUES),
    ("protocol_v7", FLAG_PROTOCOL_V7),
    ("scoped_virtual_objects", FLAG_SCOPED_VIRTUAL_OBJECTS),
];

/// The flag enabling vqueues, as a `NAME=value` environment pair.
pub const FLAG_VQUEUES: &str = "RESTATE_EXPERIMENTAL_ENABLE_VQUEUES=true";
/// The flag enabling protocol v7, as a `NAME=value` environment pair.
pub const FLAG_PROTOCOL_V7: &str = "RESTATE_EXPERIMENTAL_ENABLE_PROTOCOL_V7=true";
/// The flag enabling scoped Virtual Objects, as a `NAME=value` environment
/// pair.
pub const FLAG_SCOPED_VIRTUAL_OBJECTS: &str =
    "RESTATE_EXPERIMENTAL_ENABLE_SCOPED_VIRTUAL_OBJECTS=true";

/// The shape of a server the harness starts: its name (in the node name and
/// the base dir) and its flags, `NAME=value` environment pairs set on the
/// spawned process and expected of a reused server. Its ports are chosen free
/// at launch, so two suites in one test binary, and two runs on one host,
/// each have their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerSpec {
    /// A short name for the shape (`main`, `canary`), in the node name and
    /// the base dir.
    pub name: &'static str,
    /// The environment the server runs with, `NAME=value` pairs; the three
    /// experimental flags among them are what `/version` is checked against.
    pub flags: &'static [&'static str],
}

impl Launcher {
    /// The server of `spec`'s shape, ready: started from the binary or the
    /// running one taken as it is, its admin API answering and `/version`
    /// reporting exactly the features `spec`'s flags enable. Panics when the
    /// server does not come up (a spawned server's own log tail in the
    /// message) or reports other features.
    ///
    /// `RESTATE_ENDPOINT_HOST` overrides the host the server reaches this
    /// process's endpoint at: `127.0.0.1` for a spawned server,
    /// `host.docker.internal` for a reused one (a container of `compose.yaml`).
    pub async fn launch(self, spec: &ServerSpec) -> Restate {
        let endpoint_host = |default: &str| {
            std::env::var("RESTATE_ENDPOINT_HOST").unwrap_or_else(|_| default.to_owned())
        };
        let restate = match self {
            Self::Reuse { admin, ingress } => {
                Restate::reuse(admin, ingress, spec, endpoint_host("host.docker.internal"))
            }
            Self::Binary(binary) => Restate::spawn(&binary, spec, endpoint_host("127.0.0.1")),
        };
        restate.ready().await
    }
}

// ----- the server gate, without a server ----------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// The gate reads the environment once: a reusable server wins, then a
    /// `restate-server` binary; with neither the suite skips on a developer
    /// machine and **fails** under `CI`, naming both ways to provide a server;
    /// a suite that passes by skipping proves nothing.
    #[test]
    fn the_server_gate_prefers_a_reused_server_then_the_binary() {
        let reuse = Some(("http://a:9070".to_owned(), "http://a:8080".to_owned()));
        let binary = Some(PathBuf::from("/opt/restate-server"));
        assert_eq!(
            server_gate(reuse.clone(), binary.clone(), None),
            Ok(Some(Launcher::Reuse {
                admin: "http://a:9070".to_owned(),
                ingress: "http://a:8080".to_owned(),
            }))
        );
        assert_eq!(
            server_gate(None, binary.clone(), None),
            Ok(Some(Launcher::Binary(PathBuf::from("/opt/restate-server"))))
        );
        assert_eq!(
            server_gate(None, binary, Some(OsStr::new("true"))),
            Ok(Some(Launcher::Binary(PathBuf::from("/opt/restate-server")))),
            "the binary suffices under CI too"
        );
    }

    #[test]
    fn the_server_gate_skips_without_a_server_and_fails_under_ci() {
        assert_eq!(server_gate(None, None, None), Ok(None), "no CI: skip");
        assert_eq!(
            server_gate(None, None, Some(OsStr::new(""))),
            Ok(None),
            "an empty CI is unset"
        );
        for ci in ["true", "1", "yes"] {
            let message = server_gate(None, None, Some(OsStr::new(ci)))
                .expect_err("CI is set and there is no server: a failure, never a skip");
            for named in ["CI", "RESTATE_SERVER_BIN", "RESTATE_ADMIN_URL"] {
                assert!(message.contains(named), "CI={ci}: {message}");
            }
            assert!(!message.contains("docker"), "no docker launcher: {message}");
        }
    }
}
