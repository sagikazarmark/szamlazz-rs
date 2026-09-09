//! The deployment-level configuration the Restate services hold:
//! [`WorkerConfig`], validated into the [`ValidatedWorkerConfig`] the
//! services are built from.
//!
//! [`WorkerConfig`] is what is constant for a deployment, is not
//! account-shaped, and therefore neither travels in a request payload nor
//! routes through the gateway: the namespace of the external ids and the
//! three run retry policies, one [`RetryPolicyConfig`] per table:
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
//! [read]                        # the run retry policy of every read-only step
//! max_attempts = 5
//! initial_delay = "5s"
//! factor = 2.0
//! max_delay = "60s"
//! max_duration = "5m"
//!
//! [resolve]                     # the run retry policy of the `account` step
//! initial_delay = "1s"         # no max_attempts: the duration is the bound
//! factor = 2.0
//! max_delay = "10s"
//! max_duration = "1m"
//! ```
//!
//! The types implement `Deserialize` only and are **closed**
//! (`#[serde(deny_unknown_fields)]`): a misspelt table or key is a parse
//! error naming it, never a policy left at its default. The host
//! chooses the file format and environment merging, and reads the static
//! resolver's account configuration
//! ([`StaticConfig`](crate::account::StaticConfig)) beside these keys.
//! Everything account-shaped (credentials, endpoint, document defaults,
//! seller block) is the [`Account`](crate::account::Account) a resolver
//! produces, read by the services through
//! [`Gateway::account`](crate::gateway::Gateway::account); its value types
//! live in [`account`](crate::account), where they are journaled.
//!
//! The policies are `#[non_exhaustive]` (deployment-level, journaled nowhere,
//! but fields may be added): build one from `Default::default()` (or
//! deserialize it) and set fields. [`WorkerConfig::validate`] is the one way
//! to a [`ValidatedWorkerConfig`], and [`Order::from_parts`](crate::Order)
//! and [`Agent::from_parts`](crate::Agent) take nothing else, so a deployment
//! cannot run on an issue policy below its floor.

use std::marker::PhantomData;
use std::ops::Deref;
use std::time::Duration;

use restate_sdk::context::RunRetryPolicy;
use serde::{Deserialize, Serialize};

use crate::identity::Namespace;

use table::Table;

/// The deployment-level settings the Restate services hold: what is not
/// account-shaped and therefore does not route through the gateway.
///
/// The namespace prefixes every external id the deployment issues; the issue
/// policy is the run retry policy of the create and storno steps; the read
/// policy is the run retry policy of every read-only step; the resolve policy
/// is the run retry policy of the `account` step. All three policies default
/// when absent. [`validate`](Self::validate) checks the cross-field
/// invariants `Deserialize` cannot express and yields the
/// [`ValidatedWorkerConfig`] the services take.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerConfig {
    /// The external-id prefix of this deployment (`{namespace}:{order}:{kind}`).
    pub namespace: Namespace,
    /// The issue policy: the run retry policy of the create and storno steps.
    #[serde(default)]
    pub issue: IssueConfig,
    /// The read policy: the run retry policy of every read-only step.
    #[serde(default)]
    pub read: ReadConfig,
    /// The resolve policy: the run retry policy of the `account` step.
    #[serde(default)]
    pub resolve: ResolveConfig,
}

impl WorkerConfig {
    /// The settings for `namespace` with the default issue, read and resolve
    /// policies.
    #[must_use]
    pub fn new(namespace: Namespace) -> Self {
        Self {
            namespace,
            issue: IssueConfig::default(),
            read: ReadConfig::default(),
            resolve: ResolveConfig::default(),
        }
    }

    /// Checks the cross-field invariants that `Deserialize` cannot express
    /// and yields the configuration the services are built from.
    ///
    /// The namespace is validated when parsed and needs no further check.
    ///
    /// # Errors
    ///
    /// Returns the first violated invariant: a `max_attempts` of zero on any
    /// policy, an issue `initial_delay` below
    /// [`IssueConfig::MIN_INITIAL_DELAY`], an `initial_delay` greater than the
    /// `max_delay` of the same policy, or a `factor` below 1 on any policy.
    pub fn validate(self) -> Result<ValidatedWorkerConfig, WorkerConfigError> {
        self.check()?;
        Ok(ValidatedWorkerConfig(self))
    }

    /// The invariants, in the order they are reported: an attempt cap of
    /// zero on any table, the issue floor (the safety rule), then each
    /// table's consistency rules.
    fn check(&self) -> Result<(), WorkerConfigError> {
        self.issue.check_attempts()?;
        self.read.check_attempts()?;
        self.resolve.check_attempts()?;
        if self.issue.initial_delay < IssueConfig::MIN_INITIAL_DELAY {
            return Err(WorkerConfigError::IssueDelayBelowFloor {
                initial: self.issue.initial_delay,
                floor: IssueConfig::MIN_INITIAL_DELAY,
            });
        }
        self.issue.check_delays()?;
        self.read.check_delays()?;
        self.resolve.check_delays()?;
        Ok(())
    }
}

impl TryFrom<WorkerConfig> for ValidatedWorkerConfig {
    type Error = WorkerConfigError;

    fn try_from(config: WorkerConfig) -> Result<Self, Self::Error> {
        config.validate()
    }
}

/// A [`WorkerConfig`] whose invariants [`WorkerConfig::validate`] has
/// checked: what [`Order::from_parts`](crate::Order) and
/// [`Agent::from_parts`](crate::Agent) take, so that the services cannot be
/// built over an issue policy below its floor. Dereferences to the
/// [`WorkerConfig`] it wraps.
///
/// There is no other constructor: a policy the floor refuses (the e2e suite's
/// one-second issue delay against a mock that answers at once) is built with
/// `ValidatedWorkerConfig::unchecked`, behind the `test-util` feature, which
/// a deployment never enables (and which this documentation is built without,
/// so the method is not linked here).
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedWorkerConfig(WorkerConfig);

