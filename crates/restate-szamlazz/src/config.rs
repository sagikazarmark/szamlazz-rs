//! Deployment-level configuration: what is constant for a deployment, is not
//! account-shaped, and therefore neither travels in a request payload nor
//! routes through the gateway.
//!
//! ```toml
//! namespace = "acct"            # the external-id prefix; permanent
//!
//! [issue]                       # the run retry policy of the create and storno steps
//! max_attempts = 5
//! initial_delay = "2m"
//! factor = 2.0
//! max_delay = "10m"
//! max_duration = "1h"
//!
//! [resolve]                     # the run retry policy of the `account` step
//! initial_delay = "1s"
//! factor = 2.0
//! max_delay = "10s"
//! max_duration = "1m"
//! ```
//!
//! The types only implement `Deserialize`; the endpoint binary chooses the
//! file format and environment merging, and merges the static resolver's
//! account configuration ([`StaticConfig`](crate::account::StaticConfig))
//! beside these keys. Everything account-shaped — credentials, mode, supplier
//! pin, endpoint, document defaults, seller block — lives on the
//! [`Account`](crate::account::Account) a resolver produces, and the services
//! read it through [`Gateway::account`](crate::gateway::Gateway::account).
//! The account-level building blocks a resolver's configuration reuses
//! ([`AccountMode`], [`Defaults`], [`SellerConfig`], [`Secret`]) are defined
//! here.

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use restate_sdk::context::RunRetryPolicy;
use serde::{Deserialize, Serialize};
use szamlazz_agent::ops::invoice::{Seller, SellerEmail};

/// The deployment-level settings the Restate services hold: what is not
/// account-shaped and therefore does not route through the gateway.
///
/// The namespace prefixes every external id the deployment issues; the issue
/// policy is the run retry policy of the create and storno steps; the resolve
/// policy is the run retry policy of the `account` step. Both policies
/// default when absent. Call [`validate`](Self::validate) after parsing for
/// the cross-field invariants `Deserialize` cannot express.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WorkerConfig {
    /// The external-id prefix of this deployment (`{namespace}:{order}:{kind}`).
    pub namespace: Namespace,
    /// The issue policy: the run retry policy of the create and storno steps.
    #[serde(default)]
    pub issue: IssueConfig,
    /// The resolve policy: the run retry policy of the `account` step.
    #[serde(default)]
    pub resolve: ResolveConfig,
}

impl WorkerConfig {
    /// The settings for `namespace` with the default issue and resolve
    /// policies.
    #[must_use]
    pub fn new(namespace: Namespace) -> Self {
        Self {
            namespace,
            issue: IssueConfig::default(),
            resolve: ResolveConfig::default(),
        }
    }

    /// Checks the cross-field invariants that `Deserialize` cannot express.
    ///
    /// The namespace is validated when parsed and needs no further check.
    ///
    /// # Errors
    ///
    /// Returns the first violated invariant: `issue.max_attempts == 0`, an
    /// `initial_delay` greater than the `max_delay` of the same policy, or a
    /// `factor` below 1 on either policy.
    pub fn validate(&self) -> Result<(), WorkerConfigError> {
        if self.issue.max_attempts == 0 {
            return Err(WorkerConfigError::ZeroMaxAttempts);
        }
        for (policy, initial, max, factor) in [
            (
                Policy::Issue,
                self.issue.initial_delay,
                self.issue.max_delay,
                self.issue.factor,
            ),
            (
                Policy::Resolve,
                self.resolve.initial_delay,
                self.resolve.max_delay,
                self.resolve.factor,
            ),
        ] {
            if initial > max {
                return Err(WorkerConfigError::DelayOrder {
                    policy,
                    initial,
                    max,
                });
            }
            if factor.is_nan() || factor < 1.0 {
                return Err(WorkerConfigError::InvalidFactor { policy, factor });
            }
        }
        Ok(())
    }
}

