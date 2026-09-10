//! Discovery and binding tests of the Restate adapters: the service names,
//! the handler set with its shared flags and the per-handler retry policy,
//! plus an `Endpoint` build; the body decode every handler shares; and the
//! sentinel that the agent key reaches neither the `credentials_rejected`
//! warning nor its fault body. The decisions of each module are tested
//! inline, in the module.

use restate_sdk::discovery::{HandlerType, RetryPolicyOnMaxAttempts, ServiceType};
use restate_sdk::endpoint::Endpoint;
use restate_sdk::service::Discoverable;
use serde_json::json;

use super::{Agent, Order};
use crate::account::{Accounts, ResolveError, StaticConfig, StaticResolver};
use crate::config::{IssueConfig, ValidatedWorkerConfig, WorkerConfig};
use crate::gateway::SzamlazzAnswer;
use crate::identity::Namespace;
use crate::test_support::open_gateway;

/// [`IssueConfig::MIN_INITIAL_DELAY`] in the unit discovery reports
/// (milliseconds): the floor the write handlers' `initial_interval` clears.
#[allow(clippy::cast_possible_truncation)]
const MIN_INITIAL_DELAY_MS: u64 = IssueConfig::MIN_INITIAL_DELAY.as_millis() as u64;

/// The `inactivity_timeout` / `abort_timeout` of every handler whose step is
/// one szamlazz.hu round trip (the four reads and `set_credit_entries`' one send)
/// in the discovery reports (milliseconds): `2m`, the 60 s client timeout
/// plus the margin a stalling szamlazz.hu needs (#114). The writes whose step
/// is three trips carry `4m` / `3m`. A literal, like the attributes it
/// asserts (the handler macro takes no constant).
const ONE_TRIP_TIMEOUT_MS: u64 = 120_000;

/// The `Accounts` bundle of a test account at `endpoint` with `agent_key`,
/// through the static resolver: what a deployment builds.
fn accounts(endpoint: &str, agent_key: &str) -> Accounts {
    let config: StaticConfig = serde_json::from_value(json!({
        "account": {
            "id": "acct",
            "agent_key": agent_key,
            "endpoint": endpoint,
        },
    }))
    .expect("config");
    Accounts::from(StaticResolver::try_from(config).expect("resolver"))
}

fn namespace() -> Namespace {
    "acct".parse().expect("namespace")
}

/// The default deployment settings under `acct`, validated.
fn worker_config() -> ValidatedWorkerConfig {
    WorkerConfig::new(namespace()).validate().expect("valid")
}

#[test]
fn order_discovers_as_a_virtual_object_with_eight_public_handlers() {
    let discovery = <Order as Discoverable>::discover();
    assert_eq!(discovery.name.as_str(), "Szamlazz.Order");
    assert_eq!(discovery.ty, ServiceType::VirtualObject);

    let mut names: Vec<_> = discovery
        .handlers
        .iter()
        .map(|handler| handler.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "correct_invoice",
            "create_final",
            "create_invoice",
            "create_prepayment",
            "create_proforma",
            "delete_proforma",
            "get",
            "storno_invoice",
        ]
    );

    for handler in &discovery.handlers {
        let name = handler.name.as_str();
        assert_eq!(handler.ingress_private, None, "{name} is public");
        assert!(handler.output.is_some(), "{name} returns an output");
        assert_eq!(
            handler.retry_policy_on_max_attempts,
            Some(RetryPolicyOnMaxAttempts::Kill),
            "{name}"
        );
        if name == "get" {
            // Read-only: shared, an empty input, the reads' back-off (10s →
            // 1m) with three attempts, inherited idempotency retention; an explicit journal
            // retention so the journal is inspectable. The timeouts are the
            // reads' 2m / 2m (#114): a read step is one szamlazz.hu round trip
            // bounded by the 60 s client timeout, and szamlazz.hu has been
            // seen to stall for a minute and still answer; the server's 1 m
            // default would suspend exactly such a read.
            assert_eq!(handler.ty, Some(HandlerType::Shared));
            let input = handler.input.as_ref().expect("an empty input payload");
            assert!(
                input.content_type.is_none() && input.json_schema.is_none(),
                "get takes no input"
            );
            assert_eq!(handler.retry_policy_max_attempts, Some(3));
            assert_eq!(handler.retry_policy_initial_interval, Some(10_000));
            assert_eq!(handler.retry_policy_exponentiation_factor, Some(2.0));
            assert_eq!(handler.retry_policy_max_interval, Some(60_000));
            assert_eq!(handler.inactivity_timeout, Some(ONE_TRIP_TIMEOUT_MS));
            assert_eq!(handler.abort_timeout, Some(ONE_TRIP_TIMEOUT_MS));
            assert_eq!(handler.journal_retention, Some(24 * 3_600_000));
            assert_eq!(handler.idempotency_retention, None);
            continue;
        }
        // Exclusive is the Virtual Object default and left implicit (`None`).
        assert_eq!(handler.ty, None, "{name}");
        assert!(handler.input.is_some(), "{name} takes an input");
        // Every handler that calls szamlazz.hu kills after 5 attempts with a
        // 2m → 10m back-off and bounded timeouts. The 2m is the same rule as
        // the issue policy's floor: the retry after a crash waits out the
        // client timeout plus a margin.
        assert_eq!(
            handler.retry_policy_initial_interval,
            Some(120_000),
            "{name}"
        );
        assert!(
            handler.retry_policy_initial_interval >= Some(MIN_INITIAL_DELAY_MS),
            "{name}: initial_interval under IssueConfig::MIN_INITIAL_DELAY"
        );
        assert_eq!(handler.retry_policy_max_interval, Some(600_000), "{name}");
        assert_eq!(
            handler.retry_policy_exponentiation_factor,
            Some(2.0),
            "{name}"
        );
        assert_eq!(handler.retry_policy_max_attempts, Some(5), "{name}");
        assert_eq!(handler.inactivity_timeout, Some(240_000), "{name}");
        assert_eq!(handler.abort_timeout, Some(180_000), "{name}");
        assert_eq!(
            handler.journal_retention,
            Some(3 * 24 * 3_600_000),
            "{name}"
        );
        assert_eq!(
            handler.idempotency_retention,
            Some(30 * 24 * 3_600_000),
            "{name}"
        );
    }
}

