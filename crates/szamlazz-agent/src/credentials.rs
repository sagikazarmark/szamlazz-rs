//! Authentication material for the Számla Agent.

use std::fmt;

/// A Számla Agent key (`számlaagentkulcs`), the preferred API credential.
///
/// Generated on the szamlazz.hu dashboard ("Számla Agent kulcsok"). Agent keys
/// are API-only: they cannot log in to the website. szamlazz.hu documents that
/// keys must be lowercase.
#[doc(alias = "számlaagentkulcs")]
#[derive(Clone, PartialEq, Eq)]
pub struct AgentKey(String);

impl AgentKey {
    /// Wraps an agent key.
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// The raw key, for serialization into the request XML.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for AgentKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AgentKey(…)")
    }
}

impl From<String> for AgentKey {
    fn from(key: String) -> Self {
        Self::new(key)
    }
}

impl From<&str> for AgentKey {
    fn from(key: &str) -> Self {
        Self::new(key)
    }
}

/// Credentials injected into each request's XML at the location required
/// by the operation: directly under the root for invoice XML/PDF queries,
/// and in `beallitasok` for the other built-in operations.
///
/// Credentials are client state, not document data: request types do not carry
/// them; they are supplied when a request is serialized to the wire.
#[derive(Clone)]
#[non_exhaustive]
pub enum Credentials {
    /// Authenticate with an agent key (preferred).
    AgentKey(AgentKey),
    /// Authenticate with a szamlazz.hu user (legacy; needed for third-party
    /// invoicing setups). The user must have access to exactly one account.
    /// The vendor also accepts the same agent key in both fields. `Debug`
    /// therefore redacts both the username and password.
    #[doc(alias = "felhasználó")]
    UserPassword {
        /// The szamlazz.hu username (`felhasznalo`).
        username: String,
        /// The password (`jelszo`).
        password: String,
    },
}

impl Credentials {
    /// Agent-key credentials.
    pub fn agent_key(key: impl Into<AgentKey>) -> Self {
        Self::AgentKey(key.into())
    }

    /// Username/password credentials.
    pub fn user_password(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self::UserPassword {
            username: username.into(),
            password: password.into(),
        }
    }
}

impl From<AgentKey> for Credentials {
    fn from(key: AgentKey) -> Self {
        Self::AgentKey(key)
    }
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AgentKey(_) => f.write_str("Credentials::AgentKey(…)"),
            Self::UserPassword { .. } => f
                .debug_struct("Credentials::UserPassword")
                .field("username", &"…")
                .field("password", &"…")
                .finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The key never reaches a log through `Debug`, nor does a password.
    #[test]
    fn debug_is_redacted() {
        let debug = format!("{:?}", AgentKey::new("secret"));
        assert!(!debug.contains("secret"), "{debug}");
        let debug = format!("{:?}", Credentials::agent_key("secret"));
        assert!(!debug.contains("secret"), "{debug}");
        for (username, password) in [("login-name", "hunter2"), ("legacy-key", "legacy-key")] {
            let debug = format!("{:?}", Credentials::user_password(username, password));
            assert!(!debug.contains(username), "{debug}");
            assert!(!debug.contains(password), "{debug}");
        }
    }
}
