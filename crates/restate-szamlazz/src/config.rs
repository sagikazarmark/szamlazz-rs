//! Deployment configuration: everything that is constant for a deployment and
//! therefore never travels in a request payload.
//!
//! ```toml
//! [account]
//! slug = "acct"
//! agent_key = "..."
//! mode = "live"
//! supplier_id = 972720
//!
//! [defaults]
//! language = "hu"
//! currency = "HUF"
//!
//! [issue]
//! max_attempts = 5
//! initial_delay = "2m"
//! factor = 2.0
//! max_delay = "10m"
//! max_duration = "1h"
//! ```
//!
//! The types only implement `Deserialize`; the endpoint binary chooses the
//! file format and environment merging.
//!
//! A parsed [`Config`] is split at construction: the gateway takes the account
//! (credentials, mode, supplier pin, defaults, seller) and the services keep a
//! [`WorkerConfig`] (namespace, issue policy). `account.slug` is the
//! [`Namespace`]; the key keeps its old name for now.
//!
//! This is the legacy path. The account model the services will resolve per
//! invocation lives in [`crate::account`]: an [`Account`](crate::account::Account)
//! is built from a `Config` with `TryFrom`, and the static resolver's own
//! configuration ([`StaticConfig`](crate::account::StaticConfig)) carries the
//! account fields without the namespace.

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use restate_sdk::context::RunRetryPolicy;
use serde::{Deserialize, Serialize};
use szamlazz_agent::ops::invoice::{Seller, SellerEmail};

/// The complete deployment configuration.
///
/// The Restate services do not hold it: the account-shaped part is read
/// through [`Gateway::account`](crate::gateway::Gateway::account) and the rest
/// is a [`WorkerConfig`].
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// The single szamlazz.hu account this deployment issues for.
    pub account: AccountConfig,
    /// Document defaults that per-call overrides may change.
    #[serde(default)]
    pub defaults: Defaults,
    /// The seller block; account data is used where absent.
    #[serde(default)]
    pub seller: SellerConfig,
    /// The issue policy: the create step's run retry policy.
    #[serde(default)]
    pub issue: IssueConfig,
}

impl Config {
    /// Checks the cross-field invariants that `Deserialize` cannot express.
    ///
    /// The namespace is validated when parsed and needs no further check.
    ///
    /// # Errors
    ///
    /// Returns the first violated invariant: an empty agent key,
    /// `issue.max_attempts == 0`, `issue.initial_delay` greater than
    /// `issue.max_delay`, or `issue.factor` below 1.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.account.agent_key.expose().trim().is_empty() {
            return Err(ConfigError::EmptyAgentKey);
        }
        if self.issue.max_attempts == 0 {
            return Err(ConfigError::ZeroMaxAttempts);
        }
        if self.issue.initial_delay > self.issue.max_delay {
            return Err(ConfigError::DelayOrder {
                initial: self.issue.initial_delay,
                max: self.issue.max_delay,
            });
        }
        if self.issue.factor.is_nan() || self.issue.factor < 1.0 {
            return Err(ConfigError::InvalidFactor(self.issue.factor));
        }
        Ok(())
    }
}

/// The deployment-level settings the Restate services hold: what is not
/// account-shaped and therefore does not route through the gateway.
///
/// The namespace prefixes every external id the deployment issues; the issue
/// policy is the create step's run retry policy. Built from a [`Config`].
#[derive(Debug, Clone, PartialEq)]
pub struct WorkerConfig {
    /// The external-id prefix of this deployment (`{namespace}:{order}:{kind}`).
    pub namespace: Namespace,
    /// The issue policy: the create step's run retry policy.
    pub issue: IssueConfig,
}

impl From<&Config> for WorkerConfig {
    fn from(config: &Config) -> Self {
        Self {
            namespace: config.account.slug.clone(),
            issue: config.issue.clone(),
        }
    }
}