#[test]
fn agent_discovers_as_a_service_with_five_handlers() {
    let discovery = <Agent as Discoverable>::discover();
    assert_eq!(discovery.name.as_str(), "Szamlazz.Agent");
    assert_eq!(discovery.ty, ServiceType::Service);

    let mut names: Vec<_> = discovery
        .handlers
        .iter()
        .map(|handler| handler.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "check_account",
            "query",
            "query_taxpayer",
            "set_credit_entries",
            "storno"
        ]
    );

    for handler in &discovery.handlers {
        let name = handler.name.as_str();
        assert_eq!(handler.ingress_private, None, "{name} is public");
        assert_eq!(
            handler.retry_policy_on_max_attempts,
            Some(RetryPolicyOnMaxAttempts::Kill),
            "{name}"
        );
        if name == "query" || name == "query_taxpayer" || name == "check_account" {
            // Read-only: a short 10s → 1m back-off, three attempts, inherited
            // idempotency retention (None does not disable deduplication); an explicit journal
            // retention so the journal is inspectable, and, for the probe,
            // so the leak assertion can scan it. The reads' 2m / 2m timeouts
            // (#114): one 60 s round trip plus the margin a stalling
            // szamlazz.hu needs, the same rule as `set_credit_entries`' one send.
            assert_eq!(
                handler.retry_policy_initial_interval,
                Some(10_000),
                "{name}"
            );
            assert_eq!(handler.retry_policy_max_interval, Some(60_000), "{name}");
            assert_eq!(
                handler.retry_policy_exponentiation_factor,
                Some(2.0),
                "{name}"
            );
            assert_eq!(handler.retry_policy_max_attempts, Some(3), "{name}");
            assert_eq!(
                handler.inactivity_timeout,
                Some(ONE_TRIP_TIMEOUT_MS),
                "{name}"
            );
            assert_eq!(handler.abort_timeout, Some(ONE_TRIP_TIMEOUT_MS), "{name}");
            assert_eq!(handler.journal_retention, Some(24 * 3_600_000), "{name}");
            assert_eq!(handler.idempotency_retention, None, "{name}");
            if name == "check_account" {
                let input = handler.input.as_ref().expect("an empty input payload");
                assert!(
                    input.content_type.is_none() && input.json_schema.is_none(),
                    "check_account takes no input"
                );
            } else {
                assert!(handler.input.is_some(), "{name} takes an input");
            }
            assert!(handler.output.is_some(), "{name} returns an output");
        } else {
            // The two writes: kill; the journal and the idempotency completion
            // retained like `Szamlazz.Order`'s.
            assert_eq!(
                handler.journal_retention,
                Some(3 * 24 * 3_600_000),
                "{name}"
            );
            assert_eq!(
                handler.idempotency_retention,
                Some(30 * 24 * 3_600_000),
                "{name}"
            );
            // Both writes wait out the 60 s client timeout before the retry
            // after a crash (never the server's ~500 ms default), so that the
            // re-execution cannot run while the first send is still in flight:
            // `set_credit_entries` because an additive send is at-least-once,
            // `storno` because its re-execution's leading query would
            // otherwise look before the cut send has landed. The same rule
            // floors the issue policy's `initial_delay`.
            assert_eq!(
                handler.retry_policy_initial_interval,
                Some(120_000),
                "{name}"
            );
            assert!(
                handler.retry_policy_initial_interval >= Some(MIN_INITIAL_DELAY_MS),
                "{name}: initial_interval under IssueConfig::MIN_INITIAL_DELAY"
            );
            if name == "storno" {
                // The storno step is the same closure `Szamlazz.Order` runs, so
                // the policy is `Szamlazz.Order`'s throughout (#87): five
                // attempts, 2m → 10m (invocation attempts are spent only on
                // worker-side failures and every re-dispatch is query-first,
                // so nothing about an unmanaged storno justifies a shorter
                // budget), and the 4m/3m timeouts (query, send, re-query at
                // 60 s each): anything shorter suspends a slow storno mid-step.
                assert_eq!(handler.retry_policy_max_interval, Some(600_000), "{name}");
                assert_eq!(
                    handler.retry_policy_exponentiation_factor,
                    Some(2.0),
                    "{name}"
                );
                assert_eq!(handler.retry_policy_max_attempts, Some(5), "{name}");
                assert_eq!(handler.inactivity_timeout, Some(240_000), "{name}");
                assert_eq!(handler.abort_timeout, Some(180_000), "{name}");
            } else {
                assert_eq!(name, "set_credit_entries");
                // Two attempts: an additive send is at-least-once, so every
                // invocation attempt is a potential second copy of the entries.
                assert_eq!(handler.retry_policy_max_attempts, Some(2), "{name}");
                assert_eq!(
                    handler.inactivity_timeout,
                    Some(ONE_TRIP_TIMEOUT_MS),
                    "{name}"
                );
                assert_eq!(handler.abort_timeout, Some(ONE_TRIP_TIMEOUT_MS), "{name}");
            }
        }
    }
}

