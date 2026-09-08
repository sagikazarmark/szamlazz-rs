//! Discovery and binding tests of the Restate adapters: the service names,
//! the handler set with its shared flags and the per-handler retry policy,
//! plus an `Endpoint` build; and the fault → `TerminalError` mapping the
//! handlers share, and the sentinels that the agent key reaches neither the
//! `credentials_rejected` warning nor its fault body.

use restate_sdk::discovery::{HandlerType, RetryPolicyOnMaxAttempts, ServiceType};
use restate_sdk::endpoint::Endpoint;
use restate_sdk::service::Discoverable;
use serde_json::json;

use super::{Agent, Order};
use crate::account::{Accounts, ResolveError, StaticConfig, StaticResolver};
use crate::config::{IssueConfig, ValidatedWorkerConfig, WorkerConfig};
use crate::gateway::SzamlazzAnswer;
use crate::identity::{ExternalId, Namespace};
use crate::test_support::{Doc, ORIGINAL_TELJ, open_gateway};

/// [`IssueConfig::MIN_INITIAL_DELAY`] in the unit discovery reports
/// (milliseconds): the floor the write handlers' `initial_interval` clears.
#[allow(clippy::cast_possible_truncation)]
const MIN_INITIAL_DELAY_MS: u64 = IssueConfig::MIN_INITIAL_DELAY.as_millis() as u64;

/// The `inactivity_timeout` / `abort_timeout` of every handler whose step is
/// one szamlazz.hu round trip (the four reads and `set_payments`' one send)
/// in the discovery reports (milliseconds): `2m`, the 60 s client timeout
/// plus the margin a stalling szamlazz.hu needs (#114). The writes whose step
/// is three trips carry `4m` / `3m`. A literal, like the attributes it pins
/// (the handler macro takes no constant).
const ONE_TRIP_TIMEOUT_MS: u64 = 120_000;

