//! The keys the configuration knows, level by level, and the walk that
//! refuses every key it does not.
//!
//! The endpoint's own layout type could refuse unknown top-level keys with
//! `#[serde(deny_unknown_fields)]`, but the account-shaped value types
//! (`Defaults`, `SellerConfig`, `SellerEmailConfig`) are journaled inside
//! `Account` and must stay permissive for replay, so serde cannot be the
//! mechanism there. One walk over the merged figment value against this tree
//! covers every level the same way, and reports every unknown key at once —
//! with its path, where it came from and what is expected in its place.

use std::fmt;

use figment::Figment;
use figment::value::{Dict, Value};

/// One level of the configuration: the keys it knows, each with what sits
/// under it.
pub struct Table(&'static [(&'static str, Node)]);

impl Table {
    /// The keys of this level, in the order they are listed.
    pub fn keys(&self) -> impl Iterator<Item = &'static str> {
        self.0.iter().map(|(key, _)| *key)
    }

    fn get(&self, key: &str) -> Option<&Node> {
        self.0
            .iter()
            .find(|(known, _)| *known == key)
            .map(|(_, node)| node)
    }
}

/// What sits under a known key.
enum Node {
    /// A value; nothing below it is inspected.
    Value,
    /// A table with a fixed key set.
    Table(&'static Table),
    /// A table whose keys are the author's — the scopes of
    /// `[accounts.<scope>]` — each holding the same table.
    Keyed(&'static Table),
}

/// The top level: `namespace`, the three policies, one of the two account
/// shapes and `identity_keys`.
pub static TOP: Table = Table(&[
    ("namespace", Node::Value),
    ("issue", Node::Table(&CAPPED_POLICY)),
    ("read", Node::Table(&CAPPED_POLICY)),
    ("resolve", Node::Table(&RESOLVE_POLICY)),
    ("account", Node::Table(&ACCOUNT)),
    ("accounts", Node::Keyed(&ACCOUNT)),
    ("identity_keys", Node::Value),
]);

/// `[issue]` and `[read]`: a run retry policy capped by an attempt count as
/// well as by duration.
pub static CAPPED_POLICY: Table = Table(&[
    ("max_attempts", Node::Value),
    ("initial_delay", Node::Value),
    ("factor", Node::Value),
    ("max_delay", Node::Value),
    ("max_duration", Node::Value),
]);

/// `[resolve]`: a run retry policy bounded by duration alone.
pub static RESOLVE_POLICY: Table = Table(&[
    ("initial_delay", Node::Value),
    ("factor", Node::Value),
    ("max_delay", Node::Value),
    ("max_duration", Node::Value),
]);

/// `[account]` and each `[accounts.<scope>]`.
pub static ACCOUNT: Table = Table(&[
    ("id", Node::Value),
    ("agent_key", Node::Value),
    ("endpoint", Node::Value),
    ("defaults", Node::Table(&DEFAULTS)),
    ("seller", Node::Table(&SELLER)),
]);

/// `[account.defaults]`.
pub static DEFAULTS: Table = Table(&[
    ("e_invoice", Node::Value),
    ("language", Node::Value),
    ("currency", Node::Value),
    ("exchange_rate_bank", Node::Value),
    ("template", Node::Value),
    ("send_email", Node::Value),
    ("number_prefix", Node::Value),
    ("extra_logo", Node::Value),
    ("aggregator", Node::Value),
    ("guardian", Node::Value),
]);

/// `[account.seller]`.
pub static SELLER: Table = Table(&[
    ("bank", Node::Value),
    ("bank_account", Node::Value),
    ("signer_name", Node::Value),
    ("email", Node::Table(&SELLER_EMAIL)),
]);

/// `[account.seller.email]`.
pub static SELLER_EMAIL: Table = Table(&[
    ("reply_to", Node::Value),
    ("subject", Node::Value),
    ("body", Node::Value),
]);

/// The keys of the pre-release layout, each with where it went. The crate has
/// never been released, so there is no compatibility shim — only a clear
/// refusal: the namespace was `account.slug`, and the document defaults and
/// the seller block were top-level tables rather than part of `[account]`.
const MOVED: [(&str, &str); 3] = [
    (
        "account.slug",
        "`account.slug` is now the top-level `namespace`",
    ),
    ("defaults", "`[defaults]` is now `[account.defaults]`"),
    ("seller", "`[seller]` is now `[account.seller]`"),
];

/// Fails when `figment` holds a key the configuration does not know, at any
/// level — naming every such key — or both account shapes at once.
///
/// The shape rule is the static resolver's, and it re-checks it; it is
/// checked here first because the typed layout cannot express it: a stray
/// `RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY` on a multi-account file materialises
/// a partial `[account]`, and the layout would report that account's missing
/// `id` instead of the rule.
///
/// # Errors
///
/// Returns every unknown key with its path, its source and what is expected
/// at that level; the two shapes with the source of each; or the figment's
/// own error when its sources cannot be merged.
pub fn check(figment: &Figment) -> Result<(), Refused> {
    let merged: Value = figment
        .extract()
        .map_err(|error| Refused::Unmerged(Box::new(error)))?;
    let Some(dict) = merged.as_dict() else {
        return Ok(());
    };
    let mut found = Vec::new();
    walk(&TOP, dict, &mut Vec::new(), &mut found);
    if !found.is_empty() {
        return Err(Refused::UnknownKeys(
            found.into_iter().map(|key| key.describe(figment)).collect(),
        ));
    }
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

/// Collects every key of `dict` that `table` does not know into `found`,
/// descending into the tables it does.
fn walk(table: &'static Table, dict: &Dict, path: &mut Vec<String>, found: &mut Vec<Unknown>) {
    for (key, value) in dict {
        path.push(key.clone());
        match table.get(key) {
            None => found.push(Unknown {
                path: path.clone(),
                value: value.clone(),
                expected: table,
            }),
            Some(Node::Value) => {}
            Some(Node::Table(inner)) => {
                if let Some(dict) = value.as_dict() {
                    walk(inner, dict, path, found);
                }
            }
            Some(Node::Keyed(inner)) => {
                if let Some(scopes) = value.as_dict() {
                    for (scope, value) in scopes {
                        if let Some(dict) = value.as_dict() {
                            path.push(scope.clone());
                            walk(inner, dict, path, found);
                            path.pop();
                        }
                    }
                }
            }
        }
        path.pop();
    }
}

/// An unknown key as the walk found it: its path, the value under it (whose
/// tag says where it came from) and the level whose keys were expected.
struct Unknown {
    path: Vec<String>,
    value: Value,
    expected: &'static Table,
}

impl Unknown {
    /// Renders the key for the error: its path, how its source spells it when
    /// that differs (the environment variable that set it), where it came
    /// from, and the hint or the expected keys.
    fn describe(self, figment: &Figment) -> String {
        let path = self.path.join(".");
        let spelled = figment
            .get_metadata(self.value.tag())
            .map(|metadata| {
                leaves(&self.value, &self.path)
                    .into_iter()
                    .filter_map(|leaf| {
                        let spelled = metadata.interpolate(figment.profile(), &leaf);
                        (spelled != leaf.join(".")).then_some(spelled)
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|spelled| !spelled.is_empty())
            .map(|spelled| format!(" ({})", spelled.join(", ")))
            .unwrap_or_default();
        let origin = origin(figment, &self.value);
        let guidance = if let Some((_, hint)) = MOVED.iter().find(|(moved, _)| *moved == path) {
            format!("{hint} (pre-release layout)")
        } else {
            let keys: Vec<String> = self.expected.keys().map(|key| format!("`{key}`")).collect();
            format!("expected one of {}", keys.join(", "))
        };
        format!("unknown key `{path}`{spelled} in {origin}; {guidance}")
    }
}

/// The paths of the values under `value`: `path` itself for a value, every
/// leaf below it for a table — what an environment override spells.
fn leaves(value: &Value, path: &[String]) -> Vec<Vec<String>> {
    match value.as_dict() {
        Some(dict) if !dict.is_empty() => dict
            .iter()
            .flat_map(|(key, value)| {
                let mut path = path.to_vec();
                path.push(key.clone());
                leaves(value, &path)
            })
            .collect(),
        _ => vec![path.to_vec()],
    }
}

/// Why [`check`] refused the configuration.
#[derive(Debug)]
pub enum Refused {
    /// The figment's sources could not be merged (a file that does not
    /// parse); the configuration was not inspected. Boxed: figment's error
    /// is large next to the other variants.
    Unmerged(Box<figment::Error>),
    /// The unknown keys, each described.
    UnknownKeys(Vec<String>),
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
            Self::UnknownKeys(keys) if keys.len() == 1 => f.write_str(&keys[0]),
            Self::UnknownKeys(keys) => {
                write!(f, "{} unknown keys:", keys.len())?;
                for key in keys {
                    write!(f, "\n  {key}")?;
                }
                Ok(())
            }
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
            Self::UnknownKeys(_) | Self::BothShapes { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use restate_szamlazz::config::{
        Defaults, IssueConfig, ReadConfig, ResolveConfig, SellerConfig, SellerEmailConfig,
    };
    use serde::Serialize;

    use super::*;

    /// The keys a serialized default of `value` carries: the fields serde
    /// knows on the library type.
    fn fields<T: Serialize>(value: &T) -> BTreeSet<String> {
        serde_json::to_value(value)
            .expect("serializes")
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect()
    }

    fn known(table: &Table) -> BTreeSet<String> {
        table.keys().map(str::to_owned).collect()
    }

    /// The hand-maintained key tree must not drift from the library types it
    /// stands in for: every table whose type serializes is compared field
    /// for field. (`StaticAccount` and the top level have no `Serialize`;
    /// the loader's full-example test covers them in the direction that
    /// matters — a field the tree does not know fails to load.)
    #[test]
    fn known_keys_match_the_library_types_field_for_field() {
        assert_eq!(known(&CAPPED_POLICY), fields(&IssueConfig::default()));
        assert_eq!(known(&CAPPED_POLICY), fields(&ReadConfig::default()));
        assert_eq!(known(&RESOLVE_POLICY), fields(&ResolveConfig::default()));
        assert_eq!(known(&DEFAULTS), fields(&Defaults::default()));
        assert_eq!(known(&SELLER), fields(&SellerConfig::default()));
        assert_eq!(known(&SELLER_EMAIL), fields(&SellerEmailConfig::default()));
    }
}
