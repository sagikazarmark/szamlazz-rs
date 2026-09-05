//! Endpoint configuration: the service [`Config`](restate_szamlazz::Config)
//! plus what only the hosting process cares about (request identity keys).

use serde::Deserialize;

/// The complete endpoint configuration.
///
/// The service configuration is flattened, so the file layout is exactly the
/// one documented in `restate_szamlazz::config` (`[account]`, `[defaults]`,
/// `[seller]`, `[issue]`) with `identity_keys` at the top level.
#[derive(Debug, Clone, Deserialize)]
pub struct EndpointConfig {
    /// The deployment configuration of the services.
    #[serde(flatten)]
    pub service: restate_szamlazz::Config,
    /// Restate request identity public keys (`publickeyv1_...`).
    ///
    /// With at least one key configured the endpoint rejects unsigned
    /// requests. Listing the old and the new key keeps both valid during
    /// rotation. Accepts a list or a comma/whitespace-delimited string, so
    /// the `RESTATE_SZAMLAZZ_IDENTITY_KEYS` environment override stays a
    /// plain string.
    #[serde(default, deserialize_with = "identity_keys")]
    pub identity_keys: Vec<String>,
}

fn identity_keys<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum IdentityKeys {
        List(Vec<String>),
        Delimited(String),
    }

    Ok(match IdentityKeys::deserialize(deserializer)? {
        IdentityKeys::List(keys) => keys,
        IdentityKeys::Delimited(keys) => keys
            .split([',', ' ', '\t', '\n'])
            .filter(|key| !key.is_empty())
            .map(str::to_owned)
            .collect(),
    })
}

#[cfg(test)]
#[allow(
    clippy::result_large_err,
    reason = "`figment::Jail::expect_with` dictates the closure's `figment::Error` return type"
)]
mod tests {
    use std::time::Duration;

    use figment::Figment;
    use figment::Jail;
    use figment::providers::{Env, Format, Toml};
    use restate_szamlazz::config::{AccountMode, ConfigError};

    use super::*;

    /// The configuration example of design §9.
    const SPEC_EXAMPLE: &str = r#"
        [account]
        slug = "acct"
        agent_key = "agent-key"
        endpoint = "https://www.szamlazz.hu/szamla/"
        mode = "live"
        supplier_id = 972720

        [defaults]
        e_invoice = false
        language = "hu"
        currency = "HUF"
        exchange_rate_bank = "MNB"
        template = "default"
        send_email = false
        number_prefix = "WEB"
        extra_logo = "logo"
        aggregator = "agg"
        guardian = false

        [seller]
        bank = "Bank"
        bank_account = "1234-5678"
        signer_name = "Signer"
        [seller.email]
        reply_to = "billing@example.com"
        subject = "Your invoice"
        body = "Thank you"

        [issue]
        max_attempts = 5
        initial_delay = "2m"
        factor = 2.0
        max_delay = "10m"
        max_duration = "1h"
    "#;