/// The `Accounts` bundle of a test account at `endpoint` with `agent_key`,
/// through the static resolver: what the endpoint binary builds.
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
            // Read-only: shared, an empty input, the default back-off with
            // three attempts, no idempotency retention; an explicit journal
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
            assert_eq!(handler.retry_policy_initial_interval, None);
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
            "set_payments",
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
            // Read-only: a short 10s → 1m back-off, three attempts, no
            // idempotency retention (nothing to replay); an explicit journal
            // retention so the journal is inspectable, and, for the probe,
            // so the leak assertion can scan it. The reads' 2m / 2m timeouts
            // (#114): one 60 s round trip plus the margin a stalling
            // szamlazz.hu needs, the same rule as `set_payments`' one send.
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
            // `set_payments` because an additive send is at-least-once,
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
                assert_eq!(name, "set_payments");
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
    use crate::contract::{CreateRequest, DeleteProformaRequest, SetPaymentsRequest};

    /// What the SDK hands the handler for `bytes`: the decode never fails.
    fn body<T: for<'de> serde::Deserialize<'de>>(bytes: impl Into<Bytes>) -> Body<T> {
        Body::<T>::deserialize(&mut bytes.into()).expect("never fails")
    }

    /// The message of the `invalid_input` fault (400) `bytes` is refused with.
    fn refused<T>(bytes: impl Into<Bytes>) -> String
    where
        T: for<'de> serde::Deserialize<'de> + std::fmt::Debug,
    {
        let error = TerminalError::from(body::<T>(bytes).into_request().expect_err("refused"));
        assert_eq!(error.code(), 400);
        let fault: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
        assert_eq!(fault["code"], "invalid_input");
        assert_eq!(fault.get("order"), None);
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
    let message = refused::<SetPaymentsRequest>(json!({"invoice_number": "SZ-1"}).to_string());
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
        SetPaymentsRequest, StornoRequest,
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
        SetPaymentsRequest,
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

#[test]
fn faults_serialise_their_code_and_status() {
    use restate_sdk::errors::TerminalError;

    use super::support::Fault;
    use crate::contract::{IssuedKind, TerminalCode};
    use crate::identity::OrderKey;

    let order = OrderKey::parse("ORD-1").expect("order");
    let fault = Fault::outcome_unknown("exhausted").about(
        &order,
        Some(IssuedKind::Invoice),
        &ExternalId::new("acct:ORD-1:invoice"),
    );
    let error = TerminalError::from(fault);
    assert_eq!(error.code(), 500);
    let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
    assert_eq!(body["code"], TerminalCode::OutcomeUnknown.as_str());
    assert_eq!(body["order"], "ORD-1");
    assert_eq!(body["kind"], "invoice");
    assert_eq!(body["external_id"], "acct:ORD-1:invoice");
    assert_eq!(body.get("gen"), None);
    assert_eq!(body.get("request_id"), None);

    let cases = [
        (Fault::invalid_input("x"), 400, "invalid_input"),
        (Fault::unavailable("x"), 503, "unavailable"),
        (Fault::missing_fulfillment_date("SZ-1"), 503, "unavailable"),
        (
            Fault::credentials_rejected(&namespace(), SzamlazzAnswer::new("3", "x")),
            503,
            "credentials_rejected",
        ),
        (Fault::unknown_account("x"), 400, "unknown_account"),
        (Fault::not_found("x"), 404, "not_found"),
        (
            Fault::szamlazz_error(SzamlazzAnswer::new("152", "x")),
            422,
            "szamlazz_error",
        ),
    ];
    for (fault, status, code) in cases {
        let error = TerminalError::from(fault);
        assert_eq!(error.code(), status);
        let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
        assert_eq!(body["code"], code);
        assert_eq!(body.get("order"), None);
    }
}

/// A szamlazz.hu code never travels in `code` (that field carries a
/// `TerminalCode` token), but in `szamlazz_code`, beside it: on the 422
/// pass-through, whose message is szamlazz.hu's own; on a credential
/// rejection; on an inconclusive answer to a read. Faults that no szamlazz.hu
/// answer caused carry no `szamlazz_code` at all.
#[test]
fn a_szamlazz_code_travels_in_its_own_field() {
    use restate_sdk::errors::TerminalError;

    use super::support::Fault;

    let error = TerminalError::from(Fault::szamlazz_error(SzamlazzAnswer::new(
        "152",
        "Már létezik ilyen rendelésszámú számla.",
    )));
    assert_eq!(error.code(), 422);
    let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
    assert_eq!(body["code"], "szamlazz_error");
    assert_eq!(body["szamlazz_code"], "152");
    let message = body["message"].as_str().expect("message");
    assert!(message.contains("152"), "{message}");
    assert!(
        message.contains("Már létezik ilyen rendelésszámú számla."),
        "{message}"
    );

    let error = TerminalError::from(Fault::credentials_rejected(
        &namespace(),
        SzamlazzAnswer::new("3", "x"),
    ));
    let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
    assert_eq!(body["code"], "credentials_rejected");
    assert_eq!(body["szamlazz_code"], "3");

    let error = TerminalError::from(Fault::inconclusive_answer(SzamlazzAnswer::new("57", "x")));
    let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
    assert_eq!(body["code"], "unavailable");
    assert_eq!(body["szamlazz_code"], "57");

    for fault in [
        Fault::invalid_input("x"),
        Fault::not_found("x"),
        Fault::unavailable("x"),
        Fault::szlahu_down_answer("x"),
        Fault::outcome_unknown("x"),
        Fault::unknown_account("x"),
        Fault::missing_fulfillment_date("SZ-1"),
    ] {
        let error = TerminalError::from(fault);
        let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
        assert_eq!(body.get("szamlazz_code"), None, "{body}");
    }
}

/// The fault a credential rejection raises names the szamlazz.hu code, tells
/// the caller the outcome is not known (never that "this attempt issued
/// nothing", which a post-send re-query can make false and which uses a word
/// the glossary avoids for a handler execution, #63), and carries the
/// document identity when one is attached.
#[test]
fn credentials_rejected_fault_names_the_code_and_the_document() {
    use restate_sdk::errors::TerminalError;

    use super::support::Fault;
    use crate::contract::IssuedKind;
    use crate::identity::OrderKey;

    let order = OrderKey::parse("ORD-1").expect("order");
    let fault = Fault::credentials_rejected(
        &namespace(),
        SzamlazzAnswer::new("136", "Bejelentkezés letiltva"),
    )
    .about(
        &order,
        Some(IssuedKind::Invoice),
        &ExternalId::new("acct:ORD-1:invoice"),
    );
    let error = TerminalError::from(fault);
    assert_eq!(error.code(), 503);
    let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
    assert_eq!(body["code"], "credentials_rejected");
    assert_eq!(body["order"], "ORD-1");
    assert_eq!(body["kind"], "invoice");
    assert_eq!(body["external_id"], "acct:ORD-1:invoice");
    let message = body["message"].as_str().expect("message");
    assert!(message.contains("136"), "{message}");
    assert!(message.contains("Bejelentkezés letiltva"), "{message}");
    assert!(message.contains("fix the account's agent key"), "{message}");
    assert!(
        message.contains("retry with a new Idempotency-Key"),
        "{message}"
    );
    assert!(!message.contains("attempt"), "{message}");
    assert!(!message.contains("issued nothing"), "{message}");
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

    use super::support::Fault;
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
    drop(Fault::credentials_rejected(
        &"warmup".parse().expect("namespace"),
        SzamlazzAnswer::new("0", "warm-up"),
    ));
    LogCapture::rebuild_interest();

    // What the prologue does: resolve, fetch, open; then the gateway
    // observes the code and the fault is built.
    let account = order.accounts().resolve(None).await.expect("account");
    let credentials = order.accounts().fetch(&account).await.expect("credentials");
    let gateway = open_gateway(account, credentials);
    let outcome = gateway.verify("SZ-1").await;
    let Ok(QueryOutcome::CredentialsRejected(answer)) = outcome.clone() else {
        panic!("expected CredentialsRejected, got {outcome:?}");
    };
    assert_eq!(answer.code, "3");
    let error = TerminalError::from(Fault::credentials_rejected(
        &order.config().namespace,
        answer,
    ));
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

/// Every handler execution runs inside one span (`execution`) carrying the
/// scope, the order key, the invocation id and, once the prologue has resolved
/// it, the account id (#65). Every log line under it is thereby attributable
/// to an account in a multi-account deployment: the paging
/// `credentials_rejected` warning (whose own fields stay the namespace and
/// the code), and the events inside a gateway step's span alike.
#[tokio::test]
async fn the_execution_span_attributes_every_log_line_under_it() {
    use tracing::Instrument as _;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::prologue::{execution_span, record_account};
    use super::support::Fault;
    use crate::contract::{DocumentKind, IssuedKind};
    use crate::gateway::{LookupOutcome, LookupRequest};
    use crate::identity::{ExternalId, OrderKey};
    use crate::test_support::LogCapture;

    let server = MockServer::start().await;
    // Another code: the lookup warns about it inside its own span.
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod>57</hibakod><hibauzenet>Hibás számlaszám.</hibauzenet></xmlszamlavalasz>"#,
            "application/xml",
        ))
        .expect(1)
        .mount(&server)
        .await;
    let order = Order::from_parts(accounts(&server.uri(), "key"), worker_config());

    let capture = LogCapture::default();
    let guard = capture.subscribe();
    // Pin the span's and the warning's callsites to this thread's subscriber
    // (see `LogCapture`); the warm-up is told apart by its scope.
    {
        let span = execution_span(Some("warmup"), None, "inv_warmup");
        let _entered = span.enter();
        drop(Fault::credentials_rejected(
            &"warmup".parse().expect("namespace"),
            SzamlazzAnswer::new("0", "warm-up"),
        ));
    }
    LogCapture::rebuild_interest();

    let account = order.accounts().resolve(None).await.expect("account");
    let credentials = order.accounts().fetch(&account).await.expect("credentials");
    let gateway = open_gateway(account, credentials);
    let order_key = OrderKey::parse("ORD-1").expect("order key");
    let external_id = ExternalId::for_kind(&namespace(), &order_key, DocumentKind::Invoice);
    async {
        // What the prologue does once the `account` step has answered.
        record_account(gateway.account());
        drop(Fault::credentials_rejected(
            &namespace(),
            SzamlazzAnswer::new("3", "Sikertelen bejelentkezés."),
        ));
        let outcome = gateway
            .lookup(LookupRequest {
                external_id: &external_id,
                kind: IssuedKind::Invoice,
                order: &order_key,
                our_numbers: &[],
            })
            .await;
        assert!(matches!(outcome, Ok(LookupOutcome::Api(_))), "{outcome:?}");
    }
    .instrument(execution_span(
        Some("acme-events"),
        Some("ORD-1"),
        "inv_1abc",
    ))
    .await;
    drop(guard);

    let logs = capture.logs();
    let attributed = |line: &str| {
        line.contains("execution{")
            && line.contains("scope=acme-events")
            && line.contains("order=ORD-1")
            && line.contains("restate.invocation.id=inv_1abc")
            && line.contains("account.id=acct")
    };
    let warning = logs
        .lines()
        .find(|line| line.contains("rejected the agent credentials") && !line.contains("warmup"))
        .unwrap_or_else(|| panic!("the credentials_rejected warning: {logs}"));
    assert!(warning.contains("WARN"), "{warning}");
    assert!(attributed(warning), "{warning}");
    assert!(warning.contains("namespace=acct"), "{warning}");
    assert!(warning.contains("code=3"), "{warning}");
    let step = logs
        .lines()
        .find(|line| line.contains("gateway.lookup{"))
        .unwrap_or_else(|| panic!("an event inside the gateway step's span: {logs}"));
    assert!(attributed(step), "{step}");
    assert!(
        step.contains("external_id=acct:ORD-1:invoice"),
        "the step's own fields stay: {step}"
    );
}

