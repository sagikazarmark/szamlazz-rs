//! Standalone endpoint hosting the szamlazz.hu services for Restate.
//!
//! Binds the `Szamlazz.Order` Virtual Object and the `Szamlazz.Agent` service
//! of [`restate_szamlazz`] to one HTTP/2 endpoint and serves it for a Restate
//! server to register.

mod config;

use std::future::Future;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use anyhow::{Context as _, Result};
use clap::Parser;
use restate_sdk::endpoint::Endpoint;
use restate_sdk::http_server::HttpServer;
use restate_sdk::service::Discoverable;
use restate_szamlazz::account::StaticResolver;
use restate_szamlazz::{Accounts, Agent, Order};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use crate::config::{EndpointConfig, RequestIdentity};

/// The signals that stop the process, as the start-up log names them.
#[cfg(unix)]
const STOP_SIGNALS: &str = "SIGTERM, SIGINT";
/// The signals that stop the process, as the start-up log names them.
#[cfg(windows)]
const STOP_SIGNALS: &str = "Ctrl-C";

#[derive(Parser, Debug)]
#[command(version)]
struct Cli {
    /// Path to config file (supports JSON, YAML, or TOML).
    #[arg(long, value_name = "FILE", env = "CONFIG_FILE")]
    config: Option<PathBuf>,

    /// Address to bind.
    #[arg(long, value_name = "ADDR", default_value_t = IpAddr::V4(Ipv4Addr::UNSPECIFIED), env = "BIND_ADDR")]
    bind: IpAddr,

    /// Port to listen on.
    #[arg(long, default_value = "9080", env = "PORT")]
    port: u16,

    /// Load and validate the configuration, build the endpoint, log what
    /// would be served and exit 0, without listening. Non-zero with the
    /// error otherwise. For CI and init containers.
    #[arg(long)]
    check_config: bool,
}

impl Cli {
    fn load_config(&self) -> Result<EndpointConfig> {
        EndpointConfig::load(&config::figment(self.config.as_deref())?)
    }

    /// The address to listen on, `{bind}:{port}`.
    fn bind_addr(&self) -> SocketAddr {
        SocketAddr::from((self.bind, self.port))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let config = cli.load_config()?;
    let bind_addr = cli.bind_addr();
    let endpoint = build_endpoint(config, bind_addr)?;

    if cli.check_config {
        tracing::info!("configuration is valid; not listening (--check-config)");
        return Ok(());
    }

    // The signal handlers go in before the port opens: from the moment a
    // Restate server can reach the endpoint, a stop is honoured rather than
    // fatal.
    let stop = stop_signal().context("failed to install the stop signal handlers")?;
    let listener = TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("failed to bind {bind_addr}"))?;
    let local_addr = listener
        .local_addr()
        .context("failed to read the bound address")?;

    tracing::info!(
        addr = %local_addr,
        stop_on = STOP_SIGNALS,
        "starting Restate szamlazz.hu endpoint"
    );

    HttpServer::new(endpoint)
        .serve_with_cancel(listener, stop)
        .await;

    tracing::info!("stopped Restate szamlazz.hu endpoint");

    Ok(())
}

/// A future that completes on the first stop signal, `SIGTERM` (what
/// `docker stop`, a Kubernetes rollout and `kill` send) or `SIGINT`
/// (Ctrl-C), and logs which one arrived. The signal handlers are installed
/// when this is called, so call it before anything a stop should interrupt:
/// the SDK's `serve` waits for `SIGINT` alone, and a `SIGTERM` nobody handles
/// ends the process on the spot, without the SDK's graceful drain.
#[cfg(unix)]
fn stop_signal() -> io::Result<impl Future<Output = ()>> {
    use tokio::signal::unix::{SignalKind, signal};

    let mut terminate = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;

    Ok(async move {
        let arrived = tokio::select! {
            _ = terminate.recv() => "SIGTERM",
            _ = interrupt.recv() => "SIGINT",
        };
        tracing::info!(signal = arrived, "stopping: draining open connections");
    })
}

/// A future that completes on Ctrl-C, the one stop signal Windows has; the
/// handler is installed when this is called, as in the Unix variant.
#[cfg(windows)]
fn stop_signal() -> io::Result<impl Future<Output = ()>> {
    let mut ctrl_c = tokio::signal::windows::ctrl_c()?;

    Ok(async move {
        ctrl_c.recv().await;
        tracing::info!(signal = "Ctrl-C", "stopping: draining open connections");
    })
}