/// Both services hold the same accounts and the same deployment-level
/// settings, and nothing else: no gateway, no client.
#[tokio::test]
async fn services_bind_to_an_endpoint() {
    let worker = WorkerConfig::new(namespace()).validate().expect("valid");
    let order = Order::from_parts(accounts("http://127.0.0.1:1/", "key"), worker.clone());
    let agent = Agent::from_parts(order.accounts().clone(), order.config().clone());
    assert_eq!(order.config(), agent.config());
    assert_eq!(*order.config(), worker);
    assert_eq!(order.config().namespace.as_str(), "acct");
    // The static resolver: the single account, unscoped, with the inline key.
    let account = order.accounts().resolve(None).await.expect("account");
    assert_eq!(account.id.as_str(), "acct");
    assert!(order.accounts().fetch(&account).await.is_ok());
    assert!(
        matches!(
            agent.accounts().resolve(Some("acme-events")).await,
            Err(ResolveError::Unknown { scope }) if scope == "acme-events"
        ),
        "a single-account deployment knows no scope"
    );
    let _endpoint = Endpoint::builder().bind(order).bind(agent).build();
}

/// An embedder's store naturally derives `Debug` over its key map. The
/// crate's own `Debug` impls (`Accounts`, and `Order` / `Agent` over it)
/// never descend into the plugged-in resolver or store, so such a store's
/// keys cannot reach a log line through the services' `Debug`.
#[test]
fn a_leaky_store_does_not_print_its_keys_through_accounts_order_or_agent() {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use szamlazz_agent::Credentials;

    use crate::account::{
        Account, AccountResolver, BoxFuture, CredentialRef, CredentialStore, FetchError,
        ResolveError,
    };

    const KEY: &str = "sentinel-agent-key-7d2f9a";

    /// What a database-backed deployment might write first: the raw keys in
    /// a map, `Debug` derived.
    #[derive(Debug, Clone)]
    struct Leaky {
        keys: BTreeMap<String, String>,
    }

    impl AccountResolver for Leaky {
        fn resolve<'a>(
            &'a self,
            _scope: Option<&'a str>,
        ) -> BoxFuture<'a, Result<Account, ResolveError>> {
            Box::pin(async { Ok(Account::new("acme", "acme-key")) })
        }
    }

    impl CredentialStore for Leaky {
        fn fetch<'a>(
            &'a self,
            credential_ref: &'a CredentialRef,
        ) -> BoxFuture<'a, Result<Credentials, FetchError>> {
            Box::pin(async move {
                self.keys
                    .get(credential_ref.as_str())
                    .map(|key| Credentials::agent_key(key.as_str()))
                    .ok_or_else(|| FetchError::Gone {
                        credential_ref: credential_ref.clone(),
                    })
            })
        }
    }

    let leaky = Arc::new(Leaky {
        keys: BTreeMap::from([("acme-key".to_owned(), KEY.to_owned())]),
    });
    assert!(
        format!("{leaky:?}").contains(KEY),
        "the store really does print its keys"
    );

    let accounts = Accounts::new(leaky.clone() as Arc<dyn AccountResolver>, leaky);
    let order = Order::from_parts(accounts.clone(), worker_config());
    let agent = Agent::from_parts(accounts.clone(), worker_config());
    for (label, rendering) in [
        ("Accounts", format!("{accounts:?}")),
        ("Order", format!("{order:?}")),
        ("Agent", format!("{agent:?}")),
        ("Order alternate", format!("{order:#?}")),
    ] {
        assert!(!rendering.contains(KEY), "{label}: {rendering}");
        assert!(!rendering.contains("acme-key"), "{label}: {rendering}");
    }
}

