//! The prologue every handler runs after parsing its key (design §4): pin the
//! namespace, resolve the account, fetch its credentials, open the gateway.
//! This module holds the decisions of those steps — pure functions, which are
//! what can be unit-tested (the SDK has no mock context; the durable behaviour
//! is asserted end to end) — and the one step that runs outside the journal,
//! the credential fetch. The durable steps themselves are stamped per context
//! type in `support::{object, shared, service}::prologue`.

use std::sync::Arc;
use std::time::Duration;

use restate_sdk::errors::TerminalError;
use serde::{Deserialize, Serialize};
use szamlazz_agent::Credentials;

use super::support::Fault;
use crate::account::{Account, Accounts, BoxError, FetchError, ResolveError};
use crate::config::WorkerConfig;
use crate::gateway::Gateway;

/// What one handler execution runs on: the gateway opened for this execution
/// and the deployment settings with the namespace pinned by the journal.
///
/// Built by the prologue, dropped with the execution — no gateway or client
/// outlives one handler execution.
#[derive(Debug)]
pub(super) struct Execution {
    pub(super) gateway: Arc<Gateway>,
    pub(super) config: WorkerConfig,
}

/// The journaled answer of the `account` step: the account, or the reason the
/// request names none. Data, so that unscoped and unknown are settled by the
/// journal and never retried.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Resolution {
    /// The account the request names.
    Account(Box<Account>),
    /// The request carries no scope and the deployment serves no unscoped
    /// account.
    Unscoped,
    /// No account is reachable under the scope.
    Unknown {
        /// The scope the request carried.
        scope: String,
    },
}

