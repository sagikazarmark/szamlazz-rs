//! The static resolver: accounts from deployment configuration.
//!
//! ```toml
//! [account]
//! id = "acme"
//! agent_key = "..."
//! endpoint = "https://www.szamlazz.hu/szamla/"   # default
//! mode = "live"                                  # default
//! supplier_id = 972720
//!
//! [account.defaults]
//! currency = "HUF"
//!
//! [account.seller]
//! bank_account = "..."
//! ```
//!
//! The configuration types implement `Deserialize` only; the endpoint binary
//! chooses the file format and environment merging. [`StaticResolver`] is
//! built from a parsed [`StaticConfig`] with `TryFrom`, which validates what
//! `Deserialize` cannot, and implements both [`AccountResolver`] and
//! [`CredentialStore`]: the agent key is inline and the credential reference
//! is the account id.
//!
//! This is the single-account shape: the account is reachable unscoped and
//! any scope is unknown.

use serde::Deserialize;
use szamlazz_agent::Credentials;

use super::{
    Account, AccountId, AccountResolver, BoxFuture, CredentialRef, CredentialStore, Endpoint,
    FetchError, InvalidEndpoint, ResolveError,
};
use crate::config::{AccountMode, Defaults, Secret, SellerConfig};

/// The static resolver's configuration: one `[account]`.
#[derive(Debug, Clone, Deserialize)]
pub struct StaticConfig {
    /// The account this deployment issues for, reachable unscoped.
    pub account: StaticAccount,
}

/// One account as configured statically: the [`Account`] fields plus the
/// agent key inline.
#[derive(Debug, Clone, Deserialize)]
pub struct StaticAccount {
    /// The account's identifier; also its credential reference.
    pub id: AccountId,
    /// The Agent key (`számlaagentkulcs`). A number is accepted: keys may be
    /// all digits, and an unquoted one is a number to TOML and YAML.
    pub agent_key: Secret,
    /// The Számla Agent endpoint; the production URL when absent. Validated
    /// when the resolver is built.
    #[serde(default)]
    pub endpoint: Option<String>,
    /// Whether the account is live or a test account. Default live; always
    /// validated against `teszt`.
    #[serde(default)]
    pub mode: AccountMode,
    /// The account's supplier id (`szállító/id`), an optional ownership pin.
    #[serde(default)]
    pub supplier_id: Option<u64>,
    /// Document defaults that per-call overrides may change.
    #[serde(default)]
    pub defaults: Defaults,
    /// The seller block; account data is used where absent.
    #[serde(default)]
    pub seller: SellerConfig,
}

/// A [`StaticConfig`] that parsed but violates an invariant.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StaticConfigError {
    /// `account.id` is empty or blank.
    #[error("account.id must not be empty")]
    EmptyId,
    /// The account's `agent_key` is empty or blank.
    #[error("account {id}: agent_key must not be empty")]
    EmptyAgentKey {
        /// The account.
        id: AccountId,
    },
    /// The account's `endpoint` is not an http(s) URL.
    #[error("account {id}: {source}")]
    InvalidEndpoint {
        /// The account.
        id: AccountId,
        /// Why the endpoint is invalid.
        source: InvalidEndpoint,
    },
}

/// Accounts from deployment configuration; the resolver and the credential
/// store of a static deployment, one struct.
#[derive(Debug, Clone)]
pub struct StaticResolver {
    account: Account,
    credentials: Credentials,
}

impl StaticResolver {
    /// The configured account.
    #[must_use]
    pub fn account(&self) -> &Account {
        &self.account
    }
}

impl TryFrom<StaticConfig> for StaticResolver {
    type Error = StaticConfigError;

    /// Validates the configuration: a non-blank id and agent key, and an
    /// endpoint that is an http(s) URL.
    fn try_from(config: StaticConfig) -> Result<Self, Self::Error> {
        let StaticAccount {
            id,
            agent_key,
            endpoint,
            mode,
            supplier_id,
            defaults,
            seller,
        } = config.account;
        if id.as_str().trim().is_empty() {
            return Err(StaticConfigError::EmptyId);
        }
        if agent_key.expose().trim().is_empty() {
            return Err(StaticConfigError::EmptyAgentKey { id });
        }
        let endpoint = match endpoint {
            Some(endpoint) => {
                Endpoint::parse(&endpoint).map_err(|source| StaticConfigError::InvalidEndpoint {
                    id: id.clone(),
                    source,
                })?
            }
            None => Endpoint::production(),
        };
        let mut account = Account::new(id.clone(), CredentialRef::from(id.as_str()));
        account.mode = mode;
        account.supplier_id = supplier_id;
        account.endpoint = endpoint;
        account.defaults = defaults;
        account.seller = seller;
        Ok(Self {
            account,
            credentials: Credentials::agent_key(agent_key.expose()),
        })
    }
}

impl AccountResolver for StaticResolver {
    /// Unscoped → the account; any scope → unknown.
    fn resolve<'a>(
        &'a self,
        scope: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Account, ResolveError>> {
        Box::pin(async move {
            match scope {
                None => Ok(self.account.clone()),
                Some(scope) => Err(ResolveError::Unknown {
                    scope: scope.to_owned(),
                }),
            }
        })
    }
}