/// A handler's body is decoded by the handler, not the SDK: `Body<T>`'s SDK
/// `Deserialize` never fails (it keeps the verdict), so a malformed body
/// reaches the handler and leaves it as the structured `invalid_input` fault
/// (400, `{code, message}`) with serde's message, naming the field when there
/// is one; never the SDK's plain-text `Cannot decode input payload`.
#[test]
fn a_malformed_body_is_a_structured_invalid_input() {
    use bytes::Bytes;
    use restate_sdk::errors::TerminalError;
    use restate_sdk::serde::Deserialize as _;

    use super::Body;
    use crate::contract::document::tests::sample_document;
    use crate::contract::{CreateRequest, DeleteProformaRequest, SetCreditEntriesRequest};

    /// What the SDK hands the handler for `bytes`: the decode never fails.
    fn body<T: for<'de> serde::Deserialize<'de>>(bytes: impl Into<Bytes>) -> Body<T> {
        Body::<T>::deserialize(&mut bytes.into()).expect("never fails")
    }

    /// The message of the `invalid_input` fault (400) `bytes` is refused with.
    fn refused<T>(bytes: impl Into<Bytes>) -> String
    where
        T: for<'de> serde::Deserialize<'de> + std::fmt::Debug,
    {
        let error = TerminalError::try_from(body::<T>(bytes).into_request().expect_err("refused"))
            .expect("known fault");
        assert_eq!(error.code(), 400);
        let fault: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
        assert_eq!(fault["code"], "invalid_input");
        assert_eq!(fault["order"], serde_json::Value::Null);
        fault["message"].as_str().expect("message").to_owned()
    }

    let document = serde_json::to_value(sample_document()).expect("serialize");

    // A well-formed body decodes to the request.
    let request = body::<CreateRequest>(json!({"document": document}).to_string())
        .into_request()
        .expect("a well-formed body");
    assert_eq!(request.document, sample_document());
    assert!(!request.options.reissue);

    // A misspelt option: refused, naming the field and the known ones.
    let message = refused::<CreateRequest>(
        json!({"document": document, "options": {"resissue": true}}).to_string(),
    );
    assert!(message.starts_with("malformed request body: "), "{message}");
    assert!(message.contains("unknown field `resissue`"), "{message}");
    assert!(message.contains("`reissue`"), "{message}");

    // A wrong type, a missing required field and invalid JSON are the same
    // fault; serde's message says what it can.
    let message = refused::<DeleteProformaRequest>(json!({"force": "yes"}).to_string());
    assert!(message.contains("expected a boolean"), "{message}");
    let message = refused::<SetCreditEntriesRequest>(json!({"invoice_number": "SZ-1"}).to_string());
    assert!(message.contains("missing field `entries`"), "{message}");
    let message = refused::<CreateRequest>("not json");
    assert!(message.contains("expected"), "{message}");
    refused::<CreateRequest>(Bytes::new());
}

