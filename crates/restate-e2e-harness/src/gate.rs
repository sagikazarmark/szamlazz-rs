//! Where the Restate server comes from: the *server gate* ([`server_gate`],
//! decided once from the environment before anything starts, behind
//! [`launcher_or_skip`]), the [`Launcher`] it yields, and the shape of a
//! server the harness starts ([`ServerSpec`]).
//!
//! Two sources: `RESTATE_ADMIN_URL` / `RESTATE_INGRESS_URL` reuse a running
//! server, `RESTATE_SERVER_BIN` names a `restate-server` binary the harness
//! spawns on the loopback ([`Restate`]). With neither the suite
//! **skips** on a developer machine and **fails** when `CI` is set: a run
//! that passed by skipping proves nothing. [`launcher_or_skip`] is the one
//! place the environment is read; [`server_gate`] and [`Launcher::launch`]
//! are functions of what it read.

use std::ffi::OsStr;
use std::path::PathBuf;

use crate::server::Restate;

/// Where the suite's Restate server comes from, decided from the environment
/// before anything starts ([`server_gate`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launcher {
    /// `RESTATE_ADMIN_URL` / `RESTATE_INGRESS_URL`: a running server, expected
    /// to run with the features of the spec it is launched for; nothing is
    /// started or stopped.
    Reuse {
        /// The admin API's base URL (`http://127.0.0.1:9070`).
        admin: String,
        /// The ingress's base URL (`http://127.0.0.1:8080`).
        ingress: String,
        /// The host under which the server reaches this process's endpoint:
        /// `RESTATE_ENDPOINT_HOST`, else `host.docker.internal` (a container
        /// of the Restate image).
        endpoint_host: String,
    },
    /// `RESTATE_SERVER_BIN`: a `restate-server` binary the harness spawns on
    /// this host, on ports chosen free at launch.
    Binary {
        /// The binary.
        binary: PathBuf,
        /// The host under which the server reaches this process's endpoint:
        /// `RESTATE_ENDPOINT_HOST`, else `127.0.0.1`.
        endpoint_host: String,
    },
}

/// The endpoint host of a reused server unless overridden: a container of the
/// Restate image reaches the host this way.
pub const REUSED_ENDPOINT_HOST: &str = "host.docker.internal";

/// The endpoint host of a spawned server unless overridden: the loopback.
pub const SPAWNED_ENDPOINT_HOST: &str = "127.0.0.1";