impl ValidatedWorkerConfig {
    /// Wraps `config` **without** checking its invariants: for test harnesses
    /// whose policies are sized for a mock rather than for szamlazz.hu.
    ///
    /// Behind the `test-util` feature so that a deployment cannot reach it:
    /// an issue `initial_delay` below [`IssueConfig::MIN_INITIAL_DELAY`]
    /// re-executes the create step while the cut execution's send may still
    /// be in flight.
    #[cfg(feature = "test-util")]
    #[cfg_attr(docsrs, doc(cfg(feature = "test-util")))]
    #[must_use]
    pub fn unchecked(config: WorkerConfig) -> Self {
        Self(config)
    }

    /// The settings.
    #[must_use]
    pub fn into_inner(self) -> WorkerConfig {
        self.0
    }
}

impl Deref for ValidatedWorkerConfig {
    type Target = WorkerConfig;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<WorkerConfig> for ValidatedWorkerConfig {
    fn as_ref(&self) -> &WorkerConfig {
        &self.0
    }
}

/// A [`WorkerConfig`] that parsed but violates an invariant.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum WorkerConfigError {
    /// A policy's `max_attempts` is zero.
    #[error("{table}.max_attempts must be at least 1")]
    ZeroMaxAttempts {
        /// The policy's table ([`table::Table::NAME`]).
        table: &'static str,
    },
    /// The issue policy's `initial_delay` is below
    /// [`IssueConfig::MIN_INITIAL_DELAY`], which documents the rule.
    #[error(
        "issue.initial_delay ({initial:?}) must be at least {floor:?}: the Számla Agent client's \
         {timeout:?} request timeout plus a {margin:?} margin: szamlazz.hu has been seen to stall \
         that long and still issue, so the create and storno steps are never re-executed while \
         their send may still be in flight",
        timeout = szamlazz_agent::client::REQUEST_TIMEOUT,
        margin = IssueConfig::RE_CHECK_MARGIN
    )]
    IssueDelayBelowFloor {
        /// The configured initial delay.
        initial: Duration,
        /// The floor, [`IssueConfig::MIN_INITIAL_DELAY`].
        floor: Duration,
    },
    /// A policy's `initial_delay` exceeds its `max_delay`.
    #[error("{table}.initial_delay ({initial:?}) must not exceed {table}.max_delay ({max:?})")]
    DelayOrder {
        /// The policy's table ([`table::Table::NAME`]).
        table: &'static str,
        /// The configured initial delay.
        initial: Duration,
        /// The configured maximum delay.
        max: Duration,
    },
    /// A policy's `factor` is below 1 (the delay would shrink), or not a
    /// finite number (`nan`, `inf`; TOML and YAML accept both as floats).
    #[error("{table}.factor ({factor}) must be a finite number of at least 1")]
    InvalidFactor {
        /// The policy's table ([`table::Table::NAME`]).
        table: &'static str,
        /// The configured factor.
        factor: f32,
    },
}

/// The three tables a [`RetryPolicyConfig`] is read from, as types: each
/// carries its name and its defaults, so one struct serves the three
/// policies with three sets of defaults, and the table types are the one
/// enumeration of the policies (#184: a `Policy` enum once doubled them).
pub mod table {
    use std::fmt;
    use std::time::Duration;

    use super::RetryPolicyConfig;

    /// A policy table of the [`WorkerConfig`](super::WorkerConfig): its name
    /// and its defaults. Sealed: the three tables are the deployment's.
    pub trait Table: sealed::Sealed + Sized + fmt::Debug + Clone + Copy + PartialEq + Eq {
        /// The table's key in the deployment configuration (`issue`, `read`,
        /// `resolve`), as a [`WorkerConfigError`](super::WorkerConfigError)
        /// names it.
        const NAME: &'static str;

        /// The table's defaults.
        fn defaults() -> RetryPolicyConfig<Self>;
    }

    mod sealed {
        pub trait Sealed {}
    }

    /// `[issue]`: the run retry policy of the create step and the storno
    /// step. Defaults: five executions, `2m → 10m` doubling, bounded at `1h`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum Issue {}

    /// `[read]`: the run retry policy of every read-only step. Defaults:
    /// five executions, `5s → 60s` doubling, bounded at `5m`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum Read {}

    /// `[resolve]`: the run retry policy of the `account` step. Defaults:
    /// no attempt cap, `1s → 10s` doubling, bounded at `1m`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum Resolve {}

    impl sealed::Sealed for Issue {}
    impl sealed::Sealed for Read {}
    impl sealed::Sealed for Resolve {}

    impl Table for Issue {
        const NAME: &'static str = "issue";

        fn defaults() -> RetryPolicyConfig<Self> {
            RetryPolicyConfig::new(
                Some(5),
                Duration::from_mins(2),
                2.0,
                Duration::from_mins(10),
                Duration::from_hours(1),
            )
        }
    }

    impl Table for Read {
        const NAME: &'static str = "read";

        fn defaults() -> RetryPolicyConfig<Self> {
            RetryPolicyConfig::new(
                Some(5),
                Duration::from_secs(5),
                2.0,
                Duration::from_mins(1),
                Duration::from_mins(5),
            )
        }
    }

    impl Table for Resolve {
        const NAME: &'static str = "resolve";

        fn defaults() -> RetryPolicyConfig<Self> {
            RetryPolicyConfig::new(
                None,
                Duration::from_secs(1),
                2.0,
                Duration::from_secs(10),
                Duration::from_mins(1),
            )
        }
    }
}

/// The issue policy: the run retry policy of the create step and the storno
/// step. Restate re-executes the step after `initial_delay`, multiplying the
/// delay by `factor` up to `max_delay`, until `max_attempts` executions or
/// `max_duration`; then the step fails and the handler reports
/// `outcome_unknown`. The policy shapes no journal entry.
///
/// `initial_delay` has a floor, [`MIN_INITIAL_DELAY`](Self::MIN_INITIAL_DELAY),
/// which [`WorkerConfig::validate`] enforces.
pub type IssueConfig = RetryPolicyConfig<table::Issue>;