#[test]
fn lookup_classifies_query_outcomes() {
    use super::support::Lookup;
    use crate::contract::IssuedKind;
    use crate::gateway::QueryOutcome;
    use crate::identity::OrderKey;

    let order = OrderKey::parse("ORD-1").expect("order");
    let namespace = namespace();
    let classify =
        |outcome: QueryOutcome| Lookup::classify(outcome, &namespace, &order, IssuedKind::Invoice);

    assert_eq!(
        classify(QueryOutcome::NotFound).expect("classified"),
        Lookup::Absent
    );
    let ours = Doc::default().boxed();
    let lookup = classify(QueryOutcome::Found(ours.clone())).expect("classified");
    assert_eq!(lookup, Lookup::Ours(ours));

    let reversed = Doc {
        reversed: true,
        ..Doc::default()
    }
    .boxed();
    let lookup = classify(QueryOutcome::Found(reversed.clone())).expect("classified");
    assert_eq!(lookup, Lookup::Ours(reversed));

    // Each identity of ours off by one: another order or kind.
    let doc = |edit: fn(&mut Doc<'static>)| {
        let mut doc = Doc::default();
        edit(&mut doc);
        doc.boxed()
    };
    for (label, other) in [
        ("order", doc(|doc| doc.order = Some("ORD-2"))),
        ("kind", doc(|doc| doc.tipus = "D")),
    ] {
        let lookup = classify(QueryOutcome::Found(other.clone())).expect(label);
        assert_eq!(lookup, Lookup::Collision(other), "{label}");
    }
    // No account pin: neither `teszt` nor the seller record's id
    // (`szallito/id`) is compared with anything; a document of this order and
    // kind is ours whatever they say, and whether they say anything (an
    // absent `<teszt>` is `None` since #70).
    for (label, other) in [
        ("teszt", doc(|doc| doc.test = Some(false))),
        ("no teszt", doc(|doc| doc.test = None)),
        ("szallito/id", doc(|doc| doc.supplier_id = 1)),
    ] {
        let lookup = classify(QueryOutcome::Found(other.clone())).expect(label);
        assert_eq!(lookup, Lookup::Ours(other), "{label}");
    }
    // Another szamlazz.hu code is an answer the handler cannot conclude from:
    // the `unavailable` fault naming the code, as before the read policy.
    let fault = classify(QueryOutcome::Api(SzamlazzAnswer::new(
        "57",
        "Ismeretlen hiba",
    )))
    .expect_err("a fault");
    let error = restate_sdk::errors::TerminalError::from(fault);
    assert_eq!(error.code(), 503);
    let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
    assert_eq!(body["code"], "unavailable");
    let message = body["message"].as_str().expect("message");
    assert!(message.contains("57"), "{message}");
    assert!(message.contains("Ismeretlen hiba"), "{message}");

    // Rejected credentials are a fault of their own, not `unavailable`.
    let fault = classify(QueryOutcome::CredentialsRejected(SzamlazzAnswer::new(
        "3", "login",
    )))
    .expect_err("a fault");
    let error = restate_sdk::errors::TerminalError::from(fault);
    assert_eq!(error.code(), 503);
    let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
    assert_eq!(body["code"], "credentials_rejected");
}