/// A [`Config`] that parsed but violates an invariant.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// `account.agent_key` is empty or blank.
    #[error("account.agent_key must not be empty")]
    EmptyAgentKey,
    /// `issue.max_attempts` is zero.
    #[error("issue.max_attempts must be at least 1")]
    ZeroMaxAttempts,
    /// `issue.initial_delay` exceeds `issue.max_delay`.
    #[error("issue.initial_delay ({initial:?}) must not exceed issue.max_delay ({max:?})")]
    DelayOrder {
        /// The configured initial delay.
        initial: Duration,
        /// The configured maximum delay.
        max: Duration,
    },
    /// `issue.factor` is below 1 (the delay would shrink) or not a number.
    #[error("issue.factor ({0}) must be a number of at least 1")]
    InvalidFactor(f32),
}

/// The szamlazz.hu account.
#[derive(Debug, Clone, Deserialize)]
pub struct AccountConfig {
    /// The deployment's [`Namespace`], the external-id prefix
    /// (`{namespace}:{order}:{kind}`). The key is `slug` for now.
    pub slug: Namespace,
    /// The Agent key (`számlaagentkulcs`).
    pub agent_key: Secret,
    /// The Számla Agent endpoint; `None` uses the production URL.
    #[serde(default)]
    pub endpoint: Option<String>,
    /// Whether the account is a live or a test account; validated against
    /// `teszt` on every document found under our external ids.
    #[serde(default)]
    pub mode: AccountMode,
    /// The account's supplier id (`szállító/id`). Optional pin; when set it is
    /// validated against every document found under our external ids.
    #[serde(default)]
    pub supplier_id: Option<u64>,
}

/// Whether the account is live or a test account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountMode {
    /// A live account issuing legal documents.
    #[default]
    Live,
    /// A szamlazz.hu test account (`teszt`).
    Test,
}

impl AccountMode {
    /// Whether documents of this account carry `teszt = true`.
    #[must_use]
    pub const fn is_test(self) -> bool {
        matches!(self, Self::Test)
    }
}

/// The namespace: the external-id prefix of this deployment, 1–16 bytes of
/// `[a-z0-9-]`.
///
/// Chosen by the operator, opaque to szamlazz.hu and permanent: every
/// external id the deployment issues starts with it, so changing it would
/// hide every document issued so far. `:` is excluded because it is the
/// external-id separator.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Namespace(String);

impl Namespace {
    /// The maximum length in bytes.
    pub const MAX_LEN: usize = 16;

    /// The namespace as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(value: &str) -> Result<(), InvalidNamespace> {
        if value.is_empty() {
            return Err(InvalidNamespace::Empty);
        }
        if value.len() > Self::MAX_LEN {
            return Err(InvalidNamespace::TooLong(value.len()));
        }
        if let Some(invalid) = value
            .chars()
            .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-'))
        {
            return Err(InvalidNamespace::InvalidChar(invalid));
        }
        Ok(())
    }
}

impl FromStr for Namespace {
    type Err = InvalidNamespace;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::validate(value)?;
        Ok(Self(value.to_owned()))
    }
}

impl TryFrom<String> for Namespace {
    type Error = InvalidNamespace;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::validate(&value)?;
        Ok(Self(value))
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Namespace {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Serializes as the plain string.
impl Serialize for Namespace {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// Deserializes from a string, rejecting invalid namespaces.
impl<'de> Deserialize<'de> for Namespace {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// A string that is not a valid [`Namespace`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidNamespace {
    /// The namespace is empty.
    #[error("namespace must not be empty")]
    Empty,
    /// The namespace exceeds [`Namespace::MAX_LEN`] bytes.
    #[error("namespace is {0} bytes long, at most {max} are allowed", max = Namespace::MAX_LEN)]
    TooLong(usize),
    /// A character is outside `[a-z0-9-]`.
    #[error("namespace may only contain lowercase ASCII letters, digits and '-', found {0:?}")]
    InvalidChar(char),
}

/// A secret string whose `Debug` output is redacted.
///
/// Deserializes from a string or an integer: agent keys may be all digits,
/// and an unquoted one is a number to TOML and YAML.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Wraps a secret.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The secret in clear text.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StringOrInteger;

        impl serde::de::Visitor<'_> for StringOrInteger {
            type Value = Secret;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a string or an integer")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(Secret::from(value))
            }

            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(Secret::from(value))
            }

            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(Secret::from(value.to_string()))
            }

            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(Secret::from(value.to_string()))
            }
        }

        deserializer.deserialize_any(StringOrInteger)
    }
}

