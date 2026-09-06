//! Endpoint configuration: the deployment-level
//! [`WorkerConfig`](restate_szamlazz::WorkerConfig) (`namespace`, `[issue]`,
//! `[read]`, `[resolve]`), the static resolver's accounts
//! ([`StaticConfig`](restate_szamlazz::account::StaticConfig): `[account]` or
//! `[accounts.<scope>]`) and what only the hosting process cares about
//! (request identity keys). Strict: a key the configuration does not know, at
//! any level, is refused with its path and source ([`schema`]).

mod schema;
mod sources;

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context as _, Result, bail};
use figment::Figment;
use figment::providers::{Format, Json, Toml, Yaml};
use restate_szamlazz::WorkerConfig;
use restate_szamlazz::account::{StaticAccount, StaticConfig};
use restate_szamlazz::config::{IssueConfig, Namespace, ReadConfig, ResolveConfig};
use serde::Deserialize;

pub use sources::{EnvOverrides, PlainKeys};

/// The environment prefix of configuration overrides; `__` nests
/// (`RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY` → `account.agent_key`).
pub const ENV_PREFIX: &str = "RESTATE_SZAMLAZZ_";

/// The configuration sources: the file at `path`, if any — TOML, JSON or
/// YAML by extension — with the `RESTATE_SZAMLAZZ_` environment overrides
/// merged on top. Environment values are strings, read by the field's type
/// ([`EnvOverrides`]).
///
/// # Errors
///
/// Returns an error when the file does not exist or its extension is not
/// one of `.toml`, `.json`, `.yaml`, `.yml`. A file that exists but does not
/// parse is reported by [`EndpointConfig::load`].
pub fn figment(path: Option<&Path>) -> Result<Figment> {
    let mut figment = Figment::new();

    if let Some(path) = path {
        if !path.exists() {
            bail!("config file not found: {}", path.display());
        }

        figment = match path.extension().and_then(|extension| extension.to_str()) {
            Some("toml") => figment.merge(PlainKeys(Toml::file(path))),
            Some("json") => figment.merge(PlainKeys(Json::file(path))),
            Some("yaml" | "yml") => figment.merge(PlainKeys(Yaml::file(path))),
            _ => bail!("unsupported config file format; use .toml, .json, .yaml, or .yml"),
        };
    }

    Ok(figment.merge(EnvOverrides::new()))
}

/// The complete endpoint configuration.
///
/// The file layout is `namespace`, `[issue]`, `[read]`, `[resolve]`, either
/// `[account]` or a table of `[accounts.<scope>]` (each with its `defaults`
/// and `seller`), plus `identity_keys`, all at the top level (see
/// [`Layout`]). Load it with [`EndpointConfig::load`], which refuses unknown
/// keys and both shapes at once; the accounts' own rules (a non-blank id and
/// key, the multi-account uniqueness rules) are checked when the static
/// resolver is built from `accounts`.
#[derive(Debug, Clone)]
pub struct EndpointConfig {
    /// The deployment-level settings of the services.
    pub worker: WorkerConfig,
    /// The accounts of the static resolver.
    pub accounts: StaticConfig,
    /// Restate request identity public keys (`publickeyv1_...`).
    ///
    /// With at least one key configured the endpoint rejects unsigned
    /// requests. Listing the old and the new key keeps both valid during
    /// rotation. Accepts a list or a comma/whitespace-delimited string, so
    /// the `RESTATE_SZAMLAZZ_IDENTITY_KEYS` environment override stays a
    /// plain string.
    pub identity_keys: Vec<String>,
}

/// The file layout, one explicit field per top-level key. The library's
/// [`WorkerConfig`] and [`StaticConfig`] are assembled from it rather than
/// flattened into it, so a parse error keeps the key path and the source
/// figment attaches — `#[serde(flatten)]` deserializes through a buffer that
/// drops both.
#[derive(Debug, Deserialize)]
struct Layout {
    namespace: Namespace,
    #[serde(default)]
    issue: IssueConfig,
    #[serde(default)]
    read: ReadConfig,
    #[serde(default)]
    resolve: ResolveConfig,
    #[serde(default)]
    account: Option<StaticAccount>,
    #[serde(default)]
    accounts: BTreeMap<String, StaticAccount>,
    #[serde(default, deserialize_with = "identity_keys")]
    identity_keys: Vec<String>,
}