    fn minimal() -> &'static str {
        r#"
        [account]
        slug = "acct"
        agent_key = "agent-key"
        "#
    }

    #[test]
    fn parses_the_spec_example() {
        let config: EndpointConfig = Figment::from(Toml::string(SPEC_EXAMPLE))
            .extract()
            .expect("configuration should parse");
        let service = &config.service;

        assert_eq!(service.account.slug.as_str(), "acct");
        assert_eq!(service.account.agent_key.expose(), "agent-key");
        assert_eq!(
            service.account.endpoint.as_deref(),
            Some("https://www.szamlazz.hu/szamla/")
        );
        assert_eq!(service.account.mode, AccountMode::Live);
        assert_eq!(service.account.supplier_id, Some(972_720));
        assert!(!service.defaults.e_invoice);
        assert_eq!(service.defaults.language, "hu");
        assert_eq!(service.defaults.currency, "HUF");
        assert_eq!(service.defaults.exchange_rate_bank, "MNB");
        assert_eq!(service.defaults.template.as_deref(), Some("default"));
        assert_eq!(service.defaults.send_email, Some(false));
        assert_eq!(service.defaults.number_prefix.as_deref(), Some("WEB"));
        assert_eq!(service.defaults.extra_logo.as_deref(), Some("logo"));
        assert_eq!(service.defaults.aggregator.as_deref(), Some("agg"));
        assert_eq!(service.defaults.guardian, Some(false));
        assert_eq!(service.seller.bank.as_deref(), Some("Bank"));
        assert_eq!(service.seller.bank_account.as_deref(), Some("1234-5678"));
        assert_eq!(service.seller.signer_name.as_deref(), Some("Signer"));
        assert_eq!(
            service.seller.email.reply_to.as_deref(),
            Some("billing@example.com")
        );
        assert_eq!(
            service.seller.email.subject.as_deref(),
            Some("Your invoice")
        );
        assert_eq!(service.seller.email.body.as_deref(), Some("Thank you"));
        assert_eq!(service.issue.max_attempts, 5);
        assert_eq!(service.issue.initial_delay, Duration::from_secs(120));
        assert_eq!(service.issue.factor.to_bits(), 2.0f32.to_bits());
        assert_eq!(service.issue.max_delay, Duration::from_secs(600));
        assert_eq!(service.issue.max_duration, Duration::from_secs(3600));
        assert!(config.identity_keys.is_empty());
        service.validate().expect("the spec example is valid");
    }

    #[test]
    fn environment_overrides_the_agent_key() {
        Jail::expect_with(|jail| {
            jail.set_env("RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY", "from-env");
            jail.set_env("RESTATE_SZAMLAZZ_ACCOUNT__MODE", "test");

            let config: EndpointConfig = Figment::from(Toml::string(SPEC_EXAMPLE))
                .merge(Env::prefixed("RESTATE_SZAMLAZZ_").split("__"))
                .extract()?;

            assert_eq!(config.service.account.agent_key.expose(), "from-env");
            assert_eq!(config.service.account.mode, AccountMode::Test);
            // Untouched values survive the merge.
            assert_eq!(config.service.account.slug.as_str(), "acct");
            Ok(())
        });
    }

    #[test]
    fn parses_identity_keys_from_list_and_delimited_string() {
        let list: EndpointConfig = Figment::from(Toml::string(&format!(
            "identity_keys = [\"publickeyv1_old\", \"publickeyv1_new\"]\n{}",
            minimal()
        )))
        .extract()
        .expect("configuration should parse");
        let delimited: EndpointConfig = Figment::from(Toml::string(&format!(
            "identity_keys = \"publickeyv1_old, publickeyv1_new\"\n{}",
            minimal()
        )))
        .extract()
        .expect("configuration should parse");

        assert_eq!(list.identity_keys, ["publickeyv1_old", "publickeyv1_new"]);
        assert_eq!(delimited.identity_keys, list.identity_keys);
    }

    #[test]
    fn identity_keys_from_environment_are_a_delimited_string() {
        Jail::expect_with(|jail| {
            jail.set_env(
                "RESTATE_SZAMLAZZ_IDENTITY_KEYS",
                "publickeyv1_old,publickeyv1_new",
            );

            let config: EndpointConfig = Figment::from(Toml::string(minimal()))
                .merge(Env::prefixed("RESTATE_SZAMLAZZ_").split("__"))
                .extract()?;

            assert_eq!(config.identity_keys, ["publickeyv1_old", "publickeyv1_new"]);
            Ok(())
        });
    }

    #[test]
    fn identity_keys_default_to_empty() {
        let config: EndpointConfig = Figment::from(Toml::string(minimal()))
            .extract()
            .expect("configuration should parse");

        assert!(config.identity_keys.is_empty());
    }

    #[test]
    fn missing_account_fails_to_parse() {
        let result = Figment::from(Toml::string("identity_keys = []")).extract::<EndpointConfig>();

        assert!(result.is_err(), "the account section is required");
    }

    #[test]
    fn validation_failures_surface() {
        let config: EndpointConfig = Figment::from(Toml::string(
            r#"
            [account]
            slug = "acct"
            agent_key = " "
            "#,
        ))
        .extract()
        .expect("configuration should parse");
        assert_eq!(config.service.validate(), Err(ConfigError::EmptyAgentKey));

        let config: EndpointConfig = Figment::from(Toml::string(
            r#"
            [account]
            slug = "acct"
            agent_key = "agent-key"

            [issue]
            initial_delay = "11m"
            "#,
        ))
        .extract()
        .expect("configuration should parse");
        assert_eq!(
            config.service.validate(),
            Err(ConfigError::DelayOrder {
                initial: Duration::from_mins(11),
                max: Duration::from_mins(10),
            })
        );
    }
}