/// The read policy: the run retry policy of every read-only durable step of
/// both services (the lookup step and the exclusivity, proforma-link and
/// `get` lookups, the verifies, the order-number hint, the storno lookup,
/// `Szamlazz.Agent.query` and the `check_account` probe). A read that
/// szamlazz.hu did not answer (a transport or parse failure, `szlahu_down`)
/// is the step's retryable error, re-executed after `initial_delay`, the
/// delay multiplied by `factor` up to `max_delay`, until `max_attempts`
/// executions or `max_duration`; then the step fails and the handler reports
/// `unavailable`. Every szamlazz.hu *answer* is data and never retried. The
/// policy shapes no journal entry.
///
/// A read may be retried freely: it writes nothing, and a re-executed
/// closure's answer is exactly as fresh as a first answer. The defaults are
/// sized for szamlazz.hu, not for the worker: five executions 5 → 10 → 20 →
/// 40 s apart ride out a blip of about a minute, and, since szamlazz.hu is
/// observed to stall for a minute at a time, a stalling szamlazz.hu is waited
/// out up to the 5 m bound, instead of failing the invocation with a terminal
/// `unavailable` that is stored under the caller's `Idempotency-Key` for the
/// retention period. This policy, not the handlers' invocation retry policy,
/// is what decides how long a szamlazz.hu outage is tolerated: a run retry is
/// re-dispatched by the server without spending an invocation attempt. A
/// worker outage is the invocation retry policy's business.
pub type ReadConfig = RetryPolicyConfig<table::Read>;

/// The resolve policy: the run retry policy of the `account` step of every
/// handler, which asks the account resolver for the request's account. An
/// unavailable resolver is retried under it (`initial_delay` growing by
/// `factor` to `max_delay`, bounded by `max_duration`; no attempt cap by
/// default), and its exhaustion is the `unavailable` fault. Unscoped and
/// unknown are answers, journaled as data, never retried. Shapes no journal
/// entry.
///
/// Set explicitly for the same reason as the issue policy: the SDK's default
/// run policy sends no retry delay and the server would spend the handler's
/// `invocation_retry_policy` instead.
pub type ResolveConfig = RetryPolicyConfig<table::Resolve>;

/// A run retry policy as one table of the [`WorkerConfig`] configures it:
/// the [`IssueConfig`], [`ReadConfig`] and [`ResolveConfig`] are this one
/// struct with the table's defaults ([`Table::defaults`]). Restate
/// re-executes the step after `initial_delay`, multiplying the delay by
/// `factor` up to `max_delay`, until `max_attempts` executions (when set) or
/// `max_duration`, whichever comes first.
///
/// Durations are written in the grammar Restate's own handler attributes
/// take (jiff's friendly format: `"90s"`, `"2m"`, `"1h 30m"`, `"3d"`,
/// `"500ms"`) or as a bare non-negative integer read as seconds (`90`).
/// Closed: an unknown key is a parse error. `#[non_exhaustive]`: deserialize
/// it, or start from [`Default::default`] (the table's defaults) and set
/// fields.
///
/// The field names are the SDK's [`RunRetryPolicy`] setters' (`initial_delay`,
/// `max_delay`, `max_attempts`, `max_duration`), which this maps onto field
/// for field, with the one shortening the SDK's handler attribute also makes
/// (`factor` for `exponentiation_factor`). They are **not** the handler
/// attribute's names (`initial_interval`, `max_interval`): those configure
/// the *invocation* retry policy, which the handlers pin in code; these
/// configure a *run* retry policy, and an operator writing `initial_interval`
/// here is refused by name (the table is closed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct RetryPolicyConfig<T: Table> {
    /// Executions of the step, including the first; `None` leaves the
    /// duration as the sole bound. Default `5` on `[issue]` and `[read]`,
    /// unset on `[resolve]`.
    pub max_attempts: Option<u32>,
    /// Delay before the first re-execution. Default `2m` on `[issue]` (at
    /// least [`IssueConfig::MIN_INITIAL_DELAY`]), `5s` on `[read]`, `1s` on
    /// `[resolve]`.
    #[serde(with = "duration_str")]
    pub initial_delay: Duration,
    /// Multiplier of the delay after each re-execution. Default `2.0`.
    pub factor: f32,
    /// Cap of the delay. Default `10m` on `[issue]`, `60s` on `[read]`, `10s`
    /// on `[resolve]`.
    #[serde(with = "duration_str")]
    pub max_delay: Duration,
    /// Hard bound on the time spent re-executing the step. Default `1h` on
    /// `[issue]`, `5m` on `[read]`, `1m` on `[resolve]`.
    #[serde(with = "duration_str")]
    pub max_duration: Duration,
    #[serde(skip)]
    table: PhantomData<T>,
}

impl<T: Table> Default for RetryPolicyConfig<T> {
    /// The table's defaults ([`Table::defaults`]).
    fn default() -> Self {
        T::defaults()
    }
}

impl<T: Table> RetryPolicyConfig<T> {
    /// A policy of every field.
    const fn new(
        max_attempts: Option<u32>,
        initial_delay: Duration,
        factor: f32,
        max_delay: Duration,
        max_duration: Duration,
    ) -> Self {
        Self {
            max_attempts,
            initial_delay,
            factor,
            max_delay,
            max_duration,
            table: PhantomData,
        }
    }

    /// The policy as the SDK's run retry policy, every field set from this
    /// configuration. Built on [`RunRetryPolicy::new`], whose factor is 1.0
    /// and which caps nothing, not on `default()`, which caps the delay at
    /// 2 s and the duration at 50 s. An unset `max_attempts` sets no attempt
    /// cap: the duration is the bound.
    #[must_use]
    pub fn run_retry_policy(&self) -> RunRetryPolicy {
        let policy = RunRetryPolicy::new()
            .initial_delay(self.initial_delay)
            .exponentiation_factor(self.factor)
            .max_delay(self.max_delay)
            .max_duration(self.max_duration);
        match self.max_attempts {
            Some(max_attempts) => policy.max_attempts(max_attempts),
            None => policy,
        }
    }

