//! The configuration sources as this binary reads them: a file whose key
//! paths render without figment's profile, and environment overrides whose
//! values are strings.

use figment::error::Error;
use figment::providers::Env;
use figment::value::{Dict, Map, Value};
use figment::{Metadata, Profile, Provider};

use super::ENV_PREFIX;

/// A file provider whose key paths render plainly in errors
/// (`issue.initial_delay`), without the `default.` profile prefix figment's
/// own interpolater adds; this binary has no profiles.
pub struct PlainKeys<P>(
    /// The file provider whose data and profile are passed through as is.
    pub P,
);

impl<P: Provider> Provider for PlainKeys<P> {
    fn metadata(&self) -> Metadata {
        self.0
            .metadata()
            .interpolater(|_: &Profile, keys: &[&str]| keys.join("."))
    }

    fn data(&self) -> Result<Map<Profile, Dict>, Error> {
        self.0.data()
    }

    fn profile(&self) -> Option<Profile> {
        self.0.profile()
    }
}

/// The `RESTATE_SZAMLAZZ_` environment overrides, `__` nesting, **every value
/// a string**.
///
/// Figment's own [`Env`] parses values (`007` becomes the number `7`, `true`
/// a boolean), which is wrong for a secret: an all-digit agent key must reach
/// szamlazz.hu exactly as written. Here a value stays the string the operator
/// set, and the field's type decides how it is read — the loader extracts
/// with [`Figment::extract_lossy`](figment::Figment::extract_lossy), which reads `"3"` as `3` and `"true"` as
/// `true` where a number or a boolean is expected and leaves a string field
/// alone. In errors a key renders as the variable that set it
/// (`RESTATE_SZAMLAZZ_ISSUE__MAX_ATTEMPTS`).
pub struct EnvOverrides {
    env: Env,
}

impl EnvOverrides {
    /// The overrides of the current process environment.
    #[must_use]
    pub fn new() -> Self {
        Self {
            env: Env::prefixed(ENV_PREFIX).split("__"),
        }
    }

    /// The variable that sets the key at `keys` (`account.agent_key` →
    /// `RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY`).
    fn variable(keys: &[&str]) -> String {
        let mut name = ENV_PREFIX.to_owned();
        for (i, key) in keys.iter().enumerate() {
            if i > 0 {
                name.push_str("__");
            }
            name.push_str(&key.to_ascii_uppercase());
        }
        name
    }
}

impl Default for EnvOverrides {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for EnvOverrides {
    fn metadata(&self) -> Metadata {
        Metadata::named("environment variables")
            .interpolater(|_: &Profile, keys: &[&str]| Self::variable(keys))
    }

    fn data(&self) -> Result<Map<Profile, Dict>, Error> {
        let mut dict = Dict::new();
        for (key, value) in self.env.iter() {
            insert(&mut dict, key.as_str(), Value::from(value));
        }
        Ok(Profile::Default.collect(dict))
    }
}

/// Inserts `value` under the dotted `key`, creating the tables on the way;
/// a later leaf under a key an earlier value occupied replaces it.
fn insert(dict: &mut Dict, key: &str, value: Value) {
    match key.split_once('.') {
        None => {
            dict.insert(key.to_owned(), value);
        }
        Some((head, rest)) => {
            let entry = dict
                .entry(head.to_owned())
                .or_insert_with(|| Value::from(Dict::new()));
            if entry.as_dict().is_none() {
                *entry = Value::from(Dict::new());
            }
            if let Value::Dict(_, inner) = entry {
                insert(inner, rest, value);
            }
        }
    }
}