/// A [`WorkerConfig`] that parsed but violates an invariant.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum WorkerConfigError {
    /// `issue.max_attempts` is zero.
    #[error("issue.max_attempts must be at least 1")]
    ZeroMaxAttempts,
    /// A policy's `initial_delay` exceeds its `max_delay`.
    #[error("{policy}.initial_delay ({initial:?}) must not exceed {policy}.max_delay ({max:?})")]
    DelayOrder {
        /// The policy.
        policy: Policy,
        /// The configured initial delay.
        initial: Duration,
        /// The configured maximum delay.
        max: Duration,
    },
    /// A policy's `factor` is below 1 (the delay would shrink) or not a
    /// number.
    #[error("{policy}.factor ({factor}) must be a number of at least 1")]
    InvalidFactor {
        /// The policy.
        policy: Policy,
        /// The configured factor.
        factor: f32,
    },
}

/// One of the two run retry policies of a [`WorkerConfig`]; names the
/// configuration table in a [`WorkerConfigError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Policy {
    /// `[issue]`, the [`IssueConfig`].
    Issue,
    /// `[resolve]`, the [`ResolveConfig`].
    Resolve,
}

impl fmt::Display for Policy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Issue => "issue",
            Self::Resolve => "resolve",
        })
    }
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
/// step 4) and the storno step (§6 step 3). Restate re-executes the step after
/// `initial_delay`, multiplying the delay by `factor` up to `max_delay`, until
/// `max_attempts` executions or `max_duration` — then the step fails and the
/// handler reports `outcome_unknown`. The policy shapes no journal entry.
///
/// Durations are written as `"90s"`, `"2m"`, `"1h"` or a plain number of
/// seconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct IssueConfig {
    /// Executions of the step, including the first. Default `5`.
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

/// The resolve policy: the run retry policy of the `account` step of every
/// handler (design §4), which asks the account resolver for the request's
/// account. An unavailable resolver is retried under it — `initial_delay`
/// growing by `factor` to `max_delay`, bounded by `max_duration` and nothing
/// else — and its exhaustion is the `unavailable` fault. Unscoped and unknown
/// are answers, journaled as data, never retried. Shapes no journal entry.
///
/// Set explicitly for the same reason as the issue policy: the SDK's default
/// run policy sends no retry delay and the server would spend the handler's
/// `invocation_retry_policy` instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ResolveConfig {
    /// Delay before the first re-execution. Default `1s`.
    #[serde(with = "duration_str")]
    pub initial_delay: Duration,
    /// Multiplier of the delay after each re-execution. Default `2.0`.
    pub factor: f32,
    /// Cap of the delay. Default `10s`.
    #[serde(with = "duration_str")]
    pub max_delay: Duration,
    /// Hard bound on the time spent re-executing the step. Default `1m`.
    #[serde(with = "duration_str")]
    pub max_duration: Duration,
}

impl Default for ResolveConfig {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_secs(1),
            factor: 2.0,
            max_delay: Duration::from_secs(10),
            max_duration: Duration::from_mins(1),
        }
    }
}

