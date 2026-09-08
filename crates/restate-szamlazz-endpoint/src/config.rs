//! Endpoint configuration: the deployment-level
//! [`WorkerConfig`](restate_szamlazz::WorkerConfig) (`namespace`, `[issue]`,
//! `[read]`, `[resolve]`), the static resolver's accounts
//! ([`StaticConfig`](restate_szamlazz::account::StaticConfig): `[account]` or
//! `[accounts.<scope>]`) and what only the hosting process cares about
//! (`identity_keys`, read as the [`RequestIdentity`] decision). Strict: the
//! layout and every library type it is made of are closed
//! (`#[serde(deny_unknown_fields)]`), so a key the configuration does not
//! know, at any level, is a parse error naming the key, its path and its
//! source; the one shape rule serde cannot express is [`shape`]'s.

mod shape;
mod sources;

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context as _, Result, bail};
use figment::Figment;
use figment::providers::{Format, Json, Toml, Yaml};
use restate_szamlazz::account::{StaticAccount, StaticConfig};
use restate_szamlazz::config::{
    IssueConfig, ReadConfig, ResolveConfig, ValidatedWorkerConfig, WorkerConfig,
};
use restate_szamlazz::identity::Namespace;
use serde::Deserialize;

pub use sources::{EnvOverrides, PlainKeys};

/// The environment prefix of configuration overrides; `__` nests
/// (`RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY` → `account.agent_key`).
pub const ENV_PREFIX: &str = "RESTATE_SZAMLAZZ_";

/// The configuration sources: the file at `path`, if any (TOML, JSON or
/// YAML by extension) with the `RESTATE_SZAMLAZZ_` environment overrides
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
/// keys and both shapes at once and validates the deployment-level
/// invariants; the accounts' own rules (a non-blank id and key, the
/// multi-account uniqueness rules) are checked when the static resolver is
/// built from `accounts`.
#[derive(Debug, Clone)]
pub struct EndpointConfig {
    /// The deployment-level settings of the services, validated.
    pub worker: ValidatedWorkerConfig,
    /// The accounts of the static resolver.
    pub accounts: StaticConfig,
    /// What the endpoint does with a request's signature, as `identity_keys`
    /// decides it.
    pub request_identity: RequestIdentity,
}

/// What the endpoint does with the signature Restate puts on every request
/// when the runtime holds a request identity key, as the top-level
/// `identity_keys` decides it.
///
/// Identity keys are what enforce the assumption the scope model rests on:
/// that only the Restate runtime speaks to the endpoint. The
/// scope travels as protocol data inside the request, so an endpoint that
/// accepts unsigned requests lets any client reaching its port invoke either
/// service under any scope. Hence the distinction between the two unsigned
/// cases: an operator who wrote `identity_keys = []` chose this (a laptop);
/// one whose configuration does not mention `identity_keys` may not have;
/// the start-up log warns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestIdentity {
    /// `identity_keys` lists at least one `publickeyv1_...` public key: a
    /// request not signed with one of them is refused. Listing the old and
    /// the new key keeps both valid during rotation.
    Verified(Vec<String>),
    /// No key is configured: every request is accepted, signed or not.
    /// `deliberate` when the configuration writes the empty list out
    /// (`identity_keys = []`, in the file: the local-development opt-out),
    /// not when it omits `identity_keys` or sets it to a delimited string
    /// that yields no key (an empty `RESTATE_SZAMLAZZ_IDENTITY_KEYS`, which
    /// is what a deployment template renders when the secret is missing).
    Unsigned {
        /// Whether the operator wrote the empty list out.
        deliberate: bool,
    },
}

impl Default for RequestIdentity {
    /// What `identity_keys` unmentioned means: unsigned, and not by choice.
    fn default() -> Self {
        Self::Unsigned { deliberate: false }
    }
}

/// The file layout, one explicit field per top-level key, closed. The
/// library's [`WorkerConfig`] and [`StaticConfig`] are assembled from it
/// rather than flattened into it, so a parse error keeps the key path and the
/// source figment attaches; `#[serde(flatten)]` deserializes through a buffer
/// that drops both (and admits no `deny_unknown_fields`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// The `identity_keys` key, read straight into the decision it makes;
    /// the default is the key unmentioned.
    #[serde(default, deserialize_with = "identity_keys")]
    identity_keys: RequestIdentity,
}