/// Wires the configuration into the two services and binds them to one
/// endpoint: the static resolver over `[account]` or `[accounts.<scope>]` is
/// the `Accounts` bundle both services hold beside the deployment-level
/// `WorkerConfig`. Logs what was bound (the namespace, the shape, each
/// resolved account's scope, id and endpoint, and whether request identity
/// verification is on), never an agent key. `bind_addr` is
/// the address the endpoint will listen on (or would, under
/// `--check-config`): what the warning about accepting unsigned requests
/// names as reachable.
fn build_endpoint(config: EndpointConfig, bind_addr: SocketAddr) -> Result<Endpoint> {
    let EndpointConfig {
        worker,
        accounts,
        request_identity,
    } = config;

    let resolver = StaticResolver::try_from(accounts).context("invalid account configuration")?;
    tracing::info!(
        namespace = %worker.namespace,
        scoped = resolver.is_scoped(),
        accounts = resolver.accounts().count(),
        "loaded szamlazz.hu account configuration"
    );
    for (scope, account) in resolver.accounts() {
        tracing::info!(
            scope = scope.unwrap_or("<unscoped>"),
            account = %account.id,
            endpoint = %account.endpoint,
            "szamlazz.hu account"
        );
        // Allowed (a mock or a proxy is a legitimate target), but the agent
        // key travels in the request body, so say so (#65).
        if account.endpoint.is_cleartext() {
            tracing::warn!(
                scope = scope.unwrap_or("<unscoped>"),
                account = %account.id,
                endpoint = %account.endpoint,
                "the endpoint is plain http on a host other than loopback: the agent key is sent in cleartext"
            );
        }
    }

    let accounts = Accounts::from(resolver);
    let order = Order::from_parts(accounts.clone(), worker.clone());
    let agent = Agent::from_parts(accounts, worker);

    for discovery in [
        <Order as Discoverable>::discover(),
        <Agent as Discoverable>::discover(),
    ] {
        tracing::info!(
            service = %*discovery.name,
            kind = ?discovery.ty,
            handlers = discovery.handlers.len(),
            "bound Restate service"
        );
    }

    let mut endpoint = Endpoint::builder().bind(order).bind(agent);
    match request_identity {
        RequestIdentity::Verified(identity_keys) => {
            for identity_key in &identity_keys {
                endpoint = endpoint
                    .identity_key(identity_key)
                    .with_context(|| format!("invalid Restate identity key `{identity_key}`"))?;
            }
            tracing::info!(
                keys = identity_keys.len(),
                "request identity verification enabled"
            );
        }
        RequestIdentity::Unsigned { deliberate: true } => {
            tracing::info!(
                "request identity verification disabled: accepting unsigned requests (identity_keys = [])"
            );
        }
        // The scope travels inside the request, so without keys nothing
        // stands between the port and either service on any account; the
        // operator who did not write `identity_keys = []` may not know (#96).
        RequestIdentity::Unsigned { deliberate: false } => {
            let reachable = reachable_at(bind_addr);
            tracing::warn!(
                "request identity verification disabled: accepting unsigned requests; any client reaching {reachable} can invoke the services under any scope; set `identity_keys` to the Restate server's request identity public keys (README: Request Identity), or write `identity_keys = []` out to accept this for local development"
            );
        }
    }

    Ok(endpoint.build())
}

/// Where the unsigned-requests warning says the endpoint is reachable: the
/// address as configured, or, under `--port 0`, the address alone, since
/// the kernel picks the port at bind time and nothing is known of it before
/// (nor ever under `--check-config`, which never binds); `:0` would name an
/// address no client can reach.
fn reachable_at(bind_addr: SocketAddr) -> String {
    if bind_addr.port() == 0 {
        format!(
            "{} on the ephemeral port bound at start (--port 0; the start-up line names it)",
            bind_addr.ip()
        )
    } else {
        bind_addr.to_string()
    }
}

#[cfg(test)]
#[allow(
    clippy::result_large_err,
    reason = "`figment::Jail::expect_with` dictates the closure's `figment::Error` return type"
)]
mod tests {
    use figment::Figment;
    use figment::providers::{Format, Toml};
    use restate_szamlazz::Gateway;
    use restate_szamlazz::contract::Selector;
    use restate_szamlazz::gateway::QueryOutcome;
    use restate_szamlazz::szamlazz_agent::client::REQUEST_TIMEOUT;
    use restate_szamlazz::szamlazz_agent::reqwest;
    use wiremock::matchers::{body_string_contains, method};
    use wiremock::{Mock, MockBuilder, MockServer, ResponseTemplate};

    use super::*;

    /// A loaded configuration for the test account at `endpoint` with `extra`
    /// top-level keys.
    fn config(extra: &str, endpoint: &str, agent_key: &str) -> EndpointConfig {
        EndpointConfig::load(&Figment::from(Toml::string(&format!(
            r#"
            {extra}
            namespace = "acct"

            [account]
            id = "acme"
            agent_key = "{agent_key}"
            endpoint = "{endpoint}"
            "#
        ))))
        .expect("configuration should load")
    }

