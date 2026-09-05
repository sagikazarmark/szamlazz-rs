//! Endpoint configuration: the deployment-level
//! [`WorkerConfig`](restate_szamlazz::WorkerConfig) (`namespace`, `[issue]`,
//! `[resolve]`), the static resolver's accounts
//! ([`StaticConfig`](restate_szamlazz::account::StaticConfig): `[account]`)
//! and what only the hosting process cares about (request identity keys).

use anyhow::{Context as _, Result, bail};
use figment::Figment;
use restate_szamlazz::WorkerConfig;
use restate_szamlazz::account::StaticConfig;
use serde::Deserialize;

/// The complete endpoint configuration.
///
/// Both library configurations are flattened, so the file layout is
/// `namespace`, `[issue]`, `[resolve]` and `[account]` (with its
/// `[account.defaults]` and `[account.seller]`) plus `identity_keys`, all at
/// the top level. Load it with [`EndpointConfig::load`].
#[derive(Debug, Clone, Deserialize)]
pub struct EndpointConfig {
    /// The deployment-level settings of the services.
    #[serde(flatten)]
    pub worker: WorkerConfig,
    /// The accounts of the static resolver.
    #[serde(flatten)]
    pub accounts: StaticConfig,
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

impl EndpointConfig {
    /// Extracts the configuration from `figment` and validates the
    /// deployment-level invariants.
    ///
    /// # Errors
    ///
    /// Returns an error when the figment does not parse, when it uses the
    /// pre-release layout (top-level `[defaults]` / `[seller]`, or
    /// `account.slug` instead of `namespace`) — named explicitly, since serde
    /// would otherwise ignore the moved keys — or when
    /// [`WorkerConfig::validate`] fails. The accounts are validated when the
    /// static resolver is built.
    pub fn load(figment: &Figment) -> Result<Self> {
        let legacy: LegacyLayout = figment.extract().context("failed to parse configuration")?;
        legacy.refuse()?;
        let config: Self = figment.extract().context("failed to parse configuration")?;
        config.worker.validate().context("invalid configuration")?;
        Ok(config)
    }
}

/// The keys of the pre-release layout, which the current shape would silently
/// ignore: the namespace was `account.slug`, and the document defaults and the
/// seller block were top-level tables rather than part of `[account]`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LegacyLayout {
    defaults: Option<figment::value::Value>,
    seller: Option<figment::value::Value>,
    account: LegacyAccount,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LegacyAccount {
    slug: Option<figment::value::Value>,
}

impl LegacyLayout {
    /// Fails with a message naming every moved key that is present.
    fn refuse(&self) -> Result<()> {
        let mut moved = Vec::new();
        if self.account.slug.is_some() {
            moved.push("`account.slug` is now the top-level `namespace`");
        }
        if self.defaults.is_some() {
            moved.push("`[defaults]` is now `[account.defaults]`");
        }
        if self.seller.is_some() {
            moved.push("`[seller]` is now `[account.seller]`");
        }
        if moved.is_empty() {
            return Ok(());
        }
        bail!(
            "the configuration uses the pre-release layout: {}; see the restate-szamlazz-endpoint README",
            moved.join("; ")
        );
    }
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

    use figment::Jail;
    use figment::providers::{Env, Format, Toml};
    use restate_szamlazz::config::{AccountMode, IssueConfig, ResolveConfig};

    use super::*;

    /// The configuration example of design §9.
    const SPEC_EXAMPLE: &str = r#"
        identity_keys = []
        namespace = "acct"

        [issue]
        max_attempts = 5
        initial_delay = "2m"
        factor = 2.0
        max_delay = "10m"
        max_duration = "1h"

        [resolve]
        initial_delay = "1s"
        factor = 2.0
        max_delay = "10s"
        max_duration = "1m"

        [account]
        id = "acme"
        agent_key = "agent-key"
        endpoint = "https://www.szamlazz.hu/szamla/"
        mode = "live"
        supplier_id = 972720

        [account.defaults]
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

        [account.seller]
        bank = "Bank"
        bank_account = "1234-5678"
        signer_name = "Signer"
        [account.seller.email]
        reply_to = "billing@example.com"
        subject = "Your invoice"
        body = "Thank you"
    "#;

    fn minimal() -> &'static str {
        r#"
        namespace = "acct"