impl ResolveConfig {
    /// The policy as the SDK's run retry policy: delays and the duration bound
    /// from this configuration, no attempt cap (the duration is the bound).
    #[must_use]
    pub fn run_retry_policy(&self) -> RunRetryPolicy {
        RunRetryPolicy::new()
            .initial_delay(self.initial_delay)
            .exponentiation_factor(self.factor)
            .max_delay(self.max_delay)
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
    fn full_worker_config_parses() {
        let config: WorkerConfig = serde_json::from_value(json!({
            "namespace": "acct-1",
            "issue": {
                "max_attempts": 3,
                "initial_delay": "90s",
                "factor": 1.5,
                "max_delay": "1h",
                "max_duration": "2h",
            },
            "resolve": {
                "initial_delay": "2s",
                "factor": 3.0,
                "max_delay": "20s",
                "max_duration": "5m",
            },
        }))
        .expect("parse");

        assert_eq!(config.namespace.as_str(), "acct-1");
        assert_eq!(config.issue.max_attempts, 3);
        assert_eq!(config.issue.initial_delay, Duration::from_secs(90));
        assert_eq!(config.issue.factor.to_bits(), 1.5f32.to_bits());
        assert_eq!(config.issue.max_delay, Duration::from_secs(3600));
        assert_eq!(config.issue.max_duration, Duration::from_secs(7200));
        assert_eq!(config.resolve.initial_delay, Duration::from_secs(2));
        assert_eq!(config.resolve.factor.to_bits(), 3.0f32.to_bits());
        assert_eq!(config.resolve.max_delay, Duration::from_secs(20));
        assert_eq!(config.resolve.max_duration, Duration::from_secs(300));
        config.validate().expect("valid");
    }

    /// Only the namespace is required; both policies default, and the parsed
    /// minimum equals [`WorkerConfig::new`].
    #[test]
    fn minimal_worker_config_is_the_namespace_with_default_policies() {
        let config: WorkerConfig =
            serde_json::from_value(json!({ "namespace": "acct" })).expect("parse");

        assert_eq!(
            config,
            WorkerConfig::new("acct".parse().expect("namespace"))
        );
        assert_eq!(config.issue, IssueConfig::default());
        assert_eq!(config.issue.max_attempts, 5);
        assert_eq!(config.issue.initial_delay, Duration::from_secs(120));
        assert_eq!(config.issue.factor.to_bits(), 2.0f32.to_bits());
        assert_eq!(config.issue.max_delay, Duration::from_secs(600));
        assert_eq!(config.issue.max_duration, Duration::from_secs(3600));
        assert_eq!(config.resolve, ResolveConfig::default());
        assert_eq!(config.resolve.initial_delay, Duration::from_secs(1));
        assert_eq!(config.resolve.max_delay, Duration::from_secs(10));
        assert_eq!(config.resolve.max_duration, Duration::from_secs(60));
        config.validate().expect("valid");

        assert!(
            serde_json::from_value::<WorkerConfig>(json!({})).is_err(),
            "the namespace has no default"
        );
    }

    /// The namespace is 1–16 bytes of `[a-z0-9-]`; `:` is excluded because it
    /// is the external-id separator.
    #[test]
    fn namespace_rule_is_enforced_at_parse_time() {
        for accepted in ["a", "acct", "acct-1", "0", "a".repeat(16).as_str()] {
            let namespace: Namespace = accepted.parse().expect(accepted);
            assert_eq!(namespace.as_str(), accepted);
            assert_eq!(namespace.to_string(), accepted);
            let config: WorkerConfig =
                serde_json::from_value(json!({ "namespace": accepted })).expect(accepted);
            assert_eq!(config.namespace, namespace);
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
            let result = serde_json::from_value::<WorkerConfig>(json!({ "namespace": input }));
            assert!(result.is_err(), "{input:?} should be rejected");
        }
    }

    /// The resolve policy is the run retry policy of the `account` step:
    /// delays and the duration bound, no attempt cap.
    #[test]
    fn resolve_policy_maps_to_the_run_retry_policy_bounded_by_duration() {
        assert_eq!(
            format!("{:?}", ResolveConfig::default().run_retry_policy()),
            "RunRetryPolicy { initial_delay: 1s, factor: 2.0, max_delay: Some(10s), \
             max_attempts: None, max_duration: Some(60s) }"
        );
        let parsed: ResolveConfig =
            serde_json::from_value(json!({"initial_delay": "2s", "max_duration": "30s"}))
                .expect("parse");
        assert_eq!(parsed.initial_delay, Duration::from_secs(2));
        assert_eq!(parsed.max_duration, Duration::from_secs(30));
        assert_eq!(parsed.max_delay, Duration::from_secs(10), "default kept");
    }

    /// The issue policy is the run retry policy of the create and storno
    /// steps, every field set: `RunRetryPolicy::new()` has factor 1.0 and no caps, and
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
    fn validate_reports_invariants_of_both_policies() {
        fn config(issue: &serde_json::Value, resolve: &serde_json::Value) -> WorkerConfig {
            serde_json::from_value(json!({
                "namespace": "acct",
                "issue": issue,
                "resolve": resolve,
            }))
            .expect("parse")
        }
        let none = json!({});

        assert_eq!(
            config(&json!({"max_attempts": 0}), &none).validate(),
            Err(WorkerConfigError::ZeroMaxAttempts)
        );
        assert_eq!(
            config(&json!({"initial_delay": "11m"}), &none).validate(),
            Err(WorkerConfigError::DelayOrder {
                policy: Policy::Issue,
                initial: Duration::from_mins(11),
                max: Duration::from_secs(600),
            })
        );
        assert_eq!(
            config(&json!({"initial_delay": "10m"}), &none).validate(),
            Ok(())
        );
        assert_eq!(
            config(&json!({"factor": 0.5}), &none).validate(),
            Err(WorkerConfigError::InvalidFactor {
                policy: Policy::Issue,
                factor: 0.5
            })
        );
        assert_eq!(config(&json!({"factor": 1.0}), &none).validate(), Ok(()));

        assert_eq!(
            config(&none, &json!({"initial_delay": "11s"})).validate(),
            Err(WorkerConfigError::DelayOrder {
                policy: Policy::Resolve,
                initial: Duration::from_secs(11),
                max: Duration::from_secs(10),
            })
        );
        assert_eq!(
            config(&none, &json!({"factor": 0.0})).validate(),
            Err(WorkerConfigError::InvalidFactor {
                policy: Policy::Resolve,
                factor: 0.0
            })
        );
        assert_eq!(
            config(&none, &json!({"initial_delay": "11s"}))
                .validate()
                .expect_err("error")
                .to_string(),
            "resolve.initial_delay (11s) must not exceed resolve.max_delay (10s)",
            "the error names the table"
        );
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
        let secret: Secret = serde_json::from_value(json!("hunter2")).expect("parse");
        let debug = format!("{secret:?}");
        assert!(!debug.contains("hunter2"), "{debug}");
        assert_eq!(debug, "Secret(***)");
        assert_eq!(secret.expose(), "hunter2");
        assert_eq!(format!("{:?}", Secret::new("x")), "Secret(***)");
        assert_eq!(Secret::from("x"), Secret::from("x".to_owned()));
        let numeric: Secret = serde_json::from_value(json!(12_345_678)).expect("parse");
        assert_eq!(numeric.expose(), "12345678");
    }

    #[test]
    fn seller_config_projects_to_the_agent_seller_block() {
        let seller: SellerConfig = serde_json::from_value(json!({
            "bank": "Bank",
            "bank_account": "1234",
            "signer_name": "Signer",
            "email": {"reply_to": "r@e.hu", "subject": "S", "body": "B"},
        }))
        .expect("parse");
        let block = seller.to_seller();
        assert_eq!(block.bank.as_deref(), Some("Bank"));
        assert_eq!(block.bank_account.as_deref(), Some("1234"));
        assert_eq!(block.signer_name.as_deref(), Some("Signer"));
        let email = block.email.expect("email block");
        assert_eq!(email.reply_to.as_deref(), Some("r@e.hu"));
        assert_eq!(email.subject.as_deref(), Some("S"));
        assert_eq!(email.body.as_deref(), Some("B"));

        assert_eq!(
            SellerConfig::default().to_seller().email,
            None,
            "no email block unless a field is set"
        );
        assert_eq!(Defaults::default().language, "hu");
        assert_eq!(Defaults::default().currency, "HUF");
        assert_eq!(Defaults::default().exchange_rate_bank, "MNB");
        assert!(!Defaults::default().e_invoice);
    }
}
