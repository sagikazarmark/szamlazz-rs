//! Deployment configuration: everything that is constant for a deployment and
//! therefore never travels in a request payload.
//!
//! ```toml
//! [account]
//! slug = "acct"
//! agent_key = "..."
//! fp_secret = "..."
//!
//! [defaults]
//! language = "hu"
//! currency = "HUF"
//!
//! [issue]
//! max_attempts = 5
//! first_backoff = "2m"
//! max_backoff = "10m"
//! ```
//!
//! The types only implement `Deserialize`; the endpoint binary chooses the
//! file format and environment merging.

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use szamlazz_agent::ops::invoice::{Seller, SellerEmail};

/// The complete deployment configuration.
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
    /// Issuing attempt budget and backoff.
    #[serde(default)]
    pub issue: IssueConfig,
}

impl Config {
    /// Checks the cross-field invariants that `Deserialize` cannot express.
    ///
    /// The account slug is validated when parsed and needs no further check.
    ///
    /// # Errors
    ///
    /// Returns the first violated invariant: an empty agent key or fingerprint
    /// secret, `issue.max_attempts == 0`, or `issue.first_backoff` greater
    /// than `issue.max_backoff`.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.account.agent_key.expose().trim().is_empty() {
            return Err(ConfigError::EmptyAgentKey);
        }
        if self.account.fp_secret.expose().is_empty() {
            return Err(ConfigError::EmptyFingerprintSecret);
        }
        if self.issue.max_attempts == 0 {
            return Err(ConfigError::ZeroMaxAttempts);
        }
        if self.issue.first_backoff > self.issue.max_backoff {
            return Err(ConfigError::BackoffOrder {
                first: self.issue.first_backoff,
                max: self.issue.max_backoff,
            });
        }
        Ok(())
    }
}

/// A [`Config`] that parsed but violates an invariant.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// `account.agent_key` is empty or blank.
    #[error("account.agent_key must not be empty")]
    EmptyAgentKey,
    /// `account.fp_secret` is empty.
    #[error("account.fp_secret must not be empty")]
    EmptyFingerprintSecret,
    /// `issue.max_attempts` is zero.
    #[error("issue.max_attempts must be at least 1")]
    ZeroMaxAttempts,
    /// `issue.first_backoff` exceeds `issue.max_backoff`.
    #[error("issue.first_backoff ({first:?}) must not exceed issue.max_backoff ({max:?})")]
    BackoffOrder {
        /// The configured first backoff.
        first: Duration,
        /// The configured maximum backoff.
        max: Duration,
    },
}

/// The szamlazz.hu account.
#[derive(Debug, Clone, Deserialize)]
pub struct AccountConfig {
    /// Short name that namespaces external ids (`{slug}:{order}:{kind}:{gen}`).
    pub slug: AccountSlug,
    /// The Agent key (`számlaagentkulcs`).
    pub agent_key: Secret,
    /// The Számla Agent endpoint; `None` uses the production URL.
    #[serde(default)]
    pub endpoint: Option<String>,
    /// Whether the account is a live or a test account; validated against
    /// `teszt` on every adopted document.
    #[serde(default)]
    pub mode: AccountMode,
    /// The account's supplier id (`szállító/id`). Optional pin; otherwise
    /// learned from the first query and stored in the ledger.
    #[serde(default)]
    pub supplier_id: Option<u64>,
    /// Key of the payload fingerprint HMAC. Rotating it invalidates stored
    /// fingerprints, so every repeat request reports `payload_mismatch`.
    pub fp_secret: Secret,
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

/// The account slug: 1–16 characters of `[a-z0-9-]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AccountSlug(String);

impl AccountSlug {
    /// The maximum length in bytes.
    pub const MAX_LEN: usize = 16;

    /// The slug as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(value: &str) -> Result<(), InvalidAccountSlug> {
        if value.is_empty() {
            return Err(InvalidAccountSlug::Empty);
        }
        if value.len() > Self::MAX_LEN {
            return Err(InvalidAccountSlug::TooLong(value.len()));
        }
        if let Some(invalid) = value
            .chars()
            .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-'))
        {
            return Err(InvalidAccountSlug::InvalidChar(invalid));
        }
        Ok(())
    }
}

impl FromStr for AccountSlug {
    type Err = InvalidAccountSlug;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::validate(value)?;
        Ok(Self(value.to_owned()))
    }
}

impl TryFrom<String> for AccountSlug {
    type Error = InvalidAccountSlug;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::validate(&value)?;
        Ok(Self(value))
    }
}