/// The settled storno step as the response, for the two answers the leading
/// query can settle it with before anything is sent (#63): another code is
/// `unavailable` naming it (the shape the storno lookup gives the same code),
/// and `szlahu_down` is `unavailable` without a `szamlazz_code`. Both
/// services share this mapping.
#[test]
fn a_settled_storno_step_maps_its_leading_query_answers_onto_faults() {
    use restate_sdk::errors::TerminalError;

    use super::support::storno_response;
    use crate::gateway::StornoOutcome;

    let respond =
        |outcome: StornoOutcome| storno_response(outcome, "SZ-1".to_owned(), &namespace());

    let response = respond(StornoOutcome::AlreadyReversed {
        storno_number: "SS-1".to_owned(),
    })
    .expect("data");
    assert_eq!(response.storno_number.as_deref(), Some("SS-1"));

    let fault =
        respond(StornoOutcome::Api(SzamlazzAnswer::new("57", "Hibás XML."))).expect_err("a fault");
    let error = TerminalError::from(fault);
    assert_eq!(error.code(), 503);
    let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
    assert_eq!(body["code"], "unavailable");
    assert_eq!(body["szamlazz_code"], "57");
    let message = body["message"].as_str().expect("message");
    assert!(message.contains("code 57"), "{message}");

    let fault = respond(StornoOutcome::Unavailable {
        message: "maintenance".to_owned(),
    })
    .expect_err("a fault");
    let error = TerminalError::from(fault);
    assert_eq!(error.code(), 503);
    let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
    assert_eq!(body["code"], "unavailable");
    assert_eq!(body.get("szamlazz_code"), None, "{body}");
    let message = body["message"].as_str().expect("message");
    assert!(message.contains("szlahu_down"), "{message}");
    assert!(message.contains("nothing was sent"), "{message}");

    let fault = respond(StornoOutcome::CredentialsRejected(SzamlazzAnswer::new(
        "3", "login",
    )))
    .expect_err("a fault");
    let error = TerminalError::from(fault);
    let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
    assert_eq!(body["code"], "credentials_rejected");
}

