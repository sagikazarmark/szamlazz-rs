//! The one rule of the configuration's shape that the typed layout cannot
//! express: `[account]` and `[accounts.<scope>]` are mutually exclusive.
//!
//! Every other rule of the shape is serde's: the layout and every type it is
//! made of are closed (`#[serde(deny_unknown_fields)]`), so an unknown key at
//! any level is a parse error that figment attaches the key path and the
//! source to. The shape rule is the static resolver's, and it re-checks it;
//! it is checked here first, on the merged figment value, because the typed
//! layout would report the *symptom* instead: a stray
//! `RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY` on a multi-account file materialises
//! a partial `[account]`, and the layout would report that account's missing
//! `id`, not the rule, and not where each shape came from.

use std::fmt;

use figment::Figment;
use figment::value::Value;

/// Fails when `figment` holds both `[account]` and a non-empty `[accounts]`,
/// naming the source of each.
///
/// # Errors
///
/// The two shapes with the source of each; or the figment's own error when
/// its sources cannot be merged.
pub fn check(figment: &Figment) -> Result<(), Refused> {
    let merged: Value = figment
        .extract()
        .map_err(|error| Refused::Unmerged(Box::new(error)))?;
    let Some(dict) = merged.as_dict() else {
        return Ok(());
    };
    let account = dict
        .get("account")
        .filter(|value| !matches!(value, Value::Empty(..)));
    let accounts = dict
        .get("accounts")
        .filter(|value| value.as_dict().is_some_and(|scopes| !scopes.is_empty()));
    if let (Some(account), Some(accounts)) = (account, accounts) {
        return Err(Refused::BothShapes {
            account: origin(figment, account),
            accounts: origin(figment, accounts),
        });
    }
    Ok(())
}

/// Where `value` came from, as figment names it: `{source} {name}` (a file
/// path and its format) or the name alone (the environment).
fn origin(figment: &Figment, value: &Value) -> String {
    let Some(metadata) = figment.get_metadata(value.tag()) else {
        return "an unknown source".to_owned();
    };
    match &metadata.source {
        Some(source) => format!("{source} {}", metadata.name),
        None => metadata.name.to_string(),
    }
}

/// Why [`check`] refused the configuration.
#[derive(Debug)]
pub enum Refused {
    /// The figment's sources could not be merged (a file that does not
    /// parse); the configuration was not inspected. Boxed: figment's error
    /// is large next to the other variant.
    Unmerged(Box<figment::Error>),
    /// Both `[account]` and a non-empty `[accounts]` are present; each with
    /// where it came from.
    BothShapes {
        /// The source of `account`.
        account: String,
        /// The source of `accounts`.
        accounts: String,
    },
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unmerged(error) => error.fmt(f),
            Self::BothShapes { account, accounts } => write!(
                f,
                "[account] and [accounts.<scope>] are mutually exclusive; configure one shape: \
                 `account` comes from {account}, `accounts` from {accounts}"
            ),
        }
    }
}

impl std::error::Error for Refused {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Unmerged(error) => Some(error.as_ref()),
            Self::BothShapes { .. } => None,
        }
    }
}
