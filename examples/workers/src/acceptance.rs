//! Compiled only for local acceptance; no control routes in the example build.

use restate_szamlazz::{
    Credentials,
    account::{
        Account, AccountResolver, Accounts, BoxFuture, CredentialRef, CredentialStore, FetchError,
        ResolveError,
    },
    config::WorkerConfig,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

pub async fn probe(
    accounts: &Accounts,
    request: &worker::HttpRequest,
) -> worker::Result<worker::HttpResponse> {
    let account = accounts
        .resolve(None)
        .await
        .map_err(|_| super::invalid("probe resolution"))?;
    let credentials = accounts
        .fetch(&account)
        .await
        .map_err(|_| super::invalid("probe credentials"))?;
    let gateway = restate_szamlazz::gateway::Gateway::open(account, credentials)
        .map_err(|_| super::invalid("probe initialization"))?;
    let repeat = request.uri().query() == Some("repeat");
    if request.uri().query() == Some("cancel") {
        use futures_util::future::{Either, select};
        let external_id = restate_szamlazz::identity::ExternalId::new("workers:check-account");
        let operation = gateway.probe(&external_id);
        let timer = gloo_timers::future::TimeoutFuture::new(200);
        let cancelled = matches!(
            select(std::pin::pin!(operation), std::pin::pin!(timer)).await,
            Either::Right(_)
        );
        return worker::Response::from_json(&serde_json::json!({"cancelled":cancelled}))?
            .try_into();
    }
    let mut accepted = Vec::new();
    for _ in 0..if repeat { 2 } else { 1 } {
        accepted.push(matches!(
            gateway
                .probe(&restate_szamlazz::identity::ExternalId::new(
                    "workers:check-account"
                ))
                .await,
            Ok(restate_szamlazz::gateway::ProbeOutcome::Accepted)
        ));
    }
    worker::Response::from_json(&serde_json::json!({"accepted":accepted}))?.try_into()
}

pub fn config(mut config: WorkerConfig) -> WorkerConfig {
    config.read.max_attempts = Some(1);
    config.resolve.max_attempts = Some(1);
    config
}

pub fn accounts(accounts: Accounts, env: &worker::Env) -> Accounts {
    let mode = env
        .var("ACCEPTANCE_CREDENTIALS")
        .map(|v| v.to_string())
        .unwrap_or_default();
    let source = Arc::new(Source {
        accounts,
        mode,
        fetches: AtomicUsize::new(0),
    });
    Accounts::new(source.clone(), source)
}

struct Source {
    accounts: Accounts,
    mode: String,
    fetches: AtomicUsize,
}
impl AccountResolver for Source {
    fn resolve<'a>(
        &'a self,
        scope: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Account, ResolveError>> {
        Box::pin(async move {
            if self.mode == "resolver-hang" {
                std::future::pending::<()>().await;
            }
            self.accounts.resolve(scope).await
        })
    }
}
impl CredentialStore for Source {
    fn fetch<'a>(
        &'a self,
        credential_ref: &'a CredentialRef,
    ) -> BoxFuture<'a, Result<Credentials, FetchError>> {
        Box::pin(async move {
            if self.mode == "store-hang" {
                std::future::pending::<()>().await;
            }
            if self.mode == "missing" {
                return Err(FetchError::Gone {
                    credential_ref: credential_ref.clone(),
                });
            }
            if self.mode == "retry" && self.fetches.fetch_add(1, Ordering::SeqCst) < 2 {
                return Err(FetchError::unavailable("ACCEPTANCE-PRIVATE-SOURCE"));
            }
            self.accounts
                .fetch(&Account::new("acceptance", credential_ref.clone()))
                .await
        })
    }
}