/// `Body<T>` changes how a body is decoded, not what the discovery manifest
/// says about it: its schema and input metadata are `Json<T>`'s, so the
/// `OpenAPI` export is unchanged and still carries `additionalProperties: false`
/// for the request types.
#[test]
fn body_discovers_exactly_as_json() {
    use restate_sdk::discovery::InputPayload;
    use restate_sdk::serde::{Json, PayloadMetadata as _};

    use super::Body;
    use crate::contract::{
        CorrectRequest, CreateRequest, DeleteProformaRequest, QueryRequest, QueryTaxpayerRequest,
        SetCreditEntriesRequest, StornoRequest,
    };

    macro_rules! same_as_json {
        ($($request:ty),* $(,)?) => {$(
            assert_eq!(
                <Body<$request>>::json_schema(),
                <Json<$request>>::json_schema(),
                stringify!($request)
            );
            let body = InputPayload::from_metadata::<Body<$request>>();
            let json = InputPayload::from_metadata::<Json<$request>>();
            assert_eq!(body.content_type, json.content_type, stringify!($request));
            assert_eq!(body.required, json.required, stringify!($request));
            assert_eq!(body.json_schema, json.json_schema, stringify!($request));
        )*};
    }
    same_as_json!(
        CreateRequest,
        CorrectRequest,
        StornoRequest,
        DeleteProformaRequest,
        QueryRequest,
        QueryTaxpayerRequest,
        SetCreditEntriesRequest,
    );

    #[cfg(feature = "schemars")]
    {
        let schema = <Body<CreateRequest>>::json_schema().expect("a schema");
        assert_eq!(schema["title"], "CreateRequest");
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(
            schema["$defs"]["CreateOptions"]["additionalProperties"],
            false
        );
    }

    // The discovery manifest of the services carries it: every handler with
    // an input has a JSON input with the request's schema.
    let discovery = <Order as Discoverable>::discover();
    let create = discovery
        .handlers
        .iter()
        .find(|handler| handler.name.as_str() == "create_invoice")
        .expect("create_invoice");
    let input = create.input.as_ref().expect("an input");
    assert_eq!(input.content_type.as_deref(), Some("application/json"));
    assert_eq!(input.required, Some(true));
    assert_eq!(input.json_schema, <Json<CreateRequest>>::json_schema());
}

/// The agent key never reaches the operator's warning or the caller's fault
/// body: both are built from what szamlazz.hu answered, tagged with the
/// namespace and the code only. Every event the crate emits during the
/// exchange and the fault construction is captured at `TRACE`, and the key is
/// demonstrably on the wire when the rejection is observed.
#[tokio::test]
async fn credentials_rejected_never_leaks_the_agent_key() {
    use restate_sdk::errors::TerminalError;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::support::AnsweredCode;
    use crate::gateway::QueryOutcome;
    use crate::test_support::LogCapture;

    const KEY: &str = "sentinel-agent-key-9f3a7c";
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod>3</hibakod><hibauzenet>Sikertelen bejelentkezés.</hibauzenet></xmlszamlavalasz>"#,
            "application/xml",
        ))
        .expect(1)
        .mount(&server)
        .await;
    let order = Order::from_parts(accounts(&server.uri(), KEY), worker_config());

    let capture = LogCapture::default();
    let guard = capture.subscribe();

    // Pin the warning's callsite to this thread's subscriber (see
    // `LogCapture`). The warm-up event is told apart by its namespace.
    drop(
        AnsweredCode::CredentialsRejected(SzamlazzAnswer::new("0", "warm-up"))
            .into_fault(&"warmup".parse().expect("namespace")),
    );
    LogCapture::rebuild_interest();

    // Resolve the account, then fetch/open as an executing operation does; the gateway
    // observes the code and the fault is built.
    let account = order.accounts().resolve(None).await.expect("account");
    let credentials = order.accounts().fetch(&account).await.expect("credentials");
    let gateway = open_gateway(account, credentials);
    let outcome = gateway.verify("SZ-1").await;
    let Ok(QueryOutcome::CredentialsRejected(answer)) = outcome.clone() else {
        panic!("expected CredentialsRejected, got {outcome:?}");
    };
    assert_eq!(answer.code, "3");
    let error = TerminalError::try_from(
        AnsweredCode::CredentialsRejected(answer).into_fault(&order.config().namespace),
    )
    .expect("known fault");
    drop(guard);

    let sent = server.received_requests().await.expect("requests");
    assert!(
        String::from_utf8_lossy(&sent[0].body).contains(KEY),
        "the sentinel key must have been on the wire for the test to mean anything"
    );
    assert!(!format!("{outcome:?}").contains(KEY), "{outcome:?}");

    let logs = capture.logs();
    assert!(logs.contains("WARN"), "{logs}");
    assert!(logs.contains("namespace=acct"), "{logs}");
    assert!(logs.contains("code=3"), "{logs}");
    assert!(!logs.contains(KEY), "{logs}");
    assert_eq!(error.code(), 503);
    assert!(!error.message().contains(KEY), "{}", error.message());
    let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
    assert_eq!(body["code"], "credentials_rejected");
}