/// The `account` step's one error: the resolver could not answer. Retryable
/// to the SDK, so the resolve policy re-executes the step; its display never
/// echoes the resolver's own message (it becomes `last_failure`).
#[derive(Debug, thiserror::Error)]
#[error("the account resolver is unavailable")]
pub(super) struct ResolverUnavailable(#[source] BoxError);

/// The `account` step's closure result: every answer of the resolver as
/// data, its unavailability as the retryable error.
pub(super) fn resolution(
    result: Result<Account, ResolveError>,
) -> Result<Resolution, ResolverUnavailable> {
    match result {
        Ok(account) => Ok(Resolution::Account(Box::new(account))),
        Err(ResolveError::Unscoped) => Ok(Resolution::Unscoped),
        Err(ResolveError::Unknown { scope }) => Ok(Resolution::Unknown { scope }),
        Err(ResolveError::Unavailable(source)) => Err(ResolverUnavailable(source)),
    }
}

/// The account of a journaled [`Resolution`]: unscoped and unknown are the
/// terminal fault `unknown_account` (HTTP 400) — the request named no account
/// of this deployment, and no retry with the same request changes that.
pub(super) fn account_of(resolution: Resolution) -> Result<Account, Fault> {
    match resolution {
        Resolution::Account(account) => Ok(*account),
        Resolution::Unscoped => Err(Fault::unknown_account(
            "no account is reachable unscoped: this deployment serves accounts by scope, send the request under the account's scope (/restate/scope/{scope}/call/…)",
        )),
        Resolution::Unknown { scope } => Err(Fault::unknown_account(format!(
            "no account is reachable under scope {scope:?}"
        ))),
    }
}

/// The fault of an `account` step that ended without a resolution: the
/// resolve policy is exhausted (500) or the invocation was cancelled (409).
pub(super) fn resolve_exhausted(error: &TerminalError) -> Fault {
    Fault::unavailable(format!(
        "the account could not be resolved ({}): {}; retry with a new Idempotency-Key",
        error.code(),
        error.message()
    ))
}

/// Fetches of the credential store per handler execution, including the
/// first. Short by design — a prolonged store outage is a terminal
/// `unavailable`, not a handler retry.
const FETCH_ATTEMPTS: u32 = 3;
/// The pause before each re-fetch.
const FETCH_PAUSE: Duration = Duration::from_millis(200);

/// Fetches the account's credentials outside the journal, on every
/// execution, with a short in-process retry of an unavailable store.
///
/// # Errors
///
/// The terminal `unavailable` fault: the store is gone for this reference
/// or stayed unavailable through the retries. Terminal by decision: a
/// retryable error would route a prolonged store outage into the handler's
/// kill-on-five and an unstructured 500, whereas this is structured and
/// immediate. The cost — an outage during a replay of an invocation whose
/// create already landed surfaces as `unavailable` although the document
/// exists — is reconciled by `get` or a retry with a new `Idempotency-Key`.
pub(super) async fn fetch_credentials(
    accounts: &Accounts,
    account: &Account,
) -> Result<Credentials, Fault> {
    let mut attempt = 1;
    loop {
        match accounts.fetch(account).await {
            Ok(credentials) => return Ok(credentials),
            Err(error @ FetchError::Unavailable(_)) if attempt < FETCH_ATTEMPTS => {
                tracing::warn!(
                    account = %account.id,
                    attempt,
                    error = %error,
                    "credential store unavailable; retrying"
                );
                attempt += 1;
                tokio::time::sleep(FETCH_PAUSE).await;
            }
            Err(error) => return Err(fetch_fault(account, &error)),
        }
    }
}

/// The terminal fault of a failed credential fetch. Never echoes the store's
/// own message; names the account and the reference.
pub(super) fn fetch_fault(account: &Account, error: &FetchError) -> Fault {
    tracing::warn!(
        account = %account.id,
        credential_ref = %account.credential_ref,
        error = %error,
        "credentials could not be fetched"
    );
    Fault::unavailable(format!(
        "credentials of account {} could not be fetched ({error}); retry with a new Idempotency-Key",
        account.id
    ))
}

/// Opens the gateway for this execution over a fresh client.
pub(super) fn open(account: Account, credentials: Credentials) -> Result<Arc<Gateway>, Fault> {
    Gateway::open(account, credentials)
        .map(Arc::new)
        .map_err(|error| {
            Fault::unavailable(format!(
                "the szamlazz.hu client could not be built: {error}"
            ))
        })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::account::{AccountId, CredentialRef};
    use crate::contract::TerminalCode;

    fn account() -> Account {
        Account::new(AccountId::from("acct"), CredentialRef::from("acct"))
    }

    fn fault_body(fault: Fault) -> (u16, serde_json::Value) {
        let error = TerminalError::from(fault);
        let body = serde_json::from_str(error.message()).expect("json body");
        (error.code(), body)
    }

    #[test]
    fn a_resolved_account_is_the_resolution() {
        assert_eq!(
            resolution(Ok(account())).expect("data"),
            Resolution::Account(Box::new(account()))
        );
        assert_eq!(
            account_of(Resolution::Account(Box::new(account()))).expect("account"),
            account()
        );
    }

    #[test]
    fn unscoped_and_unknown_are_data_and_the_unknown_account_fault() {
        let unscoped = resolution(Err(ResolveError::Unscoped)).expect("data");
        assert_eq!(unscoped, Resolution::Unscoped);
        let (status, body) = fault_body(account_of(unscoped).expect_err("fault"));
        assert_eq!(status, 400);
        assert_eq!(body["code"], TerminalCode::UnknownAccount.as_str());
        assert!(
            body["message"]
                .as_str()
                .expect("message")
                .contains("unscoped"),
            "{body}"
        );

        let unknown = resolution(Err(ResolveError::Unknown {
            scope: "acme-events".to_owned(),
        }))
        .expect("data");
        assert_eq!(
            unknown,
            Resolution::Unknown {
                scope: "acme-events".to_owned()
            }
        );
        let (status, body) = fault_body(account_of(unknown).expect_err("fault"));
        assert_eq!(status, 400);
        assert_eq!(body["code"], "unknown_account");
        assert!(
            body["message"]
                .as_str()
                .expect("message")
                .contains("acme-events"),
            "{body}"
        );
    }

    #[test]
    fn an_unavailable_resolver_is_the_retryable_error_and_never_echoes_its_cause() {
        let error = resolution(Err(ResolveError::unavailable(std::io::Error::other(
            "connection refused to db.internal:5432 (secret-dsn)",
        ))))
        .expect_err("retryable");
        let rendered = error.to_string();
        assert_eq!(rendered, "the account resolver is unavailable");
        assert!(!rendered.contains("secret-dsn"));
        // The cause stays reachable for logs.
        assert!(std::error::Error::source(&error).is_some());
    }

    #[test]
    fn an_exhausted_resolve_step_is_unavailable() {
        let (status, body) = fault_body(resolve_exhausted(&TerminalError::new_with_code(
            500,
            "the account resolver is unavailable",
        )));
        assert_eq!(status, 503);
        assert_eq!(body["code"], "unavailable");
        assert!(
            body["message"]
                .as_str()
                .expect("message")
                .contains("retry with a new Idempotency-Key"),
            "{body}"
        );
    }

    #[test]
    fn a_failed_credential_fetch_is_unavailable_and_never_echoes_the_cause() {
        let (status, body) = fault_body(fetch_fault(
            &account(),
            &FetchError::unavailable(std::io::Error::other("vault token v.abc123 rejected")),
        ));
        assert_eq!(status, 503);
        assert_eq!(body["code"], "unavailable");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains("acct"), "{message}");
        assert!(!message.contains("abc123"), "{message}");

        let (status, body) = fault_body(fetch_fault(
            &account(),
            &FetchError::Gone {
                credential_ref: CredentialRef::from("acct"),
            },
        ));
        assert_eq!(status, 503);
        assert_eq!(body["code"], "unavailable");
    }

    #[test]
    fn a_resolution_round_trips_through_json() {
        let resolution = Resolution::Account(Box::new(account()));
        let json = serde_json::to_value(&resolution).expect("json");
        assert_eq!(json["Account"]["id"], json!("acct"));
        assert_eq!(
            serde_json::from_value::<Resolution>(json).expect("back"),
            resolution
        );
    }
}