impl From<String> for Secret {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for Secret {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Document defaults; [`DocumentOverrides`](crate::contract::DocumentOverrides)
/// may change the first seven per call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Defaults {
    /// Issue e-invoices (`e-számla`). Default `false`.
    pub e_invoice: bool,
    /// Document language code. Default `hu`.
    pub language: String,
    /// Currency code. Default `HUF`.
    pub currency: String,
    /// Quoting bank for non-HUF documents without an explicit rate. Default
    /// `MNB`.
    pub exchange_rate_bank: String,
    /// PDF template token.
    pub template: Option<String>,
    /// Whether szamlazz.hu should email documents to buyers.
    pub send_email: Option<bool>,
    /// Invoice number prefix (`számlaszám előtag`).
    pub number_prefix: Option<String>,
    /// Additional logo token configured on the account.
    pub extra_logo: Option<String>,
    /// Aggregator identifier for contracted integrations; not overridable per
    /// call.
    pub aggregator: Option<String>,
    /// Guardian processing flag for contracted integrations; not overridable
    /// per call.
    pub guardian: Option<bool>,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            e_invoice: false,
            language: "hu".to_owned(),
            currency: "HUF".to_owned(),
            exchange_rate_bank: "MNB".to_owned(),
            template: None,
            send_email: None,
            number_prefix: None,
            extra_logo: None,
            aggregator: None,
            guardian: None,
        }
    }
}

/// The seller (`eladó`) block; the account's own data is used where absent.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SellerConfig {
    /// Bank name.
    pub bank: Option<String>,
    /// Bank account number.
    pub bank_account: Option<String>,
    /// Name of the signer shown on documents.
    pub signer_name: Option<String>,
    /// The notification email szamlazz.hu sends to buyers.
    pub email: SellerEmailConfig,
}

impl SellerConfig {
    /// The Agent seller block. The email block is present only when at least
    /// one of its fields is set.
    #[must_use]
    pub fn to_seller(&self) -> Seller {
        let mut seller = Seller::default();
        seller.bank.clone_from(&self.bank);
        seller.bank_account.clone_from(&self.bank_account);
        seller.signer_name.clone_from(&self.signer_name);
        seller.email = self.email.to_seller_email();
        seller
    }
}

/// Settings of the notification email szamlazz.hu sends to buyers.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SellerEmailConfig {
    /// Reply-to address.
    pub reply_to: Option<String>,
    /// Subject.
    pub subject: Option<String>,
    /// Body; supports `BBCode`.
    pub body: Option<String>,
}

impl SellerEmailConfig {
    /// The Agent email block, or `None` when nothing is configured.
    #[must_use]
    pub fn to_seller_email(&self) -> Option<SellerEmail> {
        if self.reply_to.is_none() && self.subject.is_none() && self.body.is_none() {
            return None;
        }
        let mut email = SellerEmail::default();
        email.reply_to.clone_from(&self.reply_to);
        email.subject.clone_from(&self.subject);
        email.body.clone_from(&self.body);
        Some(email)
    }
}