    /// An attempt cap, when set, is at least one execution.
    fn check_attempts(&self) -> Result<(), WorkerConfigError> {
        if self.max_attempts == Some(0) {
            return Err(WorkerConfigError::ZeroMaxAttempts { table: T::NAME });
        }
        Ok(())
    }

    /// The delays are ordered and the factor does not shrink them.
    fn check_delays(&self) -> Result<(), WorkerConfigError> {
        if self.initial_delay > self.max_delay {
            return Err(WorkerConfigError::DelayOrder {
                table: T::NAME,
                initial: self.initial_delay,
                max: self.max_delay,
            });
        }
        // The factor is finite and at least 1: `inf` is a float to TOML and
        // YAML and would otherwise clear the `< 1.0` check.
        if !self.factor.is_finite() || self.factor < 1.0 {
            return Err(WorkerConfigError::InvalidFactor {
                table: T::NAME,
                factor: self.factor,
            });
        }
        Ok(())
    }
}

impl IssueConfig {
    /// The margin [`MIN_INITIAL_DELAY`](Self::MIN_INITIAL_DELAY) keeps beyond
    /// the client timeout.
    pub const RE_CHECK_MARGIN: Duration = Duration::from_secs(30);

    /// The least `initial_delay` a deployment may run with: the Számla Agent
    /// client's [`REQUEST_TIMEOUT`](szamlazz_agent::client::REQUEST_TIMEOUT)
    /// plus [`RE_CHECK_MARGIN`](Self::RE_CHECK_MARGIN), 90 s at today's
    /// values, derived rather than copied so that a change to the timeout
    /// moves the floor with it.
    ///
    /// Every re-execution of the create or storno step begins with a query
    /// for what the cut execution sent, and that query is conclusive only
    /// once the send can no longer be in flight: the client gives up on a
    /// reply at the timeout, but szamlazz.hu has been seen to stall that long
    /// and still issue. The same rule sizes every write handler's
    /// `initial_interval` (`2m`). The read and resolve policies have no floor:
    /// a read writes nothing, and the resolve policy never reaches
    /// szamlazz.hu.
    ///
    /// The derivation holds because the gateway opens its client with the
    /// default timeout: [`Gateway::open`](crate::gateway::Gateway::open) never
    /// supplies its own `reqwest::Client`, on which the timeout would be the
    /// caller's ([`Gateway::open_with_http`](crate::gateway::Gateway::open_with_http)
    /// says so).
    pub const MIN_INITIAL_DELAY: Duration =
        szamlazz_agent::client::REQUEST_TIMEOUT.saturating_add(Self::RE_CHECK_MARGIN);
}

/// Parses a duration written in the grammar Restate's own `#[handler(...)]`
/// attributes take, jiff's "friendly" format (`"90s"`, `"2m"`, `"1h"`,
/// `"3d"`, `"500ms"`, `"1h 30m"`, `"2 days"`), or as a plain number of
/// seconds (`"90"`). One grammar for the deployment configuration and the
/// handler attributes, so a value copied from one to the other parses; the
/// parser mirrors the SDK macro's (`restate-sdk-macros`, `parse_duration_lit`):
/// a [`jiff::Span`] totalled in milliseconds with days as 24 hours and weeks as
/// 7 days, months and years refused (they have no fixed length without a
/// reference date). The string form of what a duration field of the policies
/// takes; a field also takes the number as a bare integer. The grammar is
/// part of the configuration contract; the parser is not part of the API.
///
/// # Errors
///
/// Returns an error for an empty string, a string jiff does not read as a
/// span (an unknown unit, a fraction on a non-terminal unit, months or
/// years), a negative span or one that overflows.
pub(crate) fn parse_duration(value: &str) -> Result<Duration, InvalidDuration> {
    let value = value.trim();
    if value.is_empty() {
        return Err(InvalidDuration::Empty);
    }
    if value.bytes().all(|byte| byte.is_ascii_digit()) {
        return value
            .parse::<u64>()
            .map(Duration::from_secs)
            .map_err(|_| InvalidDuration::Overflow);
    }
    let span: jiff::Span = value
        .parse()
        .map_err(|error: jiff::Error| InvalidDuration::Invalid(error.to_string()))?;
    let millis = span
        .total((
            jiff::Unit::Millisecond,
            jiff::SpanRelativeTo::days_are_24_hours(),
        ))
        .map_err(|error| InvalidDuration::Invalid(error.to_string()))?;
    if millis < 0.0 {
        return Err(InvalidDuration::Negative);
    }
    // A `u64` of milliseconds is 584 million years; anything a policy takes
    // is far inside, and `Duration::from_secs_f64` refuses what is not.
    Duration::try_from_secs_f64(millis / 1000.0).map_err(|_| InvalidDuration::Overflow)
}

/// Formats a duration in the largest unit that divides it evenly: `"1h"`,
/// `"2m"`, `"90s"`, `"1500ms"`. Every output parses back with
/// [`parse_duration`] to the same value (sub-millisecond precision is
/// dropped).
#[must_use]
pub(crate) fn format_duration(duration: Duration) -> String {
    let millis = duration.as_millis();
    if millis > 0 && !millis.is_multiple_of(1000) {
        return format!("{millis}ms");
    }
    let secs = duration.as_secs();
    if secs > 0 && secs.is_multiple_of(3600) {
        format!("{}h", secs / 3600)
    } else if secs > 0 && secs.is_multiple_of(60) {
        format!("{}m", secs / 60)
    } else {
        format!("{secs}s")
    }
}