        [account]
        id = "acme"
        agent_key = "agent-key"
        "#
    }

    fn load(toml: &str) -> Result<EndpointConfig> {
        EndpointConfig::load(&Figment::from(Toml::string(toml)))
    }

    #[test]
    fn parses_the_spec_example() {
        let config = load(SPEC_EXAMPLE).expect("configuration should load");

        assert_eq!(config.worker.namespace.as_str(), "acct");
        assert_eq!(config.worker.issue.max_attempts, 5);
        assert_eq!(config.worker.issue.initial_delay, Duration::from_secs(120));
        assert_eq!(config.worker.issue.factor.to_bits(), 2.0f32.to_bits());
        assert_eq!(config.worker.issue.max_delay, Duration::from_secs(600));
        assert_eq!(config.worker.issue.max_duration, Duration::from_secs(3600));
        assert_eq!(config.worker.resolve.initial_delay, Duration::from_secs(1));
        assert_eq!(config.worker.resolve.max_delay, Duration::from_secs(10));
        assert_eq!(config.worker.resolve.max_duration, Duration::from_secs(60));

        let account = &config.accounts.account;
        assert_eq!(account.id.as_str(), "acme");
        assert_eq!(account.agent_key.expose(), "agent-key");
        assert_eq!(
            account.endpoint.as_deref(),
            Some("https://www.szamlazz.hu/szamla/")
        );
        assert_eq!(account.mode, AccountMode::Live);
        assert_eq!(account.supplier_id, Some(972_720));
        assert!(!account.defaults.e_invoice);
        assert_eq!(account.defaults.language, "hu");
        assert_eq!(account.defaults.currency, "HUF");
        assert_eq!(account.defaults.exchange_rate_bank, "MNB");
        assert_eq!(account.defaults.template.as_deref(), Some("default"));
        assert_eq!(account.defaults.send_email, Some(false));
        assert_eq!(account.defaults.number_prefix.as_deref(), Some("WEB"));
        assert_eq!(account.defaults.extra_logo.as_deref(), Some("logo"));
        assert_eq!(account.defaults.aggregator.as_deref(), Some("agg"));
        assert_eq!(account.defaults.guardian, Some(false));
        assert_eq!(account.seller.bank.as_deref(), Some("Bank"));
        assert_eq!(account.seller.bank_account.as_deref(), Some("1234-5678"));
        assert_eq!(account.seller.signer_name.as_deref(), Some("Signer"));
        assert_eq!(
            account.seller.email.reply_to.as_deref(),
            Some("billing@example.com")
        );
        assert_eq!(
            account.seller.email.subject.as_deref(),
            Some("Your invoice")
        );
        assert_eq!(account.seller.email.body.as_deref(), Some("Thank you"));
        assert!(config.identity_keys.is_empty());
    }

    /// `namespace` and `[account]` (`id`, `agent_key`) are the only required
    /// keys; the policies, the endpoint, the mode, the defaults and the
    /// seller block have defaults.
    #[test]
    fn minimal_configuration_takes_the_defaults() {
        let config = load(minimal()).expect("configuration should load");

        assert_eq!(config.worker.namespace.as_str(), "acct");
        assert_eq!(config.worker.issue, IssueConfig::default());
        assert_eq!(config.worker.resolve, ResolveConfig::default());
        assert_eq!(config.accounts.account.id.as_str(), "acme");
        assert_eq!(config.accounts.account.endpoint, None);
        assert_eq!(config.accounts.account.mode, AccountMode::Live);
        assert_eq!(config.accounts.account.supplier_id, None);
        assert_eq!(config.accounts.account.defaults.currency, "HUF");
        assert_eq!(config.accounts.account.seller.bank_account, None);
        assert!(config.identity_keys.is_empty());
    }