/// The issue policy: the run retry policy of the create step (design §5
/// step 4). Restate re-executes the step after `initial_delay`, multiplying
/// the delay by `factor` up to `max_delay`, until `max_attempts` executions or
/// `max_duration` — then the step fails and the handler reports
/// `outcome_unknown`. The policy shapes no journal entry.
///
/// Durations are written as `"90s"`, `"2m"`, `"1h"` or a plain number of
/// seconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct IssueConfig {
    /// Executions of the create step, including the first. Default `5`.
    pub max_attempts: u32,
    /// Delay before the first re-execution. Default `2m`: longer than the
    /// client timeout plus the longest observed server stall, so a request
    /// still in flight has resolved by the time the re-check runs.
    #[serde(with = "duration_str")]
    pub initial_delay: Duration,
    /// Multiplier of the delay after each re-execution. Default `2.0`.
    pub factor: f32,
    /// Cap of the delay. Default `10m`.
    #[serde(with = "duration_str")]
    pub max_delay: Duration,
    /// Hard bound on the time spent re-executing the step. Default `1h`.
    #[serde(with = "duration_str")]
    pub max_duration: Duration,
}

impl Default for IssueConfig {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            initial_delay: Duration::from_mins(2),
            factor: 2.0,
            max_delay: Duration::from_mins(10),
            max_duration: Duration::from_hours(1),
        }
    }
}

impl IssueConfig {
    /// The policy as the SDK's run retry policy, every field set from this
    /// configuration. Built on [`RunRetryPolicy::new`], whose factor is 1.0
    /// and which caps nothing — not on `default()`, which caps the delay at
    /// 2 s and the duration at 50 s.
    #[must_use]
    pub fn run_retry_policy(&self) -> RunRetryPolicy {
        RunRetryPolicy::new()
            .initial_delay(self.initial_delay)
            .exponentiation_factor(self.factor)
            .max_delay(self.max_delay)
            .max_attempts(self.max_attempts)
            .max_duration(self.max_duration)
    }
}

/// Parses a duration written as `"90s"`, `"2m"`, `"1h"` or a plain number of
/// seconds.
///
/// # Errors
///
/// Returns an error for an empty string, an unknown suffix, a non-integer
/// amount or an amount that overflows.
pub fn parse_duration(value: &str) -> Result<Duration, InvalidDuration> {
    let value = value.trim();
    if value.is_empty() {
        return Err(InvalidDuration::Empty);
    }
    let (amount, multiplier) = match value.as_bytes()[value.len() - 1] {
        b's' => (&value[..value.len() - 1], 1),
        b'm' => (&value[..value.len() - 1], 60),
        b'h' => (&value[..value.len() - 1], 60 * 60),
        b'0'..=b'9' => (value, 1),
        other => return Err(InvalidDuration::UnknownUnit(char::from(other))),
    };
    let amount: u64 = amount
        .parse()
        .map_err(|_| InvalidDuration::InvalidAmount(amount.to_owned()))?;
    amount
        .checked_mul(multiplier)
        .map(Duration::from_secs)
        .ok_or(InvalidDuration::Overflow)
}

/// Formats a whole-second duration in the largest unit that divides it
/// evenly: `"1h"`, `"2m"`, `"90s"`. Sub-second precision is dropped.
#[must_use]
pub fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs > 0 && secs.is_multiple_of(3600) {
        format!("{}h", secs / 3600)
    } else if secs > 0 && secs.is_multiple_of(60) {
        format!("{}m", secs / 60)
    } else {
        format!("{secs}s")
    }
}

/// A string that is not a valid duration.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidDuration {
    /// The string is empty.
    #[error("duration must not be empty")]
    Empty,
    /// The suffix is not one of `s`, `m`, `h`.
    #[error("unknown duration unit {0:?}; use s, m or h")]
    UnknownUnit(char),
    /// The amount before the suffix is not a non-negative integer.
    #[error("invalid duration amount {0:?}")]
    InvalidAmount(String),
    /// The amount does not fit in seconds.
    #[error("duration is too large")]
    Overflow,
}