impl CredentialStore for StaticResolver {
    /// The inline key under the account's id; anything else is gone.
    fn fetch<'a>(
        &'a self,
        credential_ref: &'a CredentialRef,
    ) -> BoxFuture<'a, Result<Credentials, FetchError>> {
        Box::pin(async move {
            if *credential_ref == self.account.credential_ref {
                Ok(self.credentials.clone())
            } else {
                Err(FetchError::Gone {
                    credential_ref: credential_ref.clone(),
                })
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The single-account shape as the endpoint binary would hand it over
    /// after merging its file format and environment overrides.
    fn single() -> StaticConfig {
        serde_json::from_value(json!({
            "account": {
                "id": "acme",
                "agent_key": "key-acme",
                "endpoint": "http://127.0.0.1:1/",
                "mode": "test",
                "supplier_id": 972_720,
                "defaults": { "currency": "EUR", "e_invoice": true },
                "seller": { "bank_account": "11111111-22222222" },
            },
        }))
        .expect("config")
    }

    #[test]
    fn single_account_shape_parses_and_builds_the_account() {
        let resolver = StaticResolver::try_from(single()).expect("resolver");
        let account = resolver.account();
        assert_eq!(account.id.as_str(), "acme");
        assert_eq!(
            account.credential_ref.as_str(),
            "acme",
            "credential_ref = id"
        );
        assert_eq!(account.mode, AccountMode::Test);
        assert_eq!(account.supplier_id, Some(972_720));
        assert_eq!(account.endpoint.as_str(), "http://127.0.0.1:1/");
        assert_eq!(account.defaults.currency, "EUR");
        assert!(account.defaults.e_invoice);
        assert_eq!(
            account.defaults.language, "hu",
            "unset defaults keep their spec value"
        );
        assert_eq!(
            account.seller.bank_account.as_deref(),
            Some("11111111-22222222")
        );
    }

    #[test]
    fn mode_omitted_is_live_and_the_endpoint_defaults_to_production() {
        let config: StaticConfig =
            serde_json::from_value(json!({ "account": { "id": "acme", "agent_key": "k" } }))
                .expect("config");
        let resolver = StaticResolver::try_from(config).expect("resolver");
        assert_eq!(resolver.account().mode, AccountMode::Live);
        assert_eq!(resolver.account().supplier_id, None);
        assert_eq!(
            resolver.account().endpoint.as_str(),
            "https://www.szamlazz.hu/szamla/"
        );
    }

    #[tokio::test]
    async fn resolve_unscoped_is_the_account_and_any_scope_is_unknown() {
        let resolver = StaticResolver::try_from(single()).expect("resolver");
        let account = resolver.resolve(None).await.expect("unscoped");
        assert_eq!(&account, resolver.account());
        assert!(
            matches!(
                resolver.resolve(Some("acme")).await,
                Err(ResolveError::Unknown { scope }) if scope == "acme"
            ),
            "even the account's own id is not a scope in the single shape"
        );
    }

    #[tokio::test]
    async fn fetch_by_the_account_id_is_the_inline_key_and_anything_else_is_gone() {
        let resolver = StaticResolver::try_from(single()).expect("resolver");
        let credentials = resolver
            .fetch(&resolver.account().credential_ref)
            .await
            .expect("fetch");
        assert!(matches!(
            credentials,
            Credentials::AgentKey(key) if key.expose() == "key-acme"
        ));
        assert!(matches!(
            resolver.fetch(&CredentialRef::from("other")).await,
            Err(FetchError::Gone { credential_ref }) if credential_ref.as_str() == "other"
        ));
    }

    #[test]
    fn invalid_endpoint_and_blank_key_and_blank_id_are_construction_errors() {
        let mut config = single();
        config.account.endpoint = Some("localhost".to_owned());
        assert!(matches!(
            StaticResolver::try_from(config),
            Err(StaticConfigError::InvalidEndpoint { id, .. }) if id.as_str() == "acme"
        ));

        let mut config = single();
        config.account.agent_key = " ".into();
        assert!(matches!(
            StaticResolver::try_from(config),
            Err(StaticConfigError::EmptyAgentKey { id }) if id.as_str() == "acme"
        ));

        let mut config = single();
        config.account.id = " ".into();
        assert!(matches!(
            StaticResolver::try_from(config),
            Err(StaticConfigError::EmptyId)
        ));
    }

    /// szamlazz.hu agent keys may be all digits, and a TOML or YAML author
    /// writing one unquoted produces a number.
    #[tokio::test]
    async fn numeric_agent_key_is_accepted() {
        let config: StaticConfig = serde_json::from_value(json!({
            "account": { "id": "acme", "agent_key": 12_345_678 },
        }))
        .expect("config");
        let resolver = StaticResolver::try_from(config).expect("resolver");
        let credentials = resolver
            .fetch(&CredentialRef::from("acme"))
            .await
            .expect("fetch");
        assert!(matches!(
            credentials,
            Credentials::AgentKey(key) if key.expose() == "12345678"
        ));
    }

    #[test]
    fn debug_renderings_redact_the_agent_key() {
        const KEY: &str = "sentinel-agent-key-7d1e";
        let config: StaticConfig = serde_json::from_value(json!({
            "account": { "id": "acme", "agent_key": KEY },
        }))
        .expect("config");
        assert!(!format!("{config:?}").contains(KEY), "{config:?}");
        let resolver = StaticResolver::try_from(config).expect("resolver");
        assert!(!format!("{resolver:?}").contains(KEY), "{resolver:?}");
    }
}