/// A read step that ended without an answer (the read policy exhausted
/// (500 carrying the last `Unanswered`) or the invocation cancelled (409))
/// is the `unavailable` fault naming the step and the last failure, about
/// the document when the caller attaches one.
#[test]
fn an_exhausted_read_is_a_structured_unavailable() {
    use restate_sdk::errors::TerminalError;

    use super::support::read_exhausted;
    use crate::contract::IssuedKind;
    use crate::identity::OrderKey;

    let last = TerminalError::new_with_code(
        500,
        "transport failure: error decoding response body: empty response",
    );
    let fault = read_exhausted("lookup-invoice", &last);
    let error = TerminalError::from(fault.clone());
    assert_eq!(error.code(), 503);
    let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
    assert_eq!(body["code"], "unavailable");
    let message = body["message"].as_str().expect("message");
    assert!(message.contains("lookup-invoice"), "{message}");
    assert!(
        message.contains("empty response"),
        "names the last failure: {message}"
    );
    assert!(message.contains("500"), "{message}");
    assert!(message.contains("Idempotency-Key"), "{message}");
    assert_eq!(body.get("order"), None, "nothing attached yet");

    let order = OrderKey::parse("ORD-1").expect("order");
    let about = fault.about(
        &order,
        Some(IssuedKind::Invoice),
        &ExternalId::new("acct:ORD-1:invoice"),
    );
    let error = TerminalError::from(about);
    let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
    assert_eq!(body["order"], "ORD-1");
    assert_eq!(body["kind"], "invoice");
    assert_eq!(body["external_id"], "acct:ORD-1:invoice");

    let cancelled = TerminalError::new_with_code(409, "cancelled");
    let error = TerminalError::from(read_exhausted("get-proforma", &cancelled));
    assert_eq!(error.code(), 503, "a cancellation is the same fault");
    assert!(error.message().contains("409"), "{}", error.message());
}

/// The best-effort reads (the storno-number hint after a verify found the
/// document already reversed, and `Szamlazz.Agent.storno`'s storno lookup in
/// the same situation) swallow an exhausted read policy: the handler's
/// answer (`reversed`) is already known, so the number is reported as unknown
/// after a `warn` naming the step. They never swallow a cancellation: the SDK
/// ends a cancelled run with 409, and an invocation told to stop must not
/// answer `reversed` as if nothing had happened (#65); the error is
/// propagated as it came.
#[test]
fn a_best_effort_read_swallows_exhaustion_but_propagates_a_cancellation() {
    use restate_sdk::errors::TerminalError;

    use super::support::best_effort;
    use crate::test_support::LogCapture;

    let capture = LogCapture::default();
    let guard = capture.subscribe();
    drop(best_effort(
        "hint-storno-warmup",
        TerminalError::new_with_code(500, "warm-up"),
    ));
    LogCapture::rebuild_interest();

    let exhausted = TerminalError::new_with_code(500, "szamlazz.hu is unavailable: maintenance");
    best_effort("hint-storno-SZ-1", exhausted).expect("exhaustion is swallowed");

    let cancelled = TerminalError::new_with_code(409, "cancelled");
    let error = best_effort("hint-storno-SZ-1", cancelled).expect_err("a cancellation propagates");
    assert_eq!(error.code(), 409);
    assert_eq!(error.message(), "cancelled");
    drop(guard);

    let logs = capture.logs();
    let warnings: Vec<&str> = logs
        .lines()
        .filter(|line| line.contains("WARN") && !line.contains("warmup"))
        .collect();
    assert_eq!(
        warnings.len(),
        1,
        "the swallowed exhaustion warns, the cancellation does not: {logs}"
    );
    assert!(warnings[0].contains("hint-storno-SZ-1"), "{}", warnings[0]);
    assert!(warnings[0].contains("maintenance"), "{}", warnings[0]);
}