impl fmt::Display for AccountSlug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for AccountSlug {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Serializes as the plain string.
impl Serialize for AccountSlug {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// Deserializes from a string, rejecting invalid slugs.
impl<'de> Deserialize<'de> for AccountSlug {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// A string that is not a valid [`AccountSlug`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidAccountSlug {
    /// The slug is empty.
    #[error("account slug must not be empty")]
    Empty,
    /// The slug exceeds [`AccountSlug::MAX_LEN`] bytes.
    #[error("account slug is {0} bytes long, at most {max} are allowed", max = AccountSlug::MAX_LEN)]
    TooLong(usize),
    /// A character is outside `[a-z0-9-]`.
    #[error("account slug may only contain lowercase ASCII letters, digits and '-', found {0:?}")]
    InvalidChar(char),
}

/// A secret string whose `Debug` output is redacted.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
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

/// Issuing attempt budget and backoff.
///
/// Durations are written as `"90s"`, `"2m"`, `"1h"` or a plain number of
/// seconds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct IssueConfig {
    /// Attempts per invocation before `outcome_unknown`. Default `5`.
    pub max_attempts: u32,
    /// Sleep after the first failed attempt (and the pre-sleep before
    /// reconciling a `pending` slot). Default `2m`.
    #[serde(with = "duration_str")]
    pub first_backoff: Duration,
    /// Cap of the doubling backoff. Default `10m`.
    #[serde(with = "duration_str")]
    pub max_backoff: Duration,
    /// Query the order number before the first attempt to detect foreign
    /// documents. Default `true`. The hint is taken regardless when
    /// `options.proforma == ledger`.
    pub detect_foreign: bool,
}

impl Default for IssueConfig {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            first_backoff: Duration::from_mins(2),
            max_backoff: Duration::from_mins(10),
            detect_foreign: true,
        }
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
                "fp_secret": "fp",
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
                "first_backoff": "90s",
                "max_backoff": "1h",
                "detect_foreign": false,
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
        assert_eq!(config.account.fp_secret.expose(), "fp");
        assert!(config.defaults.e_invoice);
        assert_eq!(config.defaults.language, "en");
        assert_eq!(config.defaults.currency, "EUR");
        assert_eq!(config.defaults.exchange_rate_bank, "OTP");
        assert_eq!(config.defaults.guardian, Some(true));
        assert_eq!(config.seller.bank.as_deref(), Some("Bank"));
        assert_eq!(config.seller.email.subject.as_deref(), Some("S"));
        assert_eq!(config.issue.max_attempts, 3);
        assert_eq!(config.issue.first_backoff, Duration::from_secs(90));
        assert_eq!(config.issue.max_backoff, Duration::from_secs(3600));
        assert!(!config.issue.detect_foreign);
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
            "account": {"slug": "acct", "agent_key": "key", "fp_secret": "fp"},
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
        assert_eq!(config.issue.first_backoff, Duration::from_secs(120));
        assert_eq!(config.issue.max_backoff, Duration::from_secs(600));
        assert!(config.issue.detect_foreign);
        config.validate().expect("valid");
    }

    #[test]
    fn invalid_slug_is_rejected_at_parse_time() {
        for slug in [
            "",
            "Acct",
            "acct_1",
            "acct 1",
            "a".repeat(17).as_str(),
            "ácct",
        ] {
            let result = serde_json::from_value::<Config>(json!({
                "account": {"slug": slug, "agent_key": "key", "fp_secret": "fp"},
            }));
            assert!(result.is_err(), "{slug:?} should be rejected");
        }
        assert_eq!("".parse::<AccountSlug>(), Err(InvalidAccountSlug::Empty));
        assert_eq!(
            "Acct".parse::<AccountSlug>(),
            Err(InvalidAccountSlug::InvalidChar('A'))
        );
        assert_eq!(
            "a".repeat(17).parse::<AccountSlug>(),
            Err(InvalidAccountSlug::TooLong(17))
        );
        let slug: AccountSlug = "a".repeat(16).parse().expect("16 chars are allowed");
        assert_eq!(slug.to_string(), "a".repeat(16));
    }

    #[test]
    fn validate_reports_invariants() {
        fn config(issue: &serde_json::Value, agent_key: &str, fp_secret: &str) -> Config {
            serde_json::from_value(json!({
                "account": {"slug": "acct", "agent_key": agent_key, "fp_secret": fp_secret},
                "issue": issue,
            }))
            .expect("parse")
        }

        assert_eq!(
            config(&json!({}), " ", "fp").validate(),
            Err(ConfigError::EmptyAgentKey)
        );
        assert_eq!(
            config(&json!({}), "key", "").validate(),
            Err(ConfigError::EmptyFingerprintSecret)
        );
        assert_eq!(
            config(&json!({"max_attempts": 0}), "key", "fp").validate(),
            Err(ConfigError::ZeroMaxAttempts)
        );
        assert_eq!(
            config(&json!({"first_backoff": "11m"}), "key", "fp").validate(),
            Err(ConfigError::BackoffOrder {
                first: Duration::from_mins(11),
                max: Duration::from_secs(600),
            })
        );
        assert_eq!(
            config(&json!({"first_backoff": "10m"}), "key", "fp").validate(),
            Ok(())
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
        assert_eq!(json["first_backoff"], "2m");
        assert_eq!(json["max_backoff"], "10m");
        let back: IssueConfig = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, IssueConfig::default());
        assert!(serde_json::from_value::<IssueConfig>(json!({"first_backoff": 120})).is_err());
    }

    #[test]
    fn secret_debug_is_redacted() {
        let config: Config = serde_json::from_value(json!({
            "account": {"slug": "acct", "agent_key": "hunter2", "fp_secret": "fp-hunter2"},
        }))
        .expect("parse");
        let debug = format!("{config:?}");
        assert!(!debug.contains("hunter2"), "{debug}");
        assert!(debug.contains("Secret(***)"));
        assert_eq!(format!("{:?}", Secret::new("x")), "Secret(***)");
        assert_eq!(Secret::from("x"), Secret::from("x".to_owned()));
    }
}