/// The server gate, the decision behind [`launcher_or_skip`]: `reuse` first,
/// then `binary`; with neither `Ok(None)` (a skip) unless `ci` is set
/// (non-empty), in which case the suite must not pass by skipping and the
/// answer is the failure message, naming both ways to provide a server.
/// `endpoint_host` (`RESTATE_ENDPOINT_HOST`) overrides the host the server
/// reaches this process's endpoint at, whose default depends on the source.
///
/// # Errors
///
/// Under `CI` with no server: the message the suite fails with.
pub fn server_gate(
    reuse: Option<(String, String)>,
    binary: Option<PathBuf>,
    endpoint_host: Option<String>,
    ci: Option<&OsStr>,
) -> Result<Option<Launcher>, String> {
    if let Some((admin, ingress)) = reuse {
        return Ok(Some(Launcher::Reuse {
            admin,
            ingress,
            endpoint_host: endpoint_host.unwrap_or_else(|| REUSED_ENDPOINT_HOST.to_owned()),
        }));
    }
    if let Some(binary) = binary {
        return Ok(Some(Launcher::Binary {
            binary,
            endpoint_host: endpoint_host.unwrap_or_else(|| SPAWNED_ENDPOINT_HOST.to_owned()),
        }));
    }
    if ci.is_some_and(|value| !value.is_empty()) {
        return Err(
            "no Restate server to run the end-to-end suite against, and CI is set: a skipped run \
             proves nothing. Provide one by setting RESTATE_SERVER_BIN to a restate-server binary \
             (spawned on this host), or by setting RESTATE_ADMIN_URL and RESTATE_INGRESS_URL to a \
             running server with the features the suite expects."
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
/// The one place the gate reads the environment: `RESTATE_ADMIN_URL` /
/// `RESTATE_INGRESS_URL` (when `reuse` allows), `RESTATE_SERVER_BIN`,
/// `RESTATE_ENDPOINT_HOST` and `CI`; an empty variable is unset (a
/// `RESTATE_ADMIN_URL=` in a CI matrix is not a server to wait 90 s on).
pub fn launcher_or_skip(reuse: Reuse) -> Option<Launcher> {
    let non_empty = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
    let reusable = match reuse {
        Reuse::Allowed => non_empty("RESTATE_ADMIN_URL").zip(non_empty("RESTATE_INGRESS_URL")),
        Reuse::Never => None,
    };
    let binary = std::env::var_os("RESTATE_SERVER_BIN")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let gate = server_gate(
        reusable,
        binary,
        non_empty("RESTATE_ENDPOINT_HOST"),
        std::env::var_os("CI").as_deref(),
    );
    match gate {
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

/// An experimental server feature: what `/version` reports it as and the
/// environment variable that enables it. A [`ServerSpec`] lists the features
/// it needs on or off; the spawned server is started with each one's
/// variable set to its value, and `/version` is checked at launch to report
/// exactly that, for a spawned and a reused server alike. A feature a spec
/// does not list is neither set nor checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Feature {
    /// The key under `features` in `/version`.
    pub name: &'static str,
    /// The environment variable that enables it (`true` / `false`).
    pub flag: &'static str,
}

impl Feature {
    /// The `NAME=value` environment pair setting this feature to `on`.
    #[must_use]
    pub fn env(&self, on: bool) -> (&'static str, &'static str) {
        (self.flag, if on { "true" } else { "false" })
    }
}

/// Virtual queues, which scoped Virtual Objects need.
pub const VQUEUES: Feature = Feature {
    name: "vqueues",
    flag: "RESTATE_EXPERIMENTAL_ENABLE_VQUEUES",
};

/// Service protocol v7; below it the SDK sees no scope.
pub const PROTOCOL_V7: Feature = Feature {
    name: "protocol_v7",
    flag: "RESTATE_EXPERIMENTAL_ENABLE_PROTOCOL_V7",
};

/// Scoped Virtual Objects (`/restate/scope/{scope}/…`).
pub const SCOPED_VIRTUAL_OBJECTS: Feature = Feature {
    name: "scoped_virtual_objects",
    flag: "RESTATE_EXPERIMENTAL_ENABLE_SCOPED_VIRTUAL_OBJECTS",
};

/// The shape of a server the harness starts: its name (in the node name and
/// the base dir), the features it needs on or off (set on the spawned process
/// and checked against `/version` of a spawned and a reused server alike) and
/// any other environment the spawned process runs with. Its ports are chosen
/// free at launch, so two suites in one test binary, and two runs on one
/// host, each have their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerSpec {
    /// A short name for the shape (`main`, `canary`), in the node name and
    /// the base dir: one path component of `[a-z0-9-]`, non-empty
    /// ([`ServerSpec::validate`]), since the base dir is removed recursively
    /// on drop and a name with a `/` or a `..` in it would name a directory
    /// that is not the harness's.
    pub name: &'static str,
    /// The experimental features and whether each is on: what the spawned
    /// server is started with and what `/version` must report. A feature not
    /// listed is not the suite's concern: neither set nor checked, so a
    /// Restate release turning one on by default breaks nothing here.
    pub features: &'static [(Feature, bool)],
    /// Other environment the spawned server runs with, `NAME=value` pairs;
    /// set before the features and the harness's own values, so a pair
    /// cannot override either.
    pub env: &'static [&'static str],
}

impl ServerSpec {
    /// Whether `name` is one safe path component: non-empty, `[a-z0-9-]`.
    #[must_use]
    pub const fn is_valid_name(name: &str) -> bool {
        let bytes = name.as_bytes();
        if bytes.is_empty() {
            return false;
        }
        let mut i = 0;
        while i < bytes.len() {
            if !(bytes[i].is_ascii_lowercase() || bytes[i].is_ascii_digit() || bytes[i] == b'-') {
                return false;
            }
            i += 1;
        }
        true
    }

    /// Panics, naming the rule, unless [`Self::name`] is one safe path
    /// component ([`Self::is_valid_name`]). Run by [`Launcher::launch`]
    /// before anything touches the filesystem.
    pub fn validate(&self) {
        assert!(
            Self::is_valid_name(self.name),
            "a ServerSpec name is one path component of [a-z0-9-], non-empty: {:?}",
            self.name
        );
    }
}

impl Launcher {
    /// The server of `spec`'s shape, ready: started from the binary or the
    /// running one taken as it is, its admin API and SQL introspection
    /// answering and `/version` reporting each of `spec`'s features as the
    /// spec has it. Panics when the server does not come up (a spawned
    /// server's own log tail in the message) or reports a feature otherwise.
    pub async fn launch(self, spec: &ServerSpec) -> Restate {
        spec.validate();
        let restate = match self {
            Self::Reuse {
                admin,
                ingress,
                endpoint_host,
            } => Restate::reuse(admin, ingress, spec, endpoint_host),
            Self::Binary {
                binary,
                endpoint_host,
            } => Restate::spawn(&binary, spec, endpoint_host),
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
            server_gate(reuse.clone(), binary.clone(), None, None),
            Ok(Some(Launcher::Reuse {
                admin: "http://a:9070".to_owned(),
                ingress: "http://a:8080".to_owned(),
                endpoint_host: REUSED_ENDPOINT_HOST.to_owned(),
            }))
        );
        assert_eq!(
            server_gate(None, binary.clone(), None, None),
            Ok(Some(Launcher::Binary {
                binary: PathBuf::from("/opt/restate-server"),
                endpoint_host: SPAWNED_ENDPOINT_HOST.to_owned(),
            }))
        );
        assert_eq!(
            server_gate(None, binary.clone(), None, Some(OsStr::new("true"))),
            Ok(Some(Launcher::Binary {
                binary: PathBuf::from("/opt/restate-server"),
                endpoint_host: SPAWNED_ENDPOINT_HOST.to_owned(),
            })),
            "the binary suffices under CI too"
        );
        assert_eq!(
            server_gate(reuse, binary, Some("172.17.0.1".to_owned()), None),
            Ok(Some(Launcher::Reuse {
                admin: "http://a:9070".to_owned(),
                ingress: "http://a:8080".to_owned(),
                endpoint_host: "172.17.0.1".to_owned(),
            })),
            "RESTATE_ENDPOINT_HOST overrides the source's default"
        );
    }

    /// A feature's environment pair is its flag with `true` or `false`.
    #[test]
    fn a_feature_renders_its_environment_pair() {
        assert_eq!(
            PROTOCOL_V7.env(true),
            ("RESTATE_EXPERIMENTAL_ENABLE_PROTOCOL_V7", "true")
        );
        assert_eq!(
            PROTOCOL_V7.env(false),
            ("RESTATE_EXPERIMENTAL_ENABLE_PROTOCOL_V7", "false")
        );
    }

    /// The name is one path component: what keeps the base dir, removed
    /// recursively on drop, under the temp directory.
    #[test]
    fn a_server_spec_name_is_one_safe_path_component() {
        for name in ["main", "canary", "smoke-2", "a"] {
            assert!(ServerSpec::is_valid_name(name), "{name}");
        }
        for name in ["", "../x", "a/b", "Main", "with space", "dot.", "é"] {
            assert!(!ServerSpec::is_valid_name(name), "{name:?}");
        }
        let bad = ServerSpec {
            name: "../escape",
            features: &[],
            env: &[],
        };
        let outcome = std::panic::catch_unwind(|| bad.validate());
        assert!(
            outcome.is_err(),
            "validate refuses the name before any spawn"
        );
    }

    #[test]
    fn the_server_gate_skips_without_a_server_and_fails_under_ci() {
        assert_eq!(server_gate(None, None, None, None), Ok(None), "no CI: skip");
        assert_eq!(
            server_gate(None, None, None, Some(OsStr::new(""))),
            Ok(None),
            "an empty CI is unset"
        );
        for ci in ["true", "1", "yes"] {
            let message = server_gate(None, None, None, Some(OsStr::new(ci)))
                .expect_err("CI is set and there is no server: a failure, never a skip");
            for named in ["CI", "RESTATE_SERVER_BIN", "RESTATE_ADMIN_URL"] {
                assert!(message.contains(named), "CI={ci}: {message}");
            }
            assert!(!message.contains("docker"), "no docker launcher: {message}");
        }
    }
}