/// `#[serde(with)]` helper for durations in the `"2m"` string form.
mod duration_str {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(
        duration: &Duration,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&super::format_duration(*duration))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Duration, D::Error> {
        let value = String::deserialize(deserializer)?;
        super::parse_duration(&value).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn full_config_parses() {
        let config: Config = serde_json::from_value(json!({
            "account": {
                "slug": "acct-1",
                "agent_key": "key",
                "endpoint": "http://127.0.0.1:1234/szamla/",
                "mode": "test",
                "supplier_id": 972_720,
            },
            "defaults": {
                "e_invoice": true,
                "language": "en",
                "currency": "EUR",
                "exchange_rate_bank": "OTP",
                "template": "SzlaMost",
                "send_email": true,
                "number_prefix": "WEB",
                "extra_logo": "logo",
                "aggregator": "agg",
                "guardian": true,
            },
            "seller": {
                "bank": "Bank",
                "bank_account": "1234",
                "signer_name": "Signer",
                "email": {"reply_to": "r@e.hu", "subject": "S", "body": "B"},
            },
            "issue": {
                "max_attempts": 3,
                "initial_delay": "90s",
                "factor": 1.5,
                "max_delay": "1h",
                "max_duration": "2h",
            },
        }))
        .expect("parse");

        assert_eq!(config.account.slug.as_str(), "acct-1");
        assert_eq!(config.account.agent_key.expose(), "key");
        assert_eq!(
            config.account.endpoint.as_deref(),
            Some("http://127.0.0.1:1234/szamla/")
        );
        assert_eq!(config.account.mode, AccountMode::Test);
        assert!(config.account.mode.is_test());
        assert_eq!(config.account.supplier_id, Some(972_720));
        assert!(config.defaults.e_invoice);
        assert_eq!(config.defaults.language, "en");
        assert_eq!(config.defaults.currency, "EUR");
        assert_eq!(config.defaults.exchange_rate_bank, "OTP");
        assert_eq!(config.defaults.guardian, Some(true));
        assert_eq!(config.seller.bank.as_deref(), Some("Bank"));
        assert_eq!(config.seller.email.subject.as_deref(), Some("S"));
        assert_eq!(config.issue.max_attempts, 3);
        assert_eq!(config.issue.initial_delay, Duration::from_secs(90));
        assert_eq!(config.issue.factor.to_bits(), 1.5f32.to_bits());
        assert_eq!(config.issue.max_delay, Duration::from_secs(3600));
        assert_eq!(config.issue.max_duration, Duration::from_secs(7200));
        config.validate().expect("valid");

        let seller = config.seller.to_seller();
        assert_eq!(seller.bank_account.as_deref(), Some("1234"));
        assert_eq!(seller.signer_name.as_deref(), Some("Signer"));
        let email = seller.email.expect("email block");
        assert_eq!(email.reply_to.as_deref(), Some("r@e.hu"));
        assert_eq!(email.body.as_deref(), Some("B"));
    }

    #[test]
    fn minimal_config_uses_spec_defaults() {
        let config: Config = serde_json::from_value(json!({
            "account": {"slug": "acct", "agent_key": "key"},
        }))
        .expect("parse");

        assert_eq!(config.account.endpoint, None);
        assert_eq!(config.account.mode, AccountMode::Live);
        assert_eq!(config.account.supplier_id, None);
        assert_eq!(config.defaults, Defaults::default());
        assert!(!config.defaults.e_invoice);
        assert_eq!(config.defaults.language, "hu");
        assert_eq!(config.defaults.currency, "HUF");
        assert_eq!(config.defaults.exchange_rate_bank, "MNB");
        assert_eq!(config.defaults.template, None);
        assert_eq!(config.seller, SellerConfig::default());
        assert_eq!(config.seller.to_seller().email, None);
        assert_eq!(config.issue, IssueConfig::default());
        assert_eq!(config.issue.max_attempts, 5);
        assert_eq!(config.issue.initial_delay, Duration::from_secs(120));
        assert_eq!(config.issue.factor.to_bits(), 2.0f32.to_bits());
        assert_eq!(config.issue.max_delay, Duration::from_secs(600));
        assert_eq!(config.issue.max_duration, Duration::from_secs(3600));
        config.validate().expect("valid");
    }

    /// The namespace is 1–16 bytes of `[a-z0-9-]`; `:` is excluded because it
    /// is the external-id separator.
    #[test]
    fn namespace_rule_is_enforced_at_parse_time() {
        for accepted in ["a", "acct", "acct-1", "0", "a".repeat(16).as_str()] {
            let namespace: Namespace = accepted.parse().expect(accepted);
            assert_eq!(namespace.as_str(), accepted);
            assert_eq!(namespace.to_string(), accepted);
            let config: Config = serde_json::from_value(json!({
                "account": {"slug": accepted, "agent_key": "key"},
            }))
            .expect(accepted);
            assert_eq!(config.account.slug, namespace);
        }

        let too_long = "a".repeat(17);
        let rejected = [
            ("", InvalidNamespace::Empty),
            ("Acct", InvalidNamespace::InvalidChar('A')),
            ("acct_1", InvalidNamespace::InvalidChar('_')),
            ("acct 1", InvalidNamespace::InvalidChar(' ')),
            ("acct:1", InvalidNamespace::InvalidChar(':')),
            ("ácct", InvalidNamespace::InvalidChar('á')),
            (too_long.as_str(), InvalidNamespace::TooLong(17)),
        ];
        for (input, expected) in rejected {
            assert_eq!(
                input.parse::<Namespace>(),
                Err(expected.clone()),
                "{input:?}"
            );
            assert_eq!(Namespace::try_from(input.to_owned()), Err(expected));
            let result = serde_json::from_value::<Config>(json!({
                "account": {"slug": input, "agent_key": "key"},
            }));
            assert!(result.is_err(), "{input:?} should be rejected");
        }
    }

    /// The services hold only the deployment-level settings: the namespace
    /// and the issue policy, taken from the parsed configuration.
    #[test]
    fn worker_config_is_the_namespace_and_the_issue_policy() {
        let config: Config = serde_json::from_value(json!({
            "account": {"slug": "acct-1", "agent_key": "key", "mode": "test"},
            "issue": {"max_attempts": 3, "initial_delay": "90s", "max_delay": "1h"},
        }))
        .expect("parse");

        let worker = WorkerConfig::from(&config);
        assert_eq!(worker.namespace.as_str(), "acct-1");
        assert_eq!(worker.issue.max_attempts, 3);
        assert_eq!(worker.issue.initial_delay, Duration::from_secs(90));
        assert_eq!(worker.issue.max_delay, Duration::from_secs(3600));
        assert_eq!(
            worker.issue.factor.to_bits(),
            2.0f32.to_bits(),
            "unset fields keep their defaults"
        );
        assert_eq!(
            worker,
            WorkerConfig {
                namespace: "acct-1".parse().expect("namespace"),
                issue: config.issue.clone(),
            }
        );
    }

    /// The issue policy is the create step's run retry policy, every field
    /// set: `RunRetryPolicy::new()` has factor 1.0 and no caps, and
    /// `default()` caps at 2 s / 50 s — neither is what the policy says.
    #[test]
    fn issue_policy_maps_to_the_run_retry_policy_field_for_field() {
        assert_eq!(
            format!("{:?}", IssueConfig::default().run_retry_policy()),
            "RunRetryPolicy { initial_delay: 120s, factor: 2.0, max_delay: Some(600s), \
             max_attempts: Some(5), max_duration: Some(3600s) }"
        );
        let short = IssueConfig {
            max_attempts: 2,
            initial_delay: Duration::from_secs(1),
            factor: 1.5,
            max_delay: Duration::from_secs(2),
            max_duration: Duration::from_secs(30),
        };
        assert_eq!(
            format!("{:?}", short.run_retry_policy()),
            "RunRetryPolicy { initial_delay: 1s, factor: 1.5, max_delay: Some(2s), \
             max_attempts: Some(2), max_duration: Some(30s) }"
        );
    }

    #[test]
    fn validate_reports_invariants() {
        fn config(issue: &serde_json::Value, agent_key: &str) -> Config {
            serde_json::from_value(json!({
                "account": {"slug": "acct", "agent_key": agent_key},
                "issue": issue,
            }))
            .expect("parse")
        }

        assert_eq!(
            config(&json!({}), " ").validate(),
            Err(ConfigError::EmptyAgentKey)
        );
        assert_eq!(
            config(&json!({"max_attempts": 0}), "key").validate(),
            Err(ConfigError::ZeroMaxAttempts)
        );
        assert_eq!(
            config(&json!({"initial_delay": "11m"}), "key").validate(),
            Err(ConfigError::DelayOrder {
                initial: Duration::from_mins(11),
                max: Duration::from_secs(600),
            })
        );
        assert_eq!(
            config(&json!({"initial_delay": "10m"}), "key").validate(),
            Ok(())
        );
        assert_eq!(
            config(&json!({"factor": 0.5}), "key").validate(),
            Err(ConfigError::InvalidFactor(0.5))
        );
        assert_eq!(config(&json!({"factor": 1.0}), "key").validate(), Ok(()));
    }

    #[test]
    fn duration_parsing_table() {
        let cases = [
            ("2m", 120),
            ("90s", 90),
            ("10m", 600),
            ("1h", 3600),
            ("45", 45),
            ("0s", 0),
            (" 3m ", 180),
        ];
        for (input, secs) in cases {
            assert_eq!(
                parse_duration(input),
                Ok(Duration::from_secs(secs)),
                "{input:?}"
            );
        }
        assert_eq!(parse_duration(""), Err(InvalidDuration::Empty));
        assert_eq!(parse_duration("2d"), Err(InvalidDuration::UnknownUnit('d')));
        assert_eq!(
            parse_duration("m"),
            Err(InvalidDuration::InvalidAmount(String::new()))
        );
        assert_eq!(
            parse_duration("1.5m"),
            Err(InvalidDuration::InvalidAmount("1.5".to_owned()))
        );
        assert_eq!(
            parse_duration("-1s"),
            Err(InvalidDuration::InvalidAmount("-1".to_owned()))
        );
        assert_eq!(
            parse_duration(&format!("{}h", u64::MAX)),
            Err(InvalidDuration::Overflow)
        );
    }

    #[test]
    fn duration_formats_in_largest_even_unit() {
        assert_eq!(format_duration(Duration::from_secs(3600)), "1h");
        assert_eq!(format_duration(Duration::from_secs(7200)), "2h");
        assert_eq!(format_duration(Duration::from_secs(120)), "2m");
        assert_eq!(format_duration(Duration::from_secs(90)), "90s");
        assert_eq!(format_duration(Duration::from_secs(0)), "0s");
        assert_eq!(format_duration(Duration::from_millis(1500)), "1s");
    }

    #[test]
    fn issue_config_round_trips_durations_as_strings() {
        let json = serde_json::to_value(IssueConfig::default()).expect("serialize");
        assert_eq!(json["initial_delay"], "2m");
        assert_eq!(json["max_delay"], "10m");
        assert_eq!(json["max_duration"], "1h");
        let back: IssueConfig = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, IssueConfig::default());
        assert!(serde_json::from_value::<IssueConfig>(json!({"initial_delay": 120})).is_err());
    }

    #[test]
    fn secret_debug_is_redacted() {
        let config: Config = serde_json::from_value(json!({
            "account": {"slug": "acct", "agent_key": "hunter2"},
        }))
        .expect("parse");
        let debug = format!("{config:?}");
        assert!(!debug.contains("hunter2"), "{debug}");
        assert!(debug.contains("Secret(***)"));
        assert_eq!(format!("{:?}", Secret::new("x")), "Secret(***)");
        assert_eq!(Secret::from("x"), Secret::from("x".to_owned()));
    }
}
