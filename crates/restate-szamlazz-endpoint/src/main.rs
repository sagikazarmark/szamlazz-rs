//! Standalone endpoint hosting the szamlazz.hu services for Restate.
//!
//! Binds the `Szamlazz.Order` Virtual Object and the `Szamlazz.Agent` service
//! of [`restate_szamlazz`] to one HTTP/2 endpoint and serves it for a Restate
//! server to register.

mod config;

use std::path::PathBuf;

use anyhow::{Context as _, Result, bail};
use clap::Parser;
use figment::Figment;
use figment::providers::{Env, Format, Json, Toml, Yaml};
use restate_sdk::endpoint::Endpoint;
use restate_sdk::http_server::HttpServer;
use restate_sdk::service::Discoverable;
use restate_szamlazz::account::StaticResolver;
use restate_szamlazz::{Accounts, Agent, Order};
use tracing_subscriber::EnvFilter;

use crate::config::EndpointConfig;

/// The environment prefix of configuration overrides; `__` nests
/// (`RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY` → `account.agent_key`).
const ENV_PREFIX: &str = "RESTATE_SZAMLAZZ_";

#[derive(Parser, Debug)]
#[command(version)]
struct Cli {
    /// Path to config file (supports JSON, YAML, or TOML).
    #[arg(long, value_name = "FILE", env = "CONFIG_FILE")]
    config: Option<PathBuf>,

    /// Port to listen on.
    #[arg(long, default_value = "9080", env = "PORT")]
    port: u16,
}

impl Cli {
    fn load_config(&self) -> Result<EndpointConfig> {
        let mut figment = Figment::new();

        if let Some(path) = self.config.as_deref() {
            if !path.exists() {
                bail!("config file not found: {}", path.display());
            }

            figment = match path.extension().and_then(|extension| extension.to_str()) {
                Some("toml") => figment.merge(Toml::file(path)),
                Some("json") => figment.merge(Json::file(path)),
                Some("yaml" | "yml") => figment.merge(Yaml::file(path)),
                _ => bail!("unsupported config file format; use .toml, .json, .yaml, or .yml"),
            };
        }

        figment = figment.merge(Env::prefixed(ENV_PREFIX).split("__"));

        EndpointConfig::load(&figment)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let config = cli.load_config()?;
    let endpoint = build_endpoint(config)?;
    let bind_addr = format!("0.0.0.0:{}", cli.port);

    tracing::info!(%bind_addr, "starting Restate szamlazz.hu endpoint");

    HttpServer::new(endpoint)
        .listen_and_serve(bind_addr.parse()?)
        .await;

    Ok(())
}

/// Wires the configuration into the two services and binds them to one
/// endpoint: the static resolver over `[account]` or `[accounts.<scope>]` is
/// the `Accounts` bundle both services hold beside the deployment-level
/// `WorkerConfig`. Logs what was bound — the namespace, the shape, and each
/// resolved account's scope, id, mode, endpoint and supplier pin — never an
/// agent key.
fn build_endpoint(config: EndpointConfig) -> Result<Endpoint> {
    let EndpointConfig {
        worker,
        accounts,
        identity_keys,
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
            mode = ?account.mode,
            endpoint = %account.endpoint,
            supplier_id = ?account.supplier_id,
            "szamlazz.hu account"
        );
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
    for identity_key in &identity_keys {
        endpoint = endpoint
            .identity_key(identity_key)
            .with_context(|| format!("invalid Restate identity key `{identity_key}`"))?;
    }
    if !identity_keys.is_empty() {
        tracing::info!(
            keys = identity_keys.len(),
            "request identity verification enabled"
        );
    }

    Ok(endpoint.build())
}

#[cfg(test)]
mod tests {
    use figment::Figment;
    use figment::providers::{Format, Toml};
    use restate_szamlazz::Gateway;
    use restate_szamlazz::contract::Selector;
    use restate_szamlazz::gateway::QueryOutcome;
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
            mode = "test"
            endpoint = "{endpoint}"
            "#
        ))))
        .expect("configuration should load")
    }

    #[test]
    fn builds_the_endpoint_from_a_loaded_config() {
        build_endpoint(config("", "http://127.0.0.1:1/", "agent-key"))
            .expect("endpoint should build");
    }

    #[test]
    fn accepts_identity_keys() {
        build_endpoint(config(
            r#"identity_keys = ["publickeyv1_w7YHemBctH5Ck2nQRQ47iBBqhNHy4FV7t2Usbye2A6f", "publickeyv1_ChjENKeMvCtRnqG2mrBK1HmPKufgFUc98K8B3ononQvp"]"#,
            "http://127.0.0.1:1/",
            "agent-key",
        ))
        .expect("endpoint should build with identity keys");
    }

    #[test]
    fn rejects_an_invalid_identity_key() {
        let Err(error) = build_endpoint(config(
            r#"identity_keys = ["not-a-key"]"#,
            "http://127.0.0.1:1/",
            "agent-key",
        )) else {
            panic!("an invalid identity key should fail the build");
        };

        assert!(
            error.to_string().contains("not-a-key"),
            "the error should name the key: {error}"
        );
    }

    /// The account's own invariants — here a blank agent key — are checked
    /// when the static resolver is built, before anything is bound.
    #[test]
    fn rejects_an_invalid_account() {
        let Err(error) = build_endpoint(config("", "http://127.0.0.1:1/", " ")) else {
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
            port: 9080,
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
        // What every handler's prologue does: resolve, fetch, open.
        let accounts = Accounts::from(
            StaticResolver::try_from(config.accounts).expect("accounts should build"),
        );
        let account = accounts.resolve(None).await.expect("the unscoped account");
        let credentials = accounts.fetch(&account).await.expect("its credentials");
        let gateway = Gateway::open(account, credentials).expect("gateway should build");

        let outcome = gateway
            .query(&Selector::InvoiceNumber("SZ-1".to_owned()))
            .await;

        assert_eq!(outcome, QueryOutcome::NotFound);
    }
}