impl EndpointConfig {
    /// Extracts the configuration from `figment` and validates the
    /// deployment-level invariants.
    ///
    /// # Errors
    ///
    /// Returns an error when the figment holds both account shapes at once
    /// (each named with its source); when it does not parse, a key the
    /// configuration does not know at any level included (named with its
    /// path and its source); or when [`WorkerConfig::validate`] fails. The
    /// accounts themselves are validated when the static resolver is built.
    pub fn load(figment: &Figment) -> Result<Self> {
        shape::check(figment).context("invalid configuration")?;
        // Lossy: an environment value is a string, and the field's type
        // decides how it is read (`"3"` → `3` where a number is expected).
        let Layout {
            namespace,
            issue,
            read,
            resolve,
            account,
            accounts,
            identity_keys,
        } = figment
            .extract_lossy()
            .context("failed to parse configuration")?;
        let worker = WorkerConfig {
            namespace,
            issue,
            read,
            resolve,
        }
        .validate()
        .context("invalid configuration")?;
        Ok(Self {
            worker,
            accounts: StaticConfig { account, accounts },
            request_identity: identity_keys,
        })
    }
}

/// Reads `identity_keys` (a list, or a comma/whitespace-delimited string,
/// the shape the `RESTATE_SZAMLAZZ_IDENTITY_KEYS` environment override has
/// since every environment value is a string) as the [`RequestIdentity`] it
/// decides. Any key makes it `Verified`. The empty list literal is the
/// deliberate opt-out; a delimited string that yields no key is not: it
/// reads as the key unmentioned, because an empty environment variable is
/// what a deployment template renders when the secret is missing.
fn identity_keys<'de, D>(deserializer: D) -> Result<RequestIdentity, D::Error>
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
        IdentityKeys::List(keys) if keys.is_empty() => {
            RequestIdentity::Unsigned { deliberate: true }
        }
        IdentityKeys::List(keys) => RequestIdentity::Verified(keys),
        IdentityKeys::Delimited(keys) => {
            let keys: Vec<String> = keys
                .split([',', ' ', '\t', '\n'])
                .filter(|key| !key.is_empty())
                .map(str::to_owned)
                .collect();
            if keys.is_empty() {
                RequestIdentity::default()
            } else {
                RequestIdentity::Verified(keys)
            }
        }
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
    use restate_szamlazz::config::{IssueConfig, ReadConfig, ResolveConfig};

    use super::*;

    /// A full configuration example, with `identity_keys = []` written out.
    const FULL_EXAMPLE: &str = r#"
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

    /// Every documented example configuration (the fixtures and every TOML
    /// block of the endpoint README, the library README and the workspace
    /// README) loads and builds its accounts, so the documentation cannot
    /// drift from what the loader accepts (a key the loader does not know
    /// fails here).
    #[test]
    fn every_documented_example_loads() {
        let documents = [
            ("endpoint README", include_str!("../README.md")),
            (
                "library README",
                include_str!("../../restate-szamlazz/README.md"),
            ),
            ("workspace README", include_str!("../../../README.md")),
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
        // The two fixtures plus the endpoint README's blocks (the quick start's
        // minimal configuration, the full single-account one, the
        // multi-account one).
        assert!(examples.len() >= 5, "the documents carry TOML examples");

        for (name, toml) in examples {
            let config = load(toml).unwrap_or_else(|error| panic!("{name}: {error:#}\n{toml}"));
            StaticResolver::try_from(config.accounts)
                .unwrap_or_else(|error| panic!("{name}: {error}\n{toml}"));
        }
    }

    /// The fault table of the endpoint README lists every `TerminalCode` with
    /// its status, as a row `` | `code` | status | ``, so the caller-facing
    /// table cannot drift from the codes the worker raises. (The library
    /// README's table is held to the same by the library's own test; this one
    /// reaches the document outside that package.)
    #[test]
    fn every_terminal_code_is_in_every_fault_table() {
        use restate_szamlazz::contract::TerminalCode;

        let documents = [(
            "endpoint README",
            include_str!("../README.md"),
            "| `{code}` | {status} |",
        )];
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
    fn parses_the_full_example() {
        let config = load(FULL_EXAMPLE).expect("configuration should load");

        assert_eq!(config.worker.namespace.as_str(), "acct");
        assert_eq!(config.worker.issue.max_attempts, Some(5));
        assert_eq!(config.worker.issue.initial_delay, Duration::from_secs(120));
        assert_eq!(config.worker.issue.factor.to_bits(), 2.0f32.to_bits());
        assert_eq!(config.worker.issue.max_delay, Duration::from_secs(600));
        assert_eq!(config.worker.issue.max_duration, Duration::from_secs(3600));
        assert_eq!(config.worker.read.max_attempts, Some(5));
        assert_eq!(config.worker.read.initial_delay, Duration::from_secs(5));
        assert_eq!(config.worker.read.factor.to_bits(), 2.0f32.to_bits());
        assert_eq!(config.worker.read.max_delay, Duration::from_secs(60));
        assert_eq!(config.worker.read.max_duration, Duration::from_secs(300));
        assert_eq!(config.worker.resolve.max_attempts, None);
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
        assert_eq!(
            config.request_identity,
            RequestIdentity::Unsigned { deliberate: true },
            "`identity_keys = []` is written out"
        );
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
        assert_eq!(account.defaults.currency, "HUF");
        assert_eq!(account.seller.bank_account, None);
        assert_eq!(
            config.request_identity,
            RequestIdentity::Unsigned { deliberate: false },
            "`identity_keys` is not mentioned"
        );
    }

    /// Environment overrides address every level with `__`: the agent key
    /// and the mode under `[account]`, a document default under
    /// `[account.defaults]`, an issue-policy field, a read-policy field and
    /// the namespace itself. Every value is a string the field's type reads:
    /// `"3"` is `3` on a count, `"1.5"` on a factor, `"true"` on a flag,
    /// `"90"` seconds on a duration, and an all-digit agent key stays the
    /// string it was written as, leading zero included.
    #[test]
    fn environment_overrides_nest_with_double_underscores_and_are_read_as_strings() {
        Jail::expect_with(|jail| {
            jail.set_env("RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY", "0071234");
            jail.set_env("RESTATE_SZAMLAZZ_ACCOUNT__DEFAULTS__CURRENCY", "EUR");
            jail.set_env("RESTATE_SZAMLAZZ_ACCOUNT__DEFAULTS__E_INVOICE", "true");
            jail.set_env("RESTATE_SZAMLAZZ_ISSUE__MAX_ATTEMPTS", "3");
            jail.set_env("RESTATE_SZAMLAZZ_ISSUE__FACTOR", "1.5");
            jail.set_env("RESTATE_SZAMLAZZ_ISSUE__INITIAL_DELAY", "90");
            jail.set_env("RESTATE_SZAMLAZZ_READ__MAX_ATTEMPTS", "4");
            jail.set_env("RESTATE_SZAMLAZZ_NAMESPACE", "from-env");

            let config = load_with_env(FULL_EXAMPLE).expect("configuration should load");

            let account = config
                .accounts
                .account
                .as_ref()
                .expect("the single account");
            assert_eq!(account.agent_key.expose(), "0071234", "byte-exact");
            assert_eq!(account.defaults.currency, "EUR");
            assert!(account.defaults.e_invoice);
            assert_eq!(config.worker.issue.max_attempts, Some(3));
            assert_eq!(config.worker.issue.factor.to_bits(), 1.5f32.to_bits());
            assert_eq!(config.worker.issue.initial_delay, Duration::from_secs(90));
            assert_eq!(config.worker.read.max_attempts, Some(4));
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

    /// The binary's sources: a file by extension (TOML, JSON or YAML) with
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
            jail.set_env("RESTATE_SZAMLAZZ_ACCOUNT__DEFAULTS__CURRENCY", "EUR");

            for file in ["config.toml", "config.json", "config.yaml"] {
                let figment = figment(Some(Path::new(file))).expect(file);
                let config = EndpointConfig::load(&figment).expect(file);
                assert_eq!(config.worker.namespace.as_str(), "acct", "{file}");
                let account = config.accounts.account.as_ref().expect(file);
                assert_eq!(account.id.as_str(), "acme", "{file}");
                assert_eq!(
                    account.defaults.currency, "EUR",
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

    /// A file's key paths render plainly in errors (`issue.factor`, not
    /// figment's `default.issue.factor`) beside the file they came from.
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

        assert_eq!(
            list.request_identity,
            RequestIdentity::Verified(vec![
                "publickeyv1_old".to_owned(),
                "publickeyv1_new".to_owned()
            ])
        );
        assert_eq!(delimited.request_identity, list.request_identity);
    }

    #[test]
    fn identity_keys_from_environment_are_a_delimited_string() {
        Jail::expect_with(|jail| {
            jail.set_env(
                "RESTATE_SZAMLAZZ_IDENTITY_KEYS",
                "publickeyv1_old,publickeyv1_new",
            );

            let config = load_with_env(minimal()).expect("configuration should load");

            assert_eq!(
                config.request_identity,
                RequestIdentity::Verified(vec![
                    "publickeyv1_old".to_owned(),
                    "publickeyv1_new".to_owned()
                ])
            );
            Ok(())
        });
    }

    /// Without a key the endpoint accepts unsigned requests either way; what
    /// differs is whether the operator said so. `identity_keys = []` written
    /// out is the deliberate opt-out (local development; an `info` at
    /// start-up); a configuration that does not mention `identity_keys` is
    /// the omission the start-up `warn` names: in the file, and when a
    /// JSON or YAML file writes the empty list.
    #[test]
    fn omitting_identity_keys_is_unsigned_by_omission_and_an_explicit_empty_list_is_deliberate() {
        let omitted = load(minimal()).expect("configuration should load");
        assert_eq!(
            omitted.request_identity,
            RequestIdentity::Unsigned { deliberate: false }
        );

        let written_out =
            load(&format!("identity_keys = []\n{}", minimal())).expect("configuration should load");
        assert_eq!(
            written_out.request_identity,
            RequestIdentity::Unsigned { deliberate: true }
        );

        Jail::expect_with(|jail| {
            jail.create_file(
                "config.json",
                r#"{"namespace": "acct", "identity_keys": [], "account": {"id": "acme", "agent_key": "k"}}"#,
            )?;
            jail.create_file(
                "config.yaml",
                "namespace: acct\nidentity_keys: []\naccount:\n  id: acme\n  agent_key: k\n",
            )?;
            for file in ["config.json", "config.yaml"] {
                let config =
                    EndpointConfig::load(&figment(Some(Path::new(file))).expect(file)).expect(file);
                assert_eq!(
                    config.request_identity,
                    RequestIdentity::Unsigned { deliberate: true },
                    "{file}: the empty list is written out"
                );
            }
            Ok(())
        });
    }

    /// A delimited string that yields no key is not the opt-out: an empty
    /// `RESTATE_SZAMLAZZ_IDENTITY_KEYS` is what a deployment template renders
    /// when the secret it should carry is missing (the very case the
    /// start-up warning exists for), so it warns like an omission, and it
    /// overrides a file's keys or its written-out `[]` the same way. Only the
    /// list literal `[]` is deliberate.
    #[test]
    fn a_delimited_string_without_keys_is_not_the_opt_out() {
        let blank = load(&format!("identity_keys = \"\"\n{}", minimal()))
            .expect("configuration should load");
        assert_eq!(
            blank.request_identity,
            RequestIdentity::Unsigned { deliberate: false }
        );
        let separators = load(&format!("identity_keys = \" , \"\n{}", minimal()))
            .expect("configuration should load");
        assert_eq!(
            separators.request_identity,
            RequestIdentity::Unsigned { deliberate: false }
        );

        Jail::expect_with(|jail| {
            jail.set_env("RESTATE_SZAMLAZZ_IDENTITY_KEYS", "");

            let over_keys = load_with_env(&format!(
                "identity_keys = [\"publickeyv1_old\"]\n{}",
                minimal()
            ))
            .expect("configuration should load");
            assert_eq!(
                over_keys.request_identity,
                RequestIdentity::Unsigned { deliberate: false },
                "the blank override replaces the file's keys and warns"
            );

            let over_opt_out = load_with_env(&format!("identity_keys = []\n{}", minimal()))
                .expect("configuration should load");
            assert_eq!(
                over_opt_out.request_identity,
                RequestIdentity::Unsigned { deliberate: false },
                "the blank override is not the file's deliberate `[]`"
            );
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

            [accounts.acme.seller]
            bank_account = "11111111-22222222"

            [accounts.beta_events]
            id = "beta"
            agent_key = "key-beta-file"
            endpoint = "http://127.0.0.1:1/"
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
            jail.set_env(
                "RESTATE_SZAMLAZZ_ACCOUNTS__BETA_EVENTS__DEFAULTS__LANGUAGE",
                "en",
            );

            let config = load_with_env(MULTI).expect("configuration should load");

            let acme = &config.accounts.accounts["acme"];
            let beta = &config.accounts.accounts["beta_events"];
            assert_eq!(acme.agent_key.expose(), "key-acme-env");
            assert_eq!(acme.defaults.language, "hu", "untouched");
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
            assert_eq!(beta.defaults.language, "en");
            assert_eq!(beta.id.as_str(), "beta");

            // What the binary then builds: each account under its scope.
            let resolver = StaticResolver::try_from(config.accounts).expect("resolver");
            assert!(resolver.is_scoped());
            assert_eq!(resolver.accounts().count(), 2);
            Ok(())
        });
    }

    /// Both shapes in one configuration are refused at load, naming both
    /// tables and where each came from, in particular a multi-account file
    /// plus a stray `RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY` left over from the
    /// flag day, which materialises a partial `[account]`: the error names
    /// `account` and the rule, not `missing field id`.
    #[test]
    fn both_shapes_are_refused_at_load_naming_both_sources() {
        let error = load(&format!(
            "{}\n[accounts.beta]\nid = \"beta\"\nagent_key = \"k\"",
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

    /// An unknown key is refused at every level (the top level, a policy,
    /// an account table and its `defaults` / `seller` / `seller.email`
    /// sub-tables, in either shape) with an error naming the key, its path,
    /// where it came from and the keys accepted there, instead of being
    /// ignored and leaving the setting at its default. The rule is serde's
    /// (every type of the layout is closed), so the first unknown key is
    /// reported; the path and the source are figment's.
    #[test]
    fn unknown_keys_are_refused_with_their_path_and_source() {
        const MULTI: &str = r#"
            namespace = "acct"

            [accounts.acme]
            id = "acme"
            agent_key = "k"
        "#;
        let cases = [
            // A misspelt policy table leaves the issue policy at its default.
            (
                format!("{}\n[isue]\nmax_attempts = 1", minimal()),
                "isue",
                "isue",
                "issue",
            ),
            // A misspelt `endpoint` posts to production.
            (
                format!("{}\nendpont = \"http://127.0.0.1:1/\"", minimal()),
                "endpont",
                "account.endpont",
                "endpoint",
            ),
            (
                format!("{}\n[account.defaults]\ncurency = \"EUR\"", minimal()),
                "curency",
                "account.defaults.curency",
                "currency",
            ),
            (
                format!("{MULTI}\n[accounts.acme.seller]\nbnk = \"B\""),
                "bnk",
                "accounts.acme.seller.bnk",
                "bank",
            ),
            (
                format!("{MULTI}\n[accounts.acme.seller.email]\nsubjet = \"S\""),
                "subjet",
                "accounts.acme.seller.email.subjet",
                "subject",
            ),
            (
                format!("{}\n[read]\nmax_atempts = 1", minimal()),
                "max_atempts",
                "read.max_atempts",
                "max_attempts",
            ),
            (
                format!("{}\n[resolve]\nattempts = 1", minimal()),
                "attempts",
                "resolve.attempts",
                "max_attempts",
            ),
        ];
        for (toml, key, path, expected) in cases {
            let error = load(&toml)
                .err()
                .unwrap_or_else(|| panic!("`{path}` must not load"));
            let message = format!("{error:#}");
            assert!(
                message.contains(&format!("unknown field: found `{key}`")),
                "the error names the key: {message}"
            );
            assert!(
                message.contains(&format!("for key \"{path}\"")),
                "the error names the path: {message}"
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

        // The value under an unknown key is never echoed: a misspelt
        // `agent_key` holds the secret.
        let error = load(&format!(
            "{}\nagent_kye = \"sentinel-secret-9f1c\"",
            minimal()
        ))
        .expect_err("a misspelt agent_key");
        let message = format!("{error:#}");
        assert!(
            message.contains("unknown field: found `agent_kye`"),
            "{message}"
        );
        assert!(!message.contains("sentinel-secret-9f1c"), "{message}");

        // The environment is a source like any other: the key renders as the
        // variable that set it.
        Jail::expect_with(|jail| {
            jail.set_env("RESTATE_SZAMLAZZ_ACOUNT__ID", "acme");
            let error = load_with_env(minimal())
                .expect_err("a misspelt environment override must not load");
            let message = format!("{error:#}");
            assert!(
                message.contains("unknown field: found `acount`"),
                "the error names the key: {message}"
            );
            assert!(
                message.contains("for key \"RESTATE_SZAMLAZZ_ACOUNT\""),
                "the error names the variable that set it: {message}"
            );
            assert!(message.contains("in environment variables"), "{message}");
            Ok(())
        });
    }

    /// A parse error names the key it happened at and where that key came
    /// from (a wrong type under `[issue]` in a file, a wrong type under
    /// `[account.defaults]` from the environment), not just serde's message.
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