/// What the two best-effort reads make of an answer, for a document the
/// verify already saw reversed: `Szamlazz.Order.storno_invoice`'s order-number
/// hint names the storno when it is the `SS` referencing the invoice; any
/// other document under the order, nothing, or another code is unknown;
/// `Szamlazz.Agent.storno`'s by-number storno lookup names it when a storno of
/// ours holds the id (#65); nothing under it (a reversal from the UI) or
/// another code is unknown. Rejected credentials stay the fault on both.
#[test]
fn the_best_effort_reads_name_the_storno_only_from_its_own_document() {
    use restate_sdk::errors::TerminalError;

    use super::support::{storno_number_from_hint, storno_number_from_lookup};
    use crate::gateway::{QueryOutcome, StornoLookupOutcome};

    let namespace = namespace();
    let rejected_body = |error: TerminalError| {
        assert_eq!(error.code(), 503);
        let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
        assert_eq!(body["code"], "credentials_rejected", "{body}");
        assert_eq!(body["szamlazz_code"], "3", "{body}");
    };
    let rejected =
        || QueryOutcome::CredentialsRejected(SzamlazzAnswer::new("3", "Sikertelen bejelentkezés."));

    let storno = Doc {
        referenced_invoice: Some("SZ-1"),
        ..Doc::new("SS-1", "SS")
    };
    assert_eq!(
        storno_number_from_hint(QueryOutcome::Found(storno.boxed()), "SZ-1", &namespace)
            .expect("data"),
        Some("SS-1".to_owned())
    );
    for not_its_storno in [
        // The reversed invoice itself is the newest document under the order.
        QueryOutcome::Found(Doc::default().boxed()),
        // Another invoice's storno.
        QueryOutcome::Found(
            Doc {
                referenced_invoice: Some("SZ-9"),
                ..Doc::new("SS-9", "SS")
            }
            .boxed(),
        ),
        QueryOutcome::NotFound,
        QueryOutcome::Api(SzamlazzAnswer::new("57", "Hibás számlaszám.")),
    ] {
        assert_eq!(
            storno_number_from_hint(not_its_storno.clone(), "SZ-1", &namespace).expect("data"),
            None,
            "{not_its_storno:?}"
        );
    }
    rejected_body(TerminalError::from(
        storno_number_from_hint(rejected(), "SZ-1", &namespace).expect_err("a fault"),
    ));

    assert_eq!(
        storno_number_from_lookup(
            StornoLookupOutcome::AlreadyReversed {
                storno_number: "SS-1".to_owned(),
            },
            &namespace,
        )
        .expect("data"),
        Some("SS-1".to_owned())
    );
    for unknown in [
        StornoLookupOutcome::Absent,
        StornoLookupOutcome::Api(SzamlazzAnswer::new("57", "Hibás számlaszám.")),
    ] {
        assert_eq!(
            storno_number_from_lookup(unknown.clone(), &namespace).expect("data"),
            None,
            "{unknown:?}"
        );
    }
    let QueryOutcome::CredentialsRejected(answer) = rejected() else {
        unreachable!()
    };
    rejected_body(TerminalError::from(
        storno_number_from_lookup(StornoLookupOutcome::CredentialsRejected(answer), &namespace)
            .expect_err("a fault"),
    ));
}

/// The Virtual Object key must arrive trimmed: Restate's per-key lock is on
/// the *raw* key, so `ORD-1` and ` ORD-1` would be two instances with two
/// locks mapping to one szamlazz.hu order and identical external ids; two
/// concurrent creates under them would both pass their lookup and both send.
/// The handler refuses a key whose trimmed form differs from the raw one as
/// `invalid_input` naming the rule; [`OrderKey::parse`] itself stays lenient
/// for the places that parse an order number rather than a key.
#[test]
fn the_order_key_must_arrive_trimmed() {
    use restate_sdk::errors::TerminalError;

    use super::support::order_key;
    use crate::identity::OrderKey;

    let key = order_key("ORD-1").expect("a trimmed key");
    assert_eq!(key.as_str(), "ORD-1");
    let key = order_key("rendelés-42").expect("non-ASCII text in NFC is fine");
    assert_eq!(key.as_str(), "rendelés-42");

    for raw in [" ORD-1", "ORD-1 ", "\tORD-1", "ORD-1\n", "\u{a0}ORD-1"] {
        assert_eq!(
            OrderKey::parse(raw).expect("the type trims").as_str(),
            "ORD-1",
            "{raw:?}: OrderKey::parse stays lenient"
        );
        let fault = order_key(raw).expect_err("refused");
        let error = TerminalError::from(fault);
        assert_eq!(error.code(), 400, "{raw:?}");
        let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
        assert_eq!(body["code"], "invalid_input", "{raw:?}");
        let message = body["message"].as_str().expect("message");
        assert!(
            message.contains("must not have leading or trailing whitespace"),
            "{raw:?}: names the rule: {message}"
        );
        assert_eq!(body.get("order"), None, "{raw:?}: no order identity yet");
    }

    // The type's own alphabet still applies to a trimmed key, with its
    // message naming the rule: no internal whitespace, no `:`, NFC, 40 bytes.
    let too_long = "x".repeat(OrderKey::MAX_LEN + 1);
    for (raw, rule) in [
        ("rendelés #42", "must not contain whitespace"),
        ("a\u{a0}b", "must not contain whitespace"),
        ("ORD:1", "must not contain ':'"),
        ("rendele\u{301}s-42", "must be in Unicode NFC"),
        (too_long.as_str(), "at most 40 are allowed"),
    ] {
        let fault = order_key(raw).expect_err(rule);
        let error = TerminalError::from(fault);
        assert_eq!(error.code(), 400, "{raw:?}");
        let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
        assert_eq!(body["code"], "invalid_input", "{raw:?}");
        assert!(
            body["message"].as_str().expect("message").contains(rule),
            "{raw:?}: names the rule: {body}"
        );
        assert_eq!(body.get("order"), None, "{raw:?}: no order identity yet");
    }
}