    /// The default bind address, as the start-up warning would name it.
    const BIND_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 9080);

    #[test]
    fn builds_the_endpoint_from_a_loaded_config() {
        build_endpoint(config("", "http://127.0.0.1:1/", "agent-key"), BIND_ADDR)
            .expect("endpoint should build");
    }

    #[test]
    fn accepts_identity_keys() {
        build_endpoint(
            config(
                r#"identity_keys = ["publickeyv1_w7YHemBctH5Ck2nQRQ47iBBqhNHy4FV7t2Usbye2A6f", "publickeyv1_ChjENKeMvCtRnqG2mrBK1HmPKufgFUc98K8B3ononQvp"]"#,
                "http://127.0.0.1:1/",
                "agent-key",
            ),
            BIND_ADDR,
        )
        .expect("endpoint should build with identity keys");
    }

    #[test]
    fn rejects_an_invalid_identity_key() {
        let Err(error) = build_endpoint(
            config(
                r#"identity_keys = ["not-a-key"]"#,
                "http://127.0.0.1:1/",
                "agent-key",
            ),
            BIND_ADDR,
        ) else {
            panic!("an invalid identity key should fail the build");
        };

        assert!(
            error.to_string().contains("not-a-key"),
            "the error should name the key: {error}"
        );
    }

    /// The account's own invariants (here a blank agent key) are checked
    /// when the static resolver is built, before anything is bound.
    #[test]
    fn rejects_an_invalid_account() {
        let Err(error) = build_endpoint(config("", "http://127.0.0.1:1/", " "), BIND_ADDR) else {
            panic!("a blank agent key should fail the build");
        };
        let message = format!("{error:#}");
        assert!(
            message.contains("invalid account configuration"),
            "{message}"
        );
        assert!(message.contains("agent_key must not be empty"), "{message}");
        assert!(
            message.contains("acme"),
            "the error names the account: {message}"
        );
    }

    #[test]
    fn missing_config_file_is_reported() {
        let cli = Cli {
            config: Some(PathBuf::from("/nonexistent/restate-szamlazz.toml")),
            bind: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            port: 9080,
            check_config: false,
        };

        let error = cli.load_config().expect_err("a missing file should fail");
        assert!(
            error.to_string().contains("config file not found"),
            "{error}"
        );
    }

    fn query() -> MockBuilder {
        Mock::given(method("POST")).and(body_string_contains("action-szamla_agent_xml"))
    }

    fn not_found() -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_raw(
            r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod><![CDATA[7]]></hibakod><hibauzenet><![CDATA[Hiányzó adat]]></hibauzenet></xmlszamlavalasz>"#,
            "application/xml",
        )
    }

    /// The configured endpoint and agent key are what the gateway opened by
    /// every handler's prologue speaks with.
    #[tokio::test]
    async fn configured_endpoint_and_key_reach_the_gateway() {
        let server = MockServer::start().await;
        query()
            .and(body_string_contains(
                "<szamlaagentkulcs>agent-key</szamlaagentkulcs>",
            ))
            .respond_with(not_found())
            .expect(1)
            .mount(&server)
            .await;

        let config = config("", &format!("{}/", server.uri()), "agent-key");
        let outcome = gateway(config)
            .await
            .query(&Selector::InvoiceNumber(
                "SZ-1".parse().expect("valid number"),
            ))
            .await;

        assert_eq!(outcome, Ok(QueryOutcome::NotFound));
    }

    /// An agent key given through the environment reaches szamlazz.hu exactly
    /// as written: an all-digit key with a leading zero included, which a
    /// value parsed as a number would lose.
    #[tokio::test]
    async fn an_all_digit_agent_key_from_the_environment_reaches_the_gateway_byte_exact() {
        const KEY: &str = "0071234";
        let server = MockServer::start().await;
        query()
            .and(body_string_contains(format!(
                "<szamlaagentkulcs>{KEY}</szamlaagentkulcs>"
            )))
            .respond_with(not_found())
            .expect(1)
            .mount(&server)
            .await;

        let mut loaded = None;
        figment::Jail::expect_with(|jail| {
            jail.create_file(
                "restate-szamlazz.toml",
                &format!(
                    r#"
                    namespace = "acct"

                    [account]
                    id = "acme"
                    endpoint = "{}/"
                    "#,
                    server.uri()
                ),
            )?;
            jail.set_env("RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY", KEY);
            let figment = config::figment(Some(std::path::Path::new("restate-szamlazz.toml")))
                .expect("the sources should assemble");
            loaded = Some(EndpointConfig::load(&figment).expect("configuration should load"));
            Ok(())
        });
        let config = loaded.expect("the configuration was loaded inside the jail");

        let outcome = gateway(config)
            .await
            .query(&Selector::InvoiceNumber(
                "SZ-1".parse().expect("valid number"),
            ))
            .await;

        assert_eq!(outcome, Ok(QueryOutcome::NotFound));
    }

    /// What every handler's prologue does with the loaded accounts: resolve
    /// the unscoped account, fetch its credentials, open the gateway. Opened
    /// over an HTTP client with no root certificates, so the test never parses
    /// the system CA store for a plain-`http://` wiremock (#136); the
    /// prologue's own `Gateway::open` is exercised by the e2e suite.
    async fn gateway(config: EndpointConfig) -> Gateway {
        let accounts = Accounts::from(
            StaticResolver::try_from(config.accounts).expect("accounts should build"),
        );
        let account = accounts.resolve(None).await.expect("the unscoped account");
        let credentials = accounts.fetch(&account).await.expect("its credentials");
        let http = reqwest::Client::builder()
            .tls_certs_only(std::iter::empty())
            .cookie_store(true)
            .timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("http client should build");
        Gateway::open_with_http(account, credentials, http).expect("gateway should build")
    }
}
