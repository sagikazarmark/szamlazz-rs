//! Standalone endpoint hosting the szamlazz.hu services for Restate.
//!
//! Binds the `Szamlazz.Order` Virtual Object and the `Szamlazz.Agent` service
//! of [`restate_szamlazz`] to one HTTP/2 endpoint and serves it for a Restate
//! server to register.

mod config;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as _, Result, bail};
use clap::Parser;
use figment::Figment;
use figment::providers::{Env, Format, Json, Toml, Yaml};
use restate_sdk::endpoint::Endpoint;
use restate_sdk::http_server::HttpServer;
use restate_sdk::service::Discoverable;
use restate_szamlazz::{Agent, Gateway, Order, WorkerConfig};
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

        let config: EndpointConfig = figment.extract().context("failed to parse configuration")?;
        config.service.validate().context("invalid configuration")?;
        Ok(config)
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
/// endpoint: the account opens the gateway, the rest is the services'
/// [`WorkerConfig`]. Logs what was bound; never the agent key.
fn build_endpoint(config: EndpointConfig) -> Result<Endpoint> {
    let EndpointConfig {
        service: config,
        identity_keys,
    } = config;

    tracing::info!(
        namespace = %config.account.slug,
        mode = ?config.account.mode,
        endpoint = ?config.account.endpoint,
        supplier_id = ?config.account.supplier_id,
        "loaded szamlazz.hu account configuration"
    );

    let gateway = Arc::new(Gateway::new(&config).context("failed to open the gateway")?);
    let worker = WorkerConfig::from(&config);
    let order = Order::from_parts(Arc::clone(&gateway), worker.clone());
    let agent = Agent::from_parts(gateway, worker);

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
    use restate_szamlazz::contract::Selector;
    use restate_szamlazz::gateway::QueryOutcome;
    use wiremock::matchers::{body_string_contains, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn config(extra: &str) -> EndpointConfig {
        Figment::from(Toml::string(&format!(
            r#"
            {extra}

            [account]
            slug = "acct"
            agent_key = "agent-key"
            mode = "test"
            endpoint = "http://127.0.0.1:1/"
            "#
        )))
        .extract()
        .expect("configuration should parse")
    }

    #[test]
    fn builds_the_endpoint_from_a_parsed_config() {
        build_endpoint(config("")).expect("endpoint should build");
    }

    #[test]
    fn accepts_identity_keys() {
        build_endpoint(config(
            r#"identity_keys = ["publickeyv1_w7YHemBctH5Ck2nQRQ47iBBqhNHy4FV7t2Usbye2A6f", "publickeyv1_ChjENKeMvCtRnqG2mrBK1HmPKufgFUc98K8B3ononQvp"]"#,
        ))
        .expect("endpoint should build with identity keys");
    }

    #[test]
    fn rejects_an_invalid_identity_key() {
        let Err(error) = build_endpoint(config(r#"identity_keys = ["not-a-key"]"#)) else {
            panic!("an invalid identity key should fail the build");
        };

        assert!(
            error.to_string().contains("not-a-key"),
            "the error should name the key: {error}"
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

    #[tokio::test]
    async fn config_endpoint_reaches_the_gateway() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(body_string_contains("action-szamla_agent_xml"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod><![CDATA[7]]></hibakod><hibauzenet><![CDATA[Hiányzó adat]]></hibauzenet></xmlszamlavalasz>"#,
                "application/xml",
            ))
            .expect(1)
            .mount(&server)
            .await;

        let config: EndpointConfig = Figment::from(Toml::string(&format!(
            r#"
            [account]
            slug = "acct"
            agent_key = "agent-key"
            mode = "test"
            endpoint = "{}/"
            "#,
            server.uri()
        )))
        .extract()
        .expect("configuration should parse");
        let gateway = Gateway::new(&config.service).expect("gateway should build");

        let outcome = gateway
            .query(&Selector::InvoiceNumber("SZ-1".to_owned()))
            .await;

        assert_eq!(outcome, QueryOutcome::NotFound);
    }
}