/// The storno intent both storno handlers build from the verified original:
/// the storno repeats the original's `telj`, lifts `eszamla` from the
/// document (the appearance cases are
/// [`the_storno_intent_lifts_eszamla_from_the_original_not_the_default`]),
/// and an original szamlazz.hu returned without a `telj` (its schema has the
/// element mandatory) is the `unavailable` fault naming the invoice, never
/// a send without the date or with a default.
#[test]
fn the_storno_intent_repeats_the_originals_fulfillment_date() {
    use restate_sdk::errors::TerminalError;

    use super::support::StornoIntent;
    use crate::account::Account;
    use crate::contract::IssuedKind;
    use crate::identity::{ExternalId, OrderKey};

    let mut account = Account::new("acct", "acct");
    account.defaults.e_invoice = false;
    let storno_id = || ExternalId::new("acct:ORD-1:storno:SZ-1");

    // `telj` present: the intent carries it; `eszamla = 2` is an e-invoice.
    let intent = StornoIntent::from_verified(
        &Doc::default().parse(),
        &account,
        "SZ-1".to_owned(),
        storno_id(),
        Some("wrong buyer".to_owned()),
    )
    .expect("an intent");
    assert_eq!(intent.fulfillment_date, ORIGINAL_TELJ);
    assert_eq!(intent.number, "SZ-1");
    assert_eq!(intent.storno_id, storno_id());
    assert_eq!(intent.comment.as_deref(), Some("wrong buyer"));
    assert!(intent.e_invoice, "lifted from the document");

    // `telj` empty (parsed as absent): the fault, 503 `unavailable`, naming
    // the invoice; `.about(..)` attaches the storno identity as every fault.
    let fault = StornoIntent::from_verified(
        &Doc {
            fulfillment_date: None,
            alap_extra: "<telj></telj>",
            ..Doc::default()
        }
        .parse(),
        &account,
        "SZ-1".to_owned(),
        storno_id(),
        None,
    )
    .expect_err("a fault");
    let error = TerminalError::from(fault.clone());
    assert_eq!(error.code(), 503);
    let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
    assert_eq!(body["code"], "unavailable");
    let message = body["message"].as_str().expect("message");
    assert!(message.contains("SZ-1"), "{message}");
    assert!(message.contains("fulfillment date"), "{message}");
    assert!(message.contains("nothing was sent"), "{message}");
    assert!(message.contains("Idempotency-Key"), "{message}");
    assert_eq!(body.get("order"), None);

    let order = OrderKey::parse("ORD-1").expect("order");
    let about = TerminalError::from(fault.about(
        &order,
        Some(IssuedKind::Invoice),
        &ExternalId::new("acct:ORD-1:storno:SZ-1"),
    ));
    let body: serde_json::Value = serde_json::from_str(about.message()).expect("json body");
    assert_eq!(body["code"], "unavailable");
    assert_eq!(body["order"], "ORD-1");
    assert_eq!(body["kind"], "invoice");
    assert_eq!(body["external_id"], "acct:ORD-1:storno:SZ-1");
}