    /// Environment overrides address every level with `__`: the agent key
    /// and the mode under `[account]`, a document default under
    /// `[account.defaults]`, an issue-policy field and the namespace itself.
    #[test]
    fn environment_overrides_nest_with_double_underscores() {
        Jail::expect_with(|jail| {
            jail.set_env("RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY", "from-env");
            jail.set_env("RESTATE_SZAMLAZZ_ACCOUNT__MODE", "test");
            jail.set_env("RESTATE_SZAMLAZZ_ACCOUNT__DEFAULTS__CURRENCY", "EUR");
            jail.set_env("RESTATE_SZAMLAZZ_ISSUE__MAX_ATTEMPTS", "3");
            jail.set_env("RESTATE_SZAMLAZZ_NAMESPACE", "from-env");

            let config = EndpointConfig::load(
                &Figment::from(Toml::string(SPEC_EXAMPLE))
                    .merge(Env::prefixed("RESTATE_SZAMLAZZ_").split("__")),
            )
            .expect("configuration should load");

            assert_eq!(config.accounts.account.agent_key.expose(), "from-env");
            assert_eq!(config.accounts.account.mode, AccountMode::Test);
            assert_eq!(config.accounts.account.defaults.currency, "EUR");
            assert_eq!(config.worker.issue.max_attempts, 3);
            assert_eq!(config.worker.namespace.as_str(), "from-env");
            // Untouched values survive the merge.
            assert_eq!(config.accounts.account.id.as_str(), "acme");
            assert_eq!(config.accounts.account.defaults.language, "hu");
            assert_eq!(config.worker.issue.max_delay, Duration::from_secs(600));
            Ok(())
        });
    }

    #[test]
    fn parses_identity_keys_from_list_and_delimited_string() {
        let list = load(&format!(
            "identity_keys = [\"publickeyv1_old\", \"publickeyv1_new\"]\n{}",
            minimal()
        ))
        .expect("configuration should load");
        let delimited = load(&format!(
            "identity_keys = \"publickeyv1_old, publickeyv1_new\"\n{}",
            minimal()
        ))
        .expect("configuration should load");

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

            let config = EndpointConfig::load(
                &Figment::from(Toml::string(minimal()))
                    .merge(Env::prefixed("RESTATE_SZAMLAZZ_").split("__")),
            )
            .expect("configuration should load");

            assert_eq!(config.identity_keys, ["publickeyv1_old", "publickeyv1_new"]);
            Ok(())
        });
    }

    #[test]
    fn missing_namespace_or_account_fails_to_parse() {
        let error = load("[account]\nid = \"acme\"\nagent_key = \"k\"").expect_err("no namespace");
        assert!(
            format!("{error:#}").contains("namespace"),
            "the error names the missing key: {error:#}"
        );

        let error = load("namespace = \"acct\"").expect_err("no account");
        assert!(
            format!("{error:#}").contains("account"),
            "the error names the missing table: {error:#}"
        );
    }

    /// The pre-release layout — `account.slug`, top-level `[defaults]` and
    /// `[seller]` — is refused by name rather than silently ignored.
    #[test]
    fn pre_release_layout_fails_with_a_clear_error() {
        let error = load(
            r#"
            [account]
            slug = "acct"
            agent_key = "agent-key"

            [defaults]
            currency = "EUR"

            [seller]
            bank_account = "1234"

            [issue]
            max_attempts = 5
            "#,
        )
        .expect_err("the old layout must not load");
        let message = format!("{error:#}");
        assert!(message.contains("pre-release layout"), "{message}");
        assert!(message.contains("`account.slug`"), "{message}");
        assert!(message.contains("`namespace`"), "{message}");
        assert!(message.contains("`[account.defaults]`"), "{message}");
        assert!(message.contains("`[account.seller]`"), "{message}");

        // A single moved key is enough, and only it is named.
        let error = load(&format!("{}\n[seller]\nbank = \"B\"", minimal()))
            .expect_err("a top-level seller table must not load");
        let message = format!("{error:#}");
        assert!(message.contains("`[account.seller]`"), "{message}");
        assert!(!message.contains("`account.slug`"), "{message}");
        assert!(!message.contains("`[account.defaults]`"), "{message}");
    }

    #[test]
    fn deployment_level_validation_failures_surface() {
        let error = load(&format!("{}\n[issue]\ninitial_delay = \"11m\"", minimal()))
            .expect_err("an inverted issue delay order must not load");
        let message = format!("{error:#}");
        assert!(message.contains("invalid configuration"), "{message}");
        assert!(
            message.contains("issue.initial_delay (660s) must not exceed issue.max_delay (600s)"),
            "{message}"
        );

        let error = load(&format!("{}\n[resolve]\nfactor = 0.5", minimal()))
            .expect_err("a shrinking resolve factor must not load");
        assert!(
            format!("{error:#}").contains("resolve.factor (0.5)"),
            "{error:#}"
        );
    }
}