impl From<Layout> for EndpointConfig {
    fn from(layout: Layout) -> Self {
        let Layout {
            namespace,
            issue,
            read,
            resolve,
            account,
            accounts,
            identity_keys,
        } = layout;
        Self {
            worker: WorkerConfig {
                namespace,
                issue,
                read,
                resolve,
            },
            accounts: StaticConfig { account, accounts },
            identity_keys,
        }
    }
}

impl EndpointConfig {
    /// Extracts the configuration from `figment` and validates the
    /// deployment-level invariants.
    ///
    /// # Errors
    ///
    /// Returns an error when the figment holds a key the configuration does
    /// not know, at any level — every such key is named with its path and its
    /// source; the pre-release layout's moved keys (`account.slug`, top-level
    /// `[defaults]` / `[seller]`) with where they went — or both account
    /// shapes at once (each named with its source); when it does not parse;
    /// or when [`WorkerConfig::validate`] fails. The accounts themselves are
    /// validated when the static resolver is built.
    pub fn load(figment: &Figment) -> Result<Self> {
        schema::check(figment).context("invalid configuration")?;
        // Lossy: an environment value is a string, and the field's type
        // decides how it is read (`"3"` → `3` where a number is expected).
        let layout: Layout = figment
            .extract_lossy()
            .context("failed to parse configuration")?;
        let config = Self::from(layout);
        config.worker.validate().context("invalid configuration")?;
        Ok(config)
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
    use figment::providers::{Format, Toml};
    use restate_szamlazz::account::StaticResolver;
    use restate_szamlazz::config::{AccountMode, IssueConfig, ReadConfig, ResolveConfig};

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

        [read]
        max_attempts = 5
        initial_delay = "5s"
        factor = 2.0
        max_delay = "60s"
        max_duration = "5m"

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

    /// Loads `toml` as the binary would load a file: through [`PlainKeys`].
    fn load(toml: &str) -> Result<EndpointConfig> {
        EndpointConfig::load(&Figment::from(PlainKeys(Toml::string(toml))))
    }

    /// Loads `toml` with the process environment's `RESTATE_SZAMLAZZ_`
    /// overrides on top, as the binary does; call it inside a [`Jail`].
    fn load_with_env(toml: &str) -> Result<EndpointConfig> {
        EndpointConfig::load(
            &Figment::from(PlainKeys(Toml::string(toml))).merge(EnvOverrides::new()),
        )
    }

    /// Every documented example configuration — the fixtures, every TOML
    /// block of the endpoint README, the library README, the workspace README
    /// and the design document — loads and builds its accounts, so the
    /// documentation cannot drift from what the loader accepts (a key the
    /// loader does not know fails here).
    #[test]
    fn every_documented_example_loads() {
        let documents = [
            ("endpoint README", include_str!("../README.md")),
            (
                "library README",
                include_str!("../../restate-szamlazz/README.md"),
            ),
            ("workspace README", include_str!("../../../README.md")),
            (
                "design document",
                include_str!("../../../docs/design/restate-szamlazz.md"),
            ),
        ];
        let mut examples = vec![
            (
                "fixtures/single.toml",
                include_str!("../fixtures/single.toml"),
            ),
            (
                "fixtures/multi.toml",
                include_str!("../fixtures/multi.toml"),
            ),
        ];
        for (name, document) in documents {
            for block in document
                .split("```toml\n")
                .skip(1)
                .filter_map(|rest| rest.split_once("```").map(|(block, _)| block))
            {
                examples.push((name, block));
            }
        }
        assert!(examples.len() >= 6, "the documents carry TOML examples");

        for (name, toml) in examples {
            let config = load(toml).unwrap_or_else(|error| panic!("{name}: {error:#}\n{toml}"));
            StaticResolver::try_from(config.accounts)
                .unwrap_or_else(|error| panic!("{name}: {error}\n{toml}"));
        }
    }

    /// The fault table of the endpoint README and design §7 lists every
    /// `TerminalCode` with its status — a README row `` | `code` | status | ``,
    /// the design's `code (status)` — so the caller-facing table cannot drift
    /// from the codes the worker raises. (The library README's table is held
    /// to the same by the library's own test; this one reaches the documents
    /// outside that package.)
    #[test]
    fn every_terminal_code_is_in_every_fault_table() {
        use restate_szamlazz::contract::TerminalCode;

        let documents = [
            (
                "endpoint README",
                include_str!("../README.md"),
                "| `{code}` | {status} |",
            ),
            (
                "design document",
                include_str!("../../../docs/design/restate-szamlazz.md"),
                "{code} ({status})",
            ),
        ];
        for (name, document, shape) in documents {
            for code in TerminalCode::ALL {
                let entry = shape
                    .replace("{code}", code.as_str())
                    .replace("{status}", &code.status().to_string());
                assert!(
                    document.contains(&entry),
                    "{name} lists `{}` with status {}",
                    code.as_str(),
                    code.status()
                );
            }
        }
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
        assert_eq!(config.worker.read.max_attempts, 5);
        assert_eq!(config.worker.read.initial_delay, Duration::from_secs(5));
        assert_eq!(config.worker.read.factor.to_bits(), 2.0f32.to_bits());
        assert_eq!(config.worker.read.max_delay, Duration::from_secs(60));
        assert_eq!(config.worker.read.max_duration, Duration::from_secs(300));
        assert_eq!(config.worker.resolve.initial_delay, Duration::from_secs(1));
        assert_eq!(config.worker.resolve.max_delay, Duration::from_secs(10));
        assert_eq!(config.worker.resolve.max_duration, Duration::from_secs(60));

        let account = config
            .accounts
            .account
            .as_ref()
            .expect("the single account");
        assert!(config.accounts.accounts.is_empty());
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
        assert_eq!(config.worker.read, ReadConfig::default());
        assert_eq!(config.worker.resolve, ResolveConfig::default());
        let account = config
            .accounts
            .account
            .as_ref()
            .expect("the single account");
        assert_eq!(account.id.as_str(), "acme");
        assert_eq!(account.endpoint, None);
        assert_eq!(account.mode, AccountMode::Live);
        assert_eq!(account.supplier_id, None);
        assert_eq!(account.defaults.currency, "HUF");
        assert_eq!(account.seller.bank_account, None);
        assert!(config.identity_keys.is_empty());
    }

    /// Environment overrides address every level with `__`: the agent key
    /// and the mode under `[account]`, a document default under
    /// `[account.defaults]`, an issue-policy field, a read-policy field and
    /// the namespace itself. Every value is a string the field's type reads:
    /// `"3"` is `3` on a count, `"1.5"` on a factor, `"true"` on a flag,
    /// `"90"` seconds on a duration, `"972720"` on the supplier pin — and an
    /// all-digit agent key stays the string it was written as, leading zero
    /// included.
    #[test]
    fn environment_overrides_nest_with_double_underscores_and_are_read_as_strings() {
        Jail::expect_with(|jail| {
            jail.set_env("RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY", "0071234");
            jail.set_env("RESTATE_SZAMLAZZ_ACCOUNT__MODE", "test");
            jail.set_env("RESTATE_SZAMLAZZ_ACCOUNT__SUPPLIER_ID", "972721");
            jail.set_env("RESTATE_SZAMLAZZ_ACCOUNT__DEFAULTS__CURRENCY", "EUR");
            jail.set_env("RESTATE_SZAMLAZZ_ACCOUNT__DEFAULTS__E_INVOICE", "true");
            jail.set_env("RESTATE_SZAMLAZZ_ISSUE__MAX_ATTEMPTS", "3");
            jail.set_env("RESTATE_SZAMLAZZ_ISSUE__FACTOR", "1.5");
            jail.set_env("RESTATE_SZAMLAZZ_ISSUE__INITIAL_DELAY", "90");
            jail.set_env("RESTATE_SZAMLAZZ_READ__MAX_ATTEMPTS", "4");
            jail.set_env("RESTATE_SZAMLAZZ_NAMESPACE", "from-env");

            let config = load_with_env(SPEC_EXAMPLE).expect("configuration should load");

            let account = config
                .accounts
                .account
                .as_ref()
                .expect("the single account");
            assert_eq!(account.agent_key.expose(), "0071234", "byte-exact");
            assert_eq!(account.mode, AccountMode::Test);
            assert_eq!(account.supplier_id, Some(972_721));
            assert_eq!(account.defaults.currency, "EUR");
            assert!(account.defaults.e_invoice);
            assert_eq!(config.worker.issue.max_attempts, 3);
            assert_eq!(config.worker.issue.factor.to_bits(), 1.5f32.to_bits());
            assert_eq!(config.worker.issue.initial_delay, Duration::from_secs(90));
            assert_eq!(config.worker.read.max_attempts, 4);
            assert_eq!(config.worker.namespace.as_str(), "from-env");
            // Untouched values survive the merge.
            assert_eq!(account.id.as_str(), "acme");
            assert_eq!(account.defaults.language, "hu");
            assert_eq!(config.worker.issue.max_delay, Duration::from_secs(600));
            assert_eq!(config.worker.read.max_delay, Duration::from_secs(60));
            Ok(())
        });
    }

    /// A string a numeric field cannot read is a parse error naming the
    /// variable, not a silently defaulted setting.
    #[test]
    fn an_environment_value_the_field_cannot_read_is_refused() {
        Jail::expect_with(|jail| {
            jail.set_env("RESTATE_SZAMLAZZ_ISSUE__MAX_ATTEMPTS", "three");
            let error = load_with_env(minimal()).expect_err("`three` is not a count");
            let message = format!("{error:#}");
            assert!(
                message.contains("RESTATE_SZAMLAZZ_ISSUE__MAX_ATTEMPTS"),
                "{message}"
            );
            assert!(message.contains("\"three\""), "{message}");
            Ok(())
        });
    }

    /// The binary's sources: a file by extension — TOML, JSON or YAML — with
    /// the environment on top; a missing file and an unknown extension are
    /// refused before anything is read.
    #[test]
    fn sources_are_the_file_by_extension_and_the_environment() {
        Jail::expect_with(|jail| {
            jail.create_file("config.toml", minimal())?;
            jail.create_file(
                "config.json",
                r#"{"namespace": "acct", "account": {"id": "acme", "agent_key": "k"}}"#,
            )?;
            jail.create_file(
                "config.yaml",
                "namespace: acct\naccount:\n  id: acme\n  agent_key: k\n",
            )?;
            jail.set_env("RESTATE_SZAMLAZZ_ACCOUNT__MODE", "test");

            for file in ["config.toml", "config.json", "config.yaml"] {
                let figment = figment(Some(Path::new(file))).expect(file);
                let config = EndpointConfig::load(&figment).expect(file);
                assert_eq!(config.worker.namespace.as_str(), "acct", "{file}");
                let account = config.accounts.account.as_ref().expect(file);
                assert_eq!(account.id.as_str(), "acme", "{file}");
                assert_eq!(
                    account.mode,
                    AccountMode::Test,
                    "{file}: the environment is merged on top"
                );
            }

            let error = figment(Some(Path::new("missing.toml"))).expect_err("missing");
            assert!(
                error.to_string().contains("config file not found"),
                "{error}"
            );

            jail.create_file("config.ini", "namespace = acct")?;
            let error = figment(Some(Path::new("config.ini"))).expect_err("unsupported");
            assert!(
                error.to_string().contains("unsupported config file format"),
                "{error}"
            );

            // No file: the environment alone.
            jail.set_env("RESTATE_SZAMLAZZ_NAMESPACE", "env");
            jail.set_env("RESTATE_SZAMLAZZ_ACCOUNT__ID", "acme");
            jail.set_env("RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY", "k");
            let config =
                EndpointConfig::load(&figment(None).expect("no file")).expect("environment only");
            assert_eq!(config.worker.namespace.as_str(), "env");
            Ok(())
        });
    }

    /// A file's key paths render plainly in errors — `issue.factor`, not
    /// figment's `default.issue.factor` — beside the file they came from.
    #[test]
    fn file_key_paths_render_without_the_profile_prefix() {
        Jail::expect_with(|jail| {
            jail.create_file(
                "bad.toml",
                &format!("{}\n[issue]\nfactor = \"x\"", minimal()),
            )?;
            let error = EndpointConfig::load(&figment(Some(Path::new("bad.toml"))).expect("file"))
                .expect_err("`x` is not a factor");
            let message = format!("{error:#}");
            assert!(message.contains("for key \"issue.factor\""), "{message}");
            assert!(message.contains("bad.toml TOML file"), "{message}");
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

            let config = load_with_env(minimal()).expect("configuration should load");

            assert_eq!(config.identity_keys, ["publickeyv1_old", "publickeyv1_new"]);
            Ok(())
        });
    }

    #[test]
    fn missing_namespace_fails_to_parse_and_no_account_is_refused_by_the_resolver() {
        let error = load("[account]\nid = \"acme\"\nagent_key = \"k\"").expect_err("no namespace");
        assert!(
            format!("{error:#}").contains("namespace"),
            "the error names the missing key: {error:#}"
        );

        // Neither shape parses (`account` is optional, `accounts` defaults to
        // empty); the static resolver refuses it when built.
        let config = load("namespace = \"acct\"").expect("parses without accounts");
        assert!(config.accounts.account.is_none());
        assert!(config.accounts.accounts.is_empty());
        let error = StaticResolver::try_from(config.accounts).expect_err("no account");
        assert!(
            error.to_string().contains("no account is configured"),
            "{error}"
        );
    }

    /// The multi-account shape: `[accounts.<scope>]` tables parse into the
    /// static resolver's map, and a per-scope environment override addresses
    /// one account's key (`…ACCOUNTS__<SCOPE>__AGENT_KEY`) and nothing else.
    #[test]
    fn multi_account_shape_parses_and_per_scope_environment_overrides_apply() {
        const MULTI: &str = r#"
            namespace = "acct"

            [accounts.acme]
            id = "acme"
            agent_key = "key-acme-file"
            endpoint = "http://127.0.0.1:1/"
            mode = "test"
            supplier_id = 972720

            [accounts.acme.seller]
            bank_account = "11111111-22222222"

            [accounts.beta_events]
            id = "beta"
            agent_key = "key-beta-file"
            endpoint = "http://127.0.0.1:1/"
            mode = "test"
            supplier_id = 972721
        "#;

        let config = load(MULTI).expect("configuration should load");
        assert!(config.accounts.account.is_none());
        assert_eq!(
            config.accounts.accounts.keys().collect::<Vec<_>>(),
            ["acme", "beta_events"]
        );
        assert_eq!(
            config.accounts.accounts["acme"].agent_key.expose(),
            "key-acme-file"
        );

        Jail::expect_with(|jail| {
            jail.set_env("RESTATE_SZAMLAZZ_ACCOUNTS__ACME__AGENT_KEY", "key-acme-env");
            jail.set_env("RESTATE_SZAMLAZZ_ACCOUNTS__BETA_EVENTS__MODE", "live");

            let config = load_with_env(MULTI).expect("configuration should load");

            let acme = &config.accounts.accounts["acme"];
            let beta = &config.accounts.accounts["beta_events"];
            assert_eq!(acme.agent_key.expose(), "key-acme-env");
            assert_eq!(acme.mode, AccountMode::Test);
            assert_eq!(
                acme.seller.bank_account.as_deref(),
                Some("11111111-22222222"),
                "the rest of the table survives"
            );
            assert_eq!(
                beta.agent_key.expose(),
                "key-beta-file",
                "the other account's key is untouched"
            );
            assert_eq!(beta.mode, AccountMode::Live);
            assert_eq!(beta.id.as_str(), "beta");

            // What the binary then builds: each account under its scope.
            let resolver = StaticResolver::try_from(config.accounts).expect("resolver");
            assert!(resolver.is_scoped());
            assert_eq!(resolver.accounts().count(), 2);
            Ok(())
        });
    }

    /// Both shapes in one configuration are refused at load, naming both
    /// tables and where each came from — in particular a multi-account file
    /// plus a stray `RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY` left over from the
    /// flag day, which materialises a partial `[account]`: the error names
    /// `account` and the rule, not `missing field id`.
    #[test]
    fn both_shapes_are_refused_at_load_naming_both_sources() {
        let error = load(&format!(
            "{}\n[accounts.beta]\nid = \"beta\"\nagent_key = \"k\"\nsupplier_id = 1",
            minimal()
        ))
        .expect_err("both shapes in one file");
        let message = format!("{error:#}");
        assert!(message.contains("`account`"), "{message}");
        assert!(message.contains("`accounts`"), "{message}");
        assert!(message.contains("mutually exclusive"), "{message}");

        Jail::expect_with(|jail| {
            jail.set_env("RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY", "stray");
            let error = load_with_env(
                r#"
                    namespace = "acct"

                    [accounts.acme]
                    id = "acme"
                    agent_key = "k"
                    supplier_id = 1
                    "#,
            )
            .expect_err("a stray single-shape override on a multi-account file");
            let message = format!("{error:#}");
            assert!(message.contains("`account`"), "{message}");
            assert!(message.contains("environment variable"), "{message}");
            assert!(message.contains("`accounts`"), "{message}");
            assert!(message.contains("TOML source string"), "{message}");
            assert!(
                !message.contains("missing field"),
                "the both-shapes rule, not the partial account's parse error: {message}"
            );
            assert!(
                !message.contains("stray"),
                "the key is not echoed: {message}"
            );

            // And the other way round: a stray scoped override on a
            // single-account file.
            jail.clear_env();
            jail.set_env("RESTATE_SZAMLAZZ_ACCOUNTS__ACME__AGENT_KEY", "stray");
            let error = load_with_env(minimal())
                .expect_err("a stray multi-shape override on a single-account file");
            let message = format!("{error:#}");
            assert!(message.contains("mutually exclusive"), "{message}");
            assert!(
                message.contains("`accounts` from environment variables"),
                "{message}"
            );
            assert!(
                !message.contains("stray"),
                "the key is not echoed: {message}"
            );
            Ok(())
        });
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

    /// An unknown key is refused at every level — the top level, a policy,
    /// an account table and its `defaults` / `seller` / `seller.email`
    /// sub-tables, in either shape — with an error naming the key, its path
    /// and where it came from, instead of being ignored and leaving the
    /// setting at its default.
    #[test]
    fn unknown_keys_are_refused_with_their_path_and_source() {
        const MULTI: &str = r#"
            namespace = "acct"

            [accounts.acme]
            id = "acme"
            agent_key = "k"
            supplier_id = 1
        "#;
        let cases = [
            // A misspelt policy table leaves the issue policy at its default.
            (
                format!("{}\n[isue]\nmax_attempts = 1", minimal()),
                "isue",
                "issue",
            ),
            // A misspelt `mode` runs a test account as live.
            (
                format!("{}\nmod = \"test\"", minimal()),
                "account.mod",
                "mode",
            ),
            // A misspelt `supplier_id` drops the supplier pin.
            (
                format!("{}\nsupplyer_id = 1", minimal()),
                "account.supplyer_id",
                "supplier_id",
            ),
            (
                format!("{}\n[account.defaults]\ncurency = \"EUR\"", minimal()),
                "account.defaults.curency",
                "currency",
            ),
            (
                format!("{MULTI}\n[accounts.acme.seller]\nbnk = \"B\""),
                "accounts.acme.seller.bnk",
                "bank",
            ),
            (
                format!("{MULTI}\n[accounts.acme.seller.email]\nsubjet = \"S\""),
                "accounts.acme.seller.email.subjet",
                "subject",
            ),
            (
                format!("{}\n[read]\nmax_atempts = 1", minimal()),
                "read.max_atempts",
                "max_attempts",
            ),
            (
                format!("{}\n[resolve]\nmax_attempts = 1", minimal()),
                "resolve.max_attempts",
                "max_delay",
            ),
        ];
        for (toml, path, expected) in cases {
            let error = load(&toml)
                .err()
                .unwrap_or_else(|| panic!("`{path}` must not load"));
            let message = format!("{error:#}");
            assert!(
                message.contains(&format!("unknown key `{path}`")),
                "the error names the key and its path: {message}"
            );
            assert!(
                message.contains("TOML source string"),
                "the error names the source: {message}"
            );
            assert!(
                message.contains(&format!("`{expected}`")),
                "the error lists what is expected there: {message}"
            );
        }

        // Every unknown key is reported, not just the first.
        let error = load(&format!("{}\nmod = \"test\"\n[isue]\nx = 1", minimal()))
            .expect_err("two unknown keys");
        let message = format!("{error:#}");
        assert!(message.contains("unknown key `isue`"), "{message}");
        assert!(message.contains("unknown key `account.mod`"), "{message}");

        // The value under an unknown key is never echoed: a misspelt
        // `agent_key` holds the secret.
        let error = load(&format!(
            "{}\nagent_kye = \"sentinel-secret-9f1c\"",
            minimal()
        ))
        .expect_err("a misspelt agent_key");
        let message = format!("{error:#}");
        assert!(
            message.contains("unknown key `account.agent_kye`"),
            "{message}"
        );
        assert!(!message.contains("sentinel-secret-9f1c"), "{message}");

        // The environment is a source like any other.
        Jail::expect_with(|jail| {
            jail.set_env("RESTATE_SZAMLAZZ_ACOUNT__MODE", "test");
            let error = load_with_env(minimal())
                .expect_err("a misspelt environment override must not load");
            let message = format!("{error:#}");
            assert!(
                message.contains("unknown key `acount` (RESTATE_SZAMLAZZ_ACOUNT__MODE)"),
                "the error names the key and the variable that set it: {message}"
            );
            assert!(message.contains("in environment variables"), "{message}");
            Ok(())
        });
    }

    /// A parse error names the key it happened at and where that key came
    /// from — a wrong type under `[issue]` in a file, a wrong type under
    /// `[account.defaults]` from the environment — not just serde's message.
    #[test]
    fn parse_errors_name_the_key_path_and_the_source() {
        let error = load(&format!("{}\n[issue]\ninitial_delay = true", minimal()))
            .expect_err("a boolean is not a duration");
        let message = format!("{error:#}");
        assert!(
            message.contains("issue.initial_delay"),
            "the error names the key path: {message}"
        );
        assert!(
            message.contains("TOML source string"),
            "the error names the source: {message}"
        );

        Jail::expect_with(|jail| {
            jail.set_env("RESTATE_SZAMLAZZ_ACCOUNT__DEFAULTS__E_INVOICE", "sometimes");
            let error = load_with_env(minimal()).expect_err("`sometimes` is not a boolean");
            let message = format!("{error:#}");
            assert!(
                message.contains("RESTATE_SZAMLAZZ_ACCOUNT__DEFAULTS__E_INVOICE"),
                "the error names the variable: {message}"
            );
            assert!(
                message.contains("in environment variables"),
                "the error names the source: {message}"
            );
            Ok(())
        });
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

        let error = load(&format!("{}\n[read]\nmax_attempts = 0", minimal()))
            .expect_err("a read policy without an execution must not load");
        assert!(
            format!("{error:#}").contains("read.max_attempts must be at least 1"),
            "{error:#}"
        );

        // An issue delay under the floor (the client timeout plus a margin)
        // would re-execute the create or storno step while its send may still
        // be in flight: the endpoint does not start. The full wording is
        // pinned where the error is defined; here, the table and the floor.
        let error = load(&format!("{}\n[issue]\ninitial_delay = \"5s\"", minimal()))
            .expect_err("an issue delay below the floor must not load");
        let message = format!("{error:#}");
        assert!(message.contains("invalid configuration"), "{message}");
        assert!(
            message.contains("issue.initial_delay (5s) must be at least 90s"),
            "{message}"
        );
    }
}