/// The storno's `eszamla` is the verified original's appearance, and the
/// account default only where the code is not an invoice appearance.
/// szamlazz.hu does not require a storno's form to match its original's: a
/// mismatch is accepted silently and the storno document takes the
/// *request's* flag (P73), so the intent, not the server, keeps a reversal in
/// its original's form: `1` (paper) is `false` under an e-invoice default,
/// `3` (the code szamlazz.hu reports for an invoice created with
/// `eszamla=true`) and `2` are `true` under a paper default, and `0` (a
/// proforma) is whatever the account default says.
#[test]
fn the_storno_intent_lifts_eszamla_from_the_original_not_the_default() {
    use super::support::StornoIntent;
    use crate::account::Account;
    use crate::identity::ExternalId;

    let mut account = Account::new("acct", "acct");
    let intent = |eszamla: i32, account: &Account| {
        StornoIntent::from_verified(
            &Doc {
                eszamla: Some(eszamla),
                ..Doc::default()
            }
            .parse(),
            account,
            "SZ-1".to_owned(),
            ExternalId::new("acct:ORD-1:storno:SZ-1"),
            None,
        )
        .expect("an intent")
        .e_invoice
    };

    account.defaults.e_invoice = true;
    assert!(!intent(1, &account), "paper, whatever the account default");
    assert!(intent(0, &account), "not an invoice: the account default");

    account.defaults.e_invoice = false;
    assert!(
        intent(3, &account),
        "e-invoice, whatever the account default"
    );
    assert!(
        intent(2, &account),
        "e-invoice, whatever the account default"
    );
    assert!(!intent(0, &account), "not an invoice: the account default");
}

/// Step 2 of both storno protocols, decided once for both services: a storno
/// of ours already under the storno external id answers `reversed` with its
/// number before anything is sent; nothing under the id proceeds to the
/// storno step; rejected credentials are the `credentials_rejected` fault and
/// another code the `unavailable` one (nothing may be concluded, nothing was
/// sent), each carrying the szamlazz.hu code beside the token, never in it.
#[test]
fn the_storno_lookup_answers_an_existing_storno_and_faults_on_a_code() {
    use std::ops::ControlFlow;

    use restate_sdk::errors::TerminalError;

    use super::support::after_storno_lookup;
    use crate::contract::StornoOutcome;
    use crate::gateway::StornoLookupOutcome;

    let namespace = namespace();
    let fault_body = |fault: super::support::Fault| {
        let error = TerminalError::from(fault);
        let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
        (error.code(), body)
    };

    assert_eq!(
        after_storno_lookup(StornoLookupOutcome::Absent, "SZ-1", &namespace).expect("data"),
        ControlFlow::Continue(()),
        "nothing under the id: on to the storno step"
    );

    let ControlFlow::Break(response) = after_storno_lookup(
        StornoLookupOutcome::AlreadyReversed {
            storno_number: "SS-1".to_owned(),
        },
        "SZ-1",
        &namespace,
    )
    .expect("data") else {
        panic!("a storno of ours under the id is the answer");
    };
    assert_eq!(response.outcome, StornoOutcome::Reversed);
    assert_eq!(response.invoice_number, "SZ-1");
    assert_eq!(response.storno_number.as_deref(), Some("SS-1"));
    assert_eq!(response.conflict_reason, None);

    let (status, body) = fault_body(
        after_storno_lookup(
            StornoLookupOutcome::CredentialsRejected(SzamlazzAnswer::new(
                "3",
                "Sikertelen bejelentkezés.",
            )),
            "SZ-1",
            &namespace,
        )
        .expect_err("a fault"),
    );
    assert_eq!(status, 503, "{body}");
    assert_eq!(body["code"], "credentials_rejected", "{body}");
    assert_eq!(body["szamlazz_code"], "3", "{body}");

    let (status, body) = fault_body(
        after_storno_lookup(
            StornoLookupOutcome::Api(SzamlazzAnswer::new("57", "Hibás számlaszám.")),
            "SZ-1",
            &namespace,
        )
        .expect_err("a fault"),
    );
    assert_eq!(status, 503, "{body}");
    assert_eq!(body["code"], "unavailable", "{body}");
    assert_eq!(body["szamlazz_code"], "57", "{body}");
    let message = body["message"].as_str().expect("message");
    assert!(message.contains("code 57"), "{message}");
    assert!(message.contains("Hibás számlaszám."), "{message}");
    assert!(message.contains("nothing may be concluded"), "{message}");
}

/// The `reversed` answer both storno handlers give once a verify found the
/// document already reversed: the storno number as the best-effort read
/// named it, or absent when that read could not name one.
#[test]
fn the_reversed_answer_carries_the_storno_number_when_known() {
    use super::support::reversed_response;
    use crate::contract::StornoOutcome;

    let known = reversed_response("SZ-1", Some("SS-1".to_owned()));
    assert_eq!(known.outcome, StornoOutcome::Reversed);
    assert_eq!(known.invoice_number, "SZ-1");
    assert_eq!(known.storno_number.as_deref(), Some("SS-1"));

    let unknown = reversed_response("SZ-1", None);
    assert_eq!(unknown.outcome, StornoOutcome::Reversed);
    assert_eq!(unknown.invoice_number, "SZ-1");
    assert_eq!(unknown.storno_number, None);
    assert_eq!(unknown.conflict_reason, None);
    assert_eq!(unknown.code, None);
}