/// A string that is not a valid duration; surfaces as serde's error message on
/// a duration field of the policies.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum InvalidDuration {
    /// The string is empty.
    #[error("duration must not be empty")]
    Empty,
    /// jiff does not read the string as a span of fixed length: an unknown
    /// unit, months or years, a malformed amount. Carries jiff's message.
    #[error(
        "invalid duration: {0}; write it as Restate does, e.g. \"90s\", \"2m\", \"1h 30m\", \"3d\""
    )]
    Invalid(String),
    /// The span is negative.
    #[error("duration must not be negative")]
    Negative,
    /// The amount does not fit.
    #[error("duration is too large")]
    Overflow,
}

/// `#[serde(with)]` helper for durations: serialized in the `"2m"` string
/// form, deserialized from that form or from a bare non-negative integer read
/// as seconds.
mod duration_str {
    use std::fmt;
    use std::time::Duration;

    use serde::de::{Unexpected, Visitor};
    use serde::{Deserializer, Serializer};

    /// What a duration field expects, as serde's error message names it.
    const EXPECTED: &str = "a duration such as \"90s\", \"2m\", \"1h 30m\", \"3d\" or a non-negative number of seconds";

    pub(super) fn serialize<S: Serializer>(
        duration: &Duration,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&super::format_duration(*duration))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Duration, D::Error> {
        struct StringOrSeconds;

        impl Visitor<'_> for StringOrSeconds {
            type Value = Duration;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(EXPECTED)
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                super::parse_duration(value).map_err(E::custom)
            }

            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(Duration::from_secs(value))
            }

            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
                u64::try_from(value)
                    .map(Duration::from_secs)
                    .map_err(|_| E::invalid_value(Unexpected::Signed(value), &self))
            }
        }

        deserializer.deserialize_any(StringOrSeconds)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The `validate` verdict of a parsed configuration, for the tables that
    /// assert on the error alone.
    fn verdict(value: serde_json::Value) -> Result<(), WorkerConfigError> {
        serde_json::from_value::<WorkerConfig>(value)
            .expect("parse")
            .validate()
            .map(|_| ())
    }

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
        assert_eq!(config.issue.max_attempts, Some(3));
        assert_eq!(config.issue.initial_delay, Duration::from_secs(90));
        assert_eq!(config.issue.factor.to_bits(), 1.5f32.to_bits());
        assert_eq!(config.issue.max_delay, Duration::from_secs(3600));
        assert_eq!(config.issue.max_duration, Duration::from_secs(7200));
        assert_eq!(config.resolve.max_attempts, None);
        assert_eq!(config.resolve.initial_delay, Duration::from_secs(2));
        assert_eq!(config.resolve.factor.to_bits(), 3.0f32.to_bits());
        assert_eq!(config.resolve.max_delay, Duration::from_secs(20));
        assert_eq!(config.resolve.max_duration, Duration::from_secs(300));
        let validated = config.clone().validate().expect("valid");
        assert_eq!(
            *validated, config,
            "the validated settings deref to the parsed ones"
        );
        assert_eq!(validated.as_ref(), &config);
        assert_eq!(validated.clone().into_inner(), config);
        assert_eq!(
            ValidatedWorkerConfig::try_from(config.clone()).expect("valid"),
            validated
        );
    }

    /// Only the namespace is required; the three policies default, and the
    /// parsed minimum equals [`WorkerConfig::new`].
    #[test]
    fn minimal_worker_config_is_the_namespace_with_default_policies() {
        let config: WorkerConfig =
            serde_json::from_value(json!({ "namespace": "acct" })).expect("parse");

        assert_eq!(
            config,
            WorkerConfig::new("acct".parse().expect("namespace"))
        );
        assert_eq!(config.issue, IssueConfig::default());
        assert_eq!(config.issue.max_attempts, Some(5));
        assert_eq!(config.issue.initial_delay, Duration::from_secs(120));
        assert_eq!(config.issue.factor.to_bits(), 2.0f32.to_bits());
        assert_eq!(config.issue.max_delay, Duration::from_secs(600));
        assert_eq!(config.issue.max_duration, Duration::from_secs(3600));
        assert_eq!(config.read, ReadConfig::default());
        assert_eq!(config.read.max_attempts, Some(5));
        assert_eq!(config.read.initial_delay, Duration::from_secs(5));
        assert_eq!(config.read.factor.to_bits(), 2.0f32.to_bits());
        assert_eq!(config.read.max_delay, Duration::from_secs(60));
        assert_eq!(config.read.max_duration, Duration::from_secs(300));
        assert_eq!(config.resolve, ResolveConfig::default());
        assert_eq!(config.resolve.max_attempts, None);
        assert_eq!(config.resolve.initial_delay, Duration::from_secs(1));
        assert_eq!(config.resolve.max_delay, Duration::from_secs(10));
        assert_eq!(config.resolve.max_duration, Duration::from_secs(60));
        config.validate().expect("valid");

        assert!(
            serde_json::from_value::<WorkerConfig>(json!({})).is_err(),
            "the namespace has no default"
        );

        // The delays between the default read executions (initial × factor^n,
        // each under the cap) sum to 75 s: a read rides out a szamlazz.hu
        // blip of about a minute instead of poisoning the caller's key with a
        // 503, and the bound is what a stalling szamlazz.hu runs into (#87).
        let read = ReadConfig::default();
        let attempts = read.max_attempts.expect("the read policy caps attempts");
        let back_off: Duration = (0..attempts - 1)
            .map(|n| {
                read.initial_delay
                    .mul_f32(read.factor.powi(n.cast_signed()))
            })
            .map(|delay| delay.min(read.max_delay))
            .sum();
        assert_eq!(back_off, Duration::from_secs(75));
        assert!(back_off < read.max_duration, "the back-off fits the bound");
    }

    /// The namespace is validated where it is parsed (`identity`); the
    /// configuration's `namespace` key takes the validated type, so a value
    /// outside its alphabet fails the parse rather than the first request.
    #[test]
    fn namespace_key_is_validated_at_parse_time() {
        for accepted in ["a", "acct", "acct-1", "0", "a".repeat(16).as_str()] {
            let config: WorkerConfig =
                serde_json::from_value(json!({ "namespace": accepted })).expect(accepted);
            assert_eq!(config.namespace.as_str(), accepted);
        }
        for rejected in [
            "",
            "Acct",
            "acct_1",
            "acct 1",
            "acct:1",
            "ácct",
            &"a".repeat(17),
        ] {
            let result = serde_json::from_value::<WorkerConfig>(json!({ "namespace": rejected }));
            assert!(result.is_err(), "{rejected:?} should be rejected");
        }
    }

    /// The configuration is closed at every level: an unknown top-level key,
    /// an unknown key in any of the three tables, is a parse error naming the
    /// key and the keys expected in its place, never a policy left at its
    /// default. (A misspelt `[isue]` table would otherwise leave the issue
    /// policy at its default.)
    #[test]
    fn unknown_keys_are_refused_at_every_level() {
        let cases = [
            (json!({"namespace": "acct", "isue": {}}), "isue", "issue"),
            (
                json!({"namespace": "acct", "issue": {"max_atempts": 1}}),
                "max_atempts",
                "max_attempts",
            ),
            (
                json!({"namespace": "acct", "read": {"initial": "1s"}}),
                "initial",
                "initial_delay",
            ),
            (
                json!({"namespace": "acct", "resolve": {"attempts": 1}}),
                "attempts",
                "max_duration",
            ),
        ];
        for (value, unknown, expected) in cases {
            let error = serde_json::from_value::<WorkerConfig>(value)
                .expect_err(unknown)
                .to_string();
            assert!(
                error.contains(&format!("unknown field `{unknown}`")),
                "{unknown}: names the key: {error}"
            );
            assert!(
                error.contains(&format!("`{expected}`")),
                "{unknown}: lists what is expected: {error}"
            );
        }
        // `max_attempts` is a key of every table: the resolve policy takes an
        // attempt cap too when one is written (the duration stays the bound
        // when none is).
        let capped: WorkerConfig =
            serde_json::from_value(json!({"namespace": "acct", "resolve": {"max_attempts": 3}}))
                .expect("parse");
        assert_eq!(capped.resolve.max_attempts, Some(3));
    }

    /// The resolve policy is the run retry policy of the `account` step:
    /// delays and the duration bound, no attempt cap unless one is written.
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
        assert_eq!(parsed.max_attempts, None, "default kept");
    }

    /// The issue policy is the run retry policy of the create and storno
    /// steps, every field set: `RunRetryPolicy::new()` has factor 1.0 and no
    /// caps, and `default()` caps at 2 s / 50 s; neither is what the policy
    /// says.
    #[test]
    fn issue_policy_maps_to_the_run_retry_policy_field_for_field() {
        assert_eq!(
            format!("{:?}", IssueConfig::default().run_retry_policy()),
            "RunRetryPolicy { initial_delay: 120s, factor: 2.0, max_delay: Some(600s), \
             max_attempts: Some(5), max_duration: Some(3600s) }"
        );
        let short = IssueConfig {
            max_attempts: Some(2),
            initial_delay: Duration::from_secs(1),
            factor: 1.5,
            max_delay: Duration::from_secs(2),
            max_duration: Duration::from_secs(30),
            ..IssueConfig::default()
        };
        assert_eq!(
            format!("{:?}", short.run_retry_policy()),
            "RunRetryPolicy { initial_delay: 1s, factor: 1.5, max_delay: Some(2s), \
             max_attempts: Some(2), max_duration: Some(30s) }"
        );
    }

    /// The read policy is the run retry policy of every read-only step
    /// (the lookups, verifies, hints, `get`'s queries, `Szamlazz.Agent.query`
    /// and the probe), every field set, like the issue policy.
    #[test]
    fn read_policy_maps_to_the_run_retry_policy_field_for_field() {
        assert_eq!(
            format!("{:?}", ReadConfig::default().run_retry_policy()),
            "RunRetryPolicy { initial_delay: 5s, factor: 2.0, max_delay: Some(60s), \
             max_attempts: Some(5), max_duration: Some(300s) }"
        );
        let short = ReadConfig {
            max_attempts: Some(2),
            initial_delay: Duration::from_secs(1),
            factor: 1.0,
            max_delay: Duration::from_secs(1),
            max_duration: Duration::from_secs(10),
            ..ReadConfig::default()
        };
        assert_eq!(
            format!("{:?}", short.run_retry_policy()),
            "RunRetryPolicy { initial_delay: 1s, factor: 1.0, max_delay: Some(1s), \
             max_attempts: Some(2), max_duration: Some(10s) }"
        );
    }

    /// `[read]` parses beside `[issue]` and `[resolve]` and defaults when
    /// absent.
    #[test]
    fn read_policy_parses_and_defaults() {
        let config: WorkerConfig = serde_json::from_value(json!({
            "namespace": "acct",
            "read": {
                "max_attempts": 4,
                "initial_delay": "2s",
                "factor": 1.5,
                "max_delay": "20s",
                "max_duration": "3m",
            },
        }))
        .expect("parse");
        assert_eq!(config.read.max_attempts, Some(4));
        assert_eq!(config.read.initial_delay, Duration::from_secs(2));
        assert_eq!(config.read.factor.to_bits(), 1.5f32.to_bits());
        assert_eq!(config.read.max_delay, Duration::from_secs(20));
        assert_eq!(config.read.max_duration, Duration::from_secs(180));
        config.validate().expect("valid");
    }

    /// Every table is held to the same invariants, and the error names its
    /// table.
    #[test]
    fn validate_reports_the_invariants_of_every_table() {
        for (table, max_delay) in [("issue", 600), ("read", 60), ("resolve", 10)] {
            let config = |policy: serde_json::Value| json!({ "namespace": "acct", table: policy });
            assert_eq!(
                verdict(config(json!({"max_attempts": 0}))),
                Err(WorkerConfigError::ZeroMaxAttempts { table }),
                "{table}"
            );
            assert_eq!(
                verdict(config(json!({"max_attempts": 0})))
                    .expect_err("error")
                    .to_string(),
                format!("{table}.max_attempts must be at least 1")
            );
            assert_eq!(
                verdict(config(json!({"max_attempts": 1}))),
                Ok(()),
                "{table}"
            );
            let over = Duration::from_secs(max_delay + 1);
            let initial = json!(max_delay + 1);
            assert_eq!(
                verdict(config(json!({"initial_delay": initial}))),
                Err(WorkerConfigError::DelayOrder {
                    table,
                    initial: over,
                    max: Duration::from_secs(max_delay),
                }),
                "{table}"
            );
            assert_eq!(
                verdict(config(json!({"initial_delay": initial})))
                    .expect_err("error")
                    .to_string(),
                format!(
                    "{table}.initial_delay ({over:?}) must not exceed {table}.max_delay ({:?})",
                    Duration::from_secs(max_delay)
                ),
                "the error names the table"
            );
            assert_eq!(
                verdict(config(json!({"initial_delay": max_delay}))),
                Ok(()),
                "{table}: equal delays are in order"
            );
            assert_eq!(
                verdict(config(json!({"factor": 0.5}))),
                Err(WorkerConfigError::InvalidFactor { table, factor: 0.5 }),
                "{table}"
            );
            assert_eq!(verdict(config(json!({"factor": 1.0}))), Ok(()), "{table}");
            assert_eq!(
                verdict(config(json!({"factor": 0.5})))
                    .expect_err("error")
                    .to_string(),
                format!("{table}.factor (0.5) must be a finite number of at least 1")
            );
        }
    }

    /// `inf` is a float to TOML (`factor = inf`) and is at least 1, so the
    /// order check alone would pass it on to Restate as a non-finite
    /// exponentiation factor; it is refused as not finite, like `nan`. JSON
    /// cannot write either, so the check is exercised on the struct.
    #[test]
    fn a_non_finite_factor_is_refused_on_every_table() {
        for non_finite in [f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
            let config = WorkerConfig {
                issue: IssueConfig {
                    factor: non_finite,
                    ..IssueConfig::default()
                },
                read: ReadConfig {
                    factor: non_finite,
                    ..ReadConfig::default()
                },
                resolve: ResolveConfig {
                    factor: non_finite,
                    ..ResolveConfig::default()
                },
                ..WorkerConfig::new("acct".parse().expect("namespace"))
            };
            let error = config
                .validate()
                .expect_err("a non-finite factor is refused");
            assert!(
                matches!(
                    error,
                    WorkerConfigError::InvalidFactor {
                        table: "issue",
                        factor
                    } if factor.is_nan() == non_finite.is_nan()
                ),
                "{non_finite}: {error:?}"
            );
            assert!(
                error
                    .to_string()
                    .contains("must be a finite number of at least 1"),
                "{error}"
            );
        }
        // The check is per table: a finite issue factor and an infinite read
        // one is the read table's error.
        let config = WorkerConfig {
            read: ReadConfig {
                factor: f32::INFINITY,
                ..ReadConfig::default()
            },
            ..WorkerConfig::new("acct".parse().expect("namespace"))
        };
        assert!(matches!(
            config.validate(),
            Err(WorkerConfigError::InvalidFactor { table: "read", .. })
        ));
    }

    /// `issue.initial_delay` is floored at [`IssueConfig::MIN_INITIAL_DELAY`],
    /// the Számla Agent client's timeout plus a margin, derived, not copied.
    /// The other two policies have no floor.
    #[test]
    fn validate_floors_the_issue_initial_delay_at_the_client_timeout_plus_a_margin() {
        let issue = |issue: serde_json::Value| json!({ "namespace": "acct", "issue": issue });

        assert_eq!(
            IssueConfig::MIN_INITIAL_DELAY,
            szamlazz_agent::client::REQUEST_TIMEOUT + IssueConfig::RE_CHECK_MARGIN,
            "the floor is derived from the client's timeout"
        );
        assert_eq!(IssueConfig::MIN_INITIAL_DELAY, Duration::from_secs(90));
        assert!(
            IssueConfig::default().initial_delay >= IssueConfig::MIN_INITIAL_DELAY,
            "the default clears its own floor"
        );

        // The boundary: 90 s is accepted, 89 s is not.
        assert_eq!(verdict(issue(json!({"initial_delay": "90s"}))), Ok(()));
        assert_eq!(
            verdict(issue(json!({"initial_delay": "89s"}))),
            Err(WorkerConfigError::IssueDelayBelowFloor {
                initial: Duration::from_secs(89),
                floor: Duration::from_secs(90),
            })
        );
        assert_eq!(
            verdict(issue(json!({"initial_delay": "5s"})))
                .expect_err("error")
                .to_string(),
            "issue.initial_delay (5s) must be at least 90s: the Számla Agent client's 60s request \
             timeout plus a 30s margin: szamlazz.hu has been seen to stall that long and still \
             issue, so the create and storno steps are never re-executed while their send may \
             still be in flight",
            "the error names the rule"
        );

        // The floor is checked before the order of the delays: it is the
        // safety rule, the order a consistency rule.
        assert_eq!(
            verdict(issue(json!({"initial_delay": "5s", "max_delay": "4s"}))),
            Err(WorkerConfigError::IssueDelayBelowFloor {
                initial: Duration::from_secs(5),
                floor: Duration::from_secs(90),
            })
        );

        // No floor on the read or resolve policy: a 1 s delay is fine (the
        // e2e suite runs on exactly that).
        assert_eq!(
            verdict(json!({
                "namespace": "acct",
                "read": { "initial_delay": "1s", "max_delay": "1s" },
                "resolve": { "initial_delay": "1s", "max_delay": "1s" },
            })),
            Ok(())
        );
    }

    /// The one way past the floor is `unchecked`, behind `test-util`: what
    /// the e2e harness builds its one-second issue policy with. It checks
    /// nothing.
    #[cfg(feature = "test-util")]
    #[test]
    fn unchecked_skips_validation() {
        let below: WorkerConfig = serde_json::from_value(json!({
            "namespace": "acct",
            "issue": { "initial_delay": "1s", "max_delay": "2s" },
        }))
        .expect("parse");
        assert!(below.clone().validate().is_err());
        let unchecked = ValidatedWorkerConfig::unchecked(below.clone());
        assert_eq!(*unchecked, below);
    }

    /// The grammar is Restate's: what `#[handler(initial_interval = "2m")]`
    /// accepts, `[issue] initial_delay = "2m"` accepts, `"3d"` and
    /// `"1h 30m"` included; a bare number of seconds besides.
    #[test]
    fn duration_parsing_table() {
        let cases = [
            ("2m", 120_000),
            ("90s", 90_000),
            ("10m", 600_000),
            ("1h", 3_600_000),
            ("45", 45_000),
            ("0s", 0),
            (" 3m ", 180_000),
            ("3d", 3 * 24 * 3_600_000),
            ("1h 30m", 5_400_000),
            ("1h30m", 5_400_000),
            ("500ms", 500),
            ("2 days", 2 * 24 * 3_600_000),
            ("1 week", 7 * 24 * 3_600_000),
            ("1.5m", 90_000),
        ];
        for (input, millis) in cases {
            assert_eq!(
                parse_duration(input),
                Ok(Duration::from_millis(millis)),
                "{input:?}"
            );
        }
        assert_eq!(parse_duration(""), Err(InvalidDuration::Empty));
        assert!(
            matches!(parse_duration("2x"), Err(InvalidDuration::Invalid(_))),
            "an unknown unit"
        );
        assert!(
            matches!(parse_duration("m"), Err(InvalidDuration::Invalid(_))),
            "no amount"
        );
        assert!(
            matches!(parse_duration("1 month"), Err(InvalidDuration::Invalid(_))),
            "months have no fixed length"
        );
        assert_eq!(parse_duration("-1s"), Err(InvalidDuration::Negative));
        assert_eq!(
            parse_duration(&format!("{}", u64::MAX).repeat(2)),
            Err(InvalidDuration::Overflow)
        );
        let message = InvalidDuration::Invalid("x".to_owned()).to_string();
        assert!(message.contains("\"3d\""), "names the grammar: {message}");
    }

    #[test]
    fn duration_formats_in_largest_even_unit() {
        assert_eq!(format_duration(Duration::from_secs(3600)), "1h");
        assert_eq!(format_duration(Duration::from_secs(7200)), "2h");
        assert_eq!(format_duration(Duration::from_secs(120)), "2m");
        assert_eq!(format_duration(Duration::from_secs(90)), "90s");
        assert_eq!(format_duration(Duration::from_secs(0)), "0s");
        assert_eq!(format_duration(Duration::from_millis(1500)), "1500ms");
        for millis in [0, 1, 999, 1500, 90_000, 3_600_000, 3 * 24 * 3_600_000] {
            let duration = Duration::from_millis(millis);
            assert_eq!(
                parse_duration(&format_duration(duration)),
                Ok(duration),
                "round trip of {millis} ms"
            );
        }
    }

    /// A policy serialises to its configuration shape: durations as strings,
    /// an unset attempt cap as `null`, and the table marker nowhere.
    #[test]
    fn policies_round_trip_through_their_configuration_shape() {
        let json = serde_json::to_value(IssueConfig::default()).expect("serialize");
        assert_eq!(json["initial_delay"], "2m");
        assert_eq!(json["max_delay"], "10m");
        assert_eq!(json["max_duration"], "1h");
        assert_eq!(json["max_attempts"], 5);
        assert_eq!(
            json.as_object().expect("object").keys().collect::<Vec<_>>(),
            [
                "factor",
                "initial_delay",
                "max_attempts",
                "max_delay",
                "max_duration"
            ]
        );
        let back: IssueConfig = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, IssueConfig::default());

        let json = serde_json::to_value(ResolveConfig::default()).expect("serialize");
        assert_eq!(json["max_attempts"], serde_json::Value::Null);
        let back: ResolveConfig = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, ResolveConfig::default());
    }

    /// A duration field takes the `"2m"` string form or a bare non-negative
    /// integer, read as seconds, on every policy, as the doc comments say.
    /// A float, a negative number or another type is refused.
    #[test]
    fn duration_fields_accept_a_bare_integer_as_seconds() {
        let issue: IssueConfig =
            serde_json::from_value(json!({"initial_delay": 120, "max_delay": "10m"}))
                .expect("integer seconds");
        assert_eq!(issue.initial_delay, Duration::from_secs(120));
        assert_eq!(issue.max_delay, Duration::from_secs(600));

        let read: ReadConfig =
            serde_json::from_value(json!({"max_duration": 0})).expect("zero seconds");
        assert_eq!(read.max_duration, Duration::ZERO);

        let resolve: ResolveConfig =
            serde_json::from_value(json!({"initial_delay": 2})).expect("integer seconds");
        assert_eq!(resolve.initial_delay, Duration::from_secs(2));

        for rejected in [
            json!(1.5),
            json!(-1),
            json!(true),
            json!([1]),
            json!({"s": 1}),
        ] {
            let error = serde_json::from_value::<IssueConfig>(json!({"initial_delay": rejected}))
                .expect_err(&format!("{rejected} is not a duration"));
            assert!(
                error
                    .to_string()
                    .contains("expected a duration such as \"90s\", \"2m\", \"1h 30m\", \"3d\""),
                "the error says what a duration is: {rejected}: {error}"
            );
        }
    }
}
