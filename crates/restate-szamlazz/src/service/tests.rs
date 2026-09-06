//! Discovery and binding tests of the Restate adapters (design §11): the
//! service names, the handler set with its shared flags and the per-handler
//! retry policy, plus an `Endpoint` build — and the fault → `TerminalError`
//! mapping the handlers share, the account pins of a found document, and the
//! sentinels that the agent key reaches neither the `credentials_rejected`
//! warning nor the fault body of `credentials_rejected` or `account_mismatch`.

use restate_sdk::discovery::{HandlerType, RetryPolicyOnMaxAttempts, ServiceType};
use restate_sdk::endpoint::Endpoint;
use restate_sdk::service::Discoverable;
use serde_json::json;
use szamlazz_agent::ops::query_xml::InvoiceDocument;

use super::{Agent, Order};
use crate::account::{Accounts, ResolveError, StaticConfig, StaticResolver};
use crate::config::{Namespace, WorkerConfig};
use crate::gateway::Gateway;

/// The `Accounts` bundle of a test account at `endpoint` with `agent_key`,
/// through the static resolver — what the endpoint binary builds.
fn accounts(endpoint: &str, agent_key: &str) -> Accounts {
    let config: StaticConfig = serde_json::from_value(json!({
        "account": {
            "id": "acct",
            "agent_key": agent_key,
            "endpoint": endpoint,
            "mode": "test",
        },
    }))
    .expect("config");
    Accounts::from(StaticResolver::try_from(config).expect("resolver"))
}

fn namespace() -> Namespace {
    "acct".parse().expect("namespace")
}

/// The supplier id of the documents [`found`] builds.
pub(super) const SUPPLIER: u64 = 972_720;

/// szamlazz.hu's `<szamla>` XML of a live `SZ-1` of `ORD-1` from a test
/// account with `supplier_id`, with the given `alap` elements overridden.
fn szamla_xml(supplier_id: u64, alap_overrides: &[(&str, &str)]) -> String {
    let mut alap = vec![
        ("szamlaszam", "SZ-1"),
        ("tipus", "SZ"),
        ("eszamla", "2"),
        ("rendelesszam", "ORD-1"),
        ("teszt", "true"),
    ];
    for &(tag, value) in alap_overrides {
        match alap.iter_mut().find(|(name, _)| *name == tag) {
            Some(slot) => slot.1 = value,
            None => alap.push((tag, value)),
        }
    }
    let alap = alap.iter().fold(String::new(), |mut xml, (tag, value)| {
        use std::fmt::Write as _;
        write!(xml, "<{tag}>{value}</{tag}>").expect("writing to a String cannot fail");
        xml
    });
    format!(
        r#"<szamla xmlns="http://www.szamlazz.hu/szamla">
          <szallito><id>{supplier_id}</id><nev>Seller</nev><cim><irsz>1111</irsz><telepules>Budapest</telepules><cim>Fő u. 1.</cim></cim></szallito>
          <alap><id>1</id>{alap}</alap>
          <vevo><nev>Buyer</nev></vevo><tetelek></tetelek>
          <osszegek><totalossz><netto>0</netto><afa>0</afa><brutto>0</brutto></totalossz></osszegek>
          </szamla>"#
    )
}

/// The document of [`szamla_xml`], parsed as a query answer.
pub(super) fn found(supplier_id: u64, alap_overrides: &[(&str, &str)]) -> Box<InvoiceDocument> {
    use szamlazz_agent::InvoiceNumber;
    use szamlazz_agent::ops::query_pdf::InvoiceSelector;
    use szamlazz_agent::ops::query_xml::QueryInvoiceXml;
    use szamlazz_agent::wire::{AgentRequest as _, RawResponse};

    Box::new(
        QueryInvoiceXml::new(InvoiceSelector::InvoiceNumber(InvoiceNumber::new("SZ-1")))
            .parse(&RawResponse::new::<&str, &str>(
                [],
                szamla_xml(supplier_id, alap_overrides).into_bytes(),
            ))
            .expect("parse"),
    )
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
            // retention so the journal is inspectable.
            assert_eq!(handler.ty, Some(HandlerType::Shared));
            let input = handler.input.as_ref().expect("an empty input payload");
            assert!(
                input.content_type.is_none() && input.json_schema.is_none(),
                "get takes no input"
            );
            assert_eq!(handler.retry_policy_max_attempts, Some(3));
            assert_eq!(handler.retry_policy_initial_interval, None);
            assert_eq!(handler.inactivity_timeout, None);
            assert_eq!(handler.abort_timeout, None);
            assert_eq!(handler.journal_retention, Some(24 * 3_600_000));
            assert_eq!(handler.idempotency_retention, None);
            continue;
        }
        // Exclusive is the Virtual Object default and left implicit (`None`).
        assert_eq!(handler.ty, None, "{name}");
        assert!(handler.input.is_some(), "{name} takes an input");
        // ADR 0004: every handler that calls szamlazz.hu kills after 5
        // attempts with a 2m → 10m back-off and bounded timeouts.
        assert_eq!(
            handler.retry_policy_initial_interval,
            Some(120_000),
            "{name}"
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
fn agent_discovers_as_a_service_with_four_handlers() {
    let discovery = <Agent as Discoverable>::discover();
    assert_eq!(discovery.name.as_str(), "Szamlazz.Agent");
    assert_eq!(discovery.ty, ServiceType::Service);

    let mut names: Vec<_> = discovery
        .handlers
        .iter()
        .map(|handler| handler.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["check_account", "query", "set_payments", "storno"]);

    for handler in &discovery.handlers {
        let name = handler.name.as_str();
        assert_eq!(handler.ingress_private, None, "{name} is public");
        assert_eq!(
            handler.retry_policy_on_max_attempts,
            Some(RetryPolicyOnMaxAttempts::Kill),
            "{name}"
        );
        if name == "query" || name == "check_account" {
            // Read-only: a short 10s → 1m back-off, three attempts, no
            // idempotency retention (nothing to replay); an explicit journal
            // retention so the journal is inspectable — and, for the probe,
            // so the leak assertion can scan it.
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
            assert_eq!(handler.inactivity_timeout, None, "{name}");
            assert_eq!(handler.abort_timeout, None, "{name}");
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
            assert_eq!(handler.retry_policy_max_attempts, Some(2), "{name}");
            assert_eq!(handler.inactivity_timeout, Some(120_000), "{name}");
            assert_eq!(handler.abort_timeout, Some(120_000), "{name}");
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
}

/// Both services hold the same accounts and the same deployment-level
/// settings, and nothing else — no gateway, no client.
#[tokio::test]
async fn services_bind_to_an_endpoint() {
    let worker = WorkerConfig::new(namespace());
    let order = Order::from_parts(accounts("http://127.0.0.1:1/", "key"), worker.clone());
    let agent = Agent::from_parts(order.accounts().clone(), order.config().clone());
    assert_eq!(order.config(), agent.config());
    assert_eq!(*order.config(), worker);
    assert_eq!(order.config().namespace.as_str(), "acct");
    // The static resolver: the single account, unscoped, with the inline key.
    let account = order.accounts().resolve(None).await.expect("account");
    assert_eq!(account.id.as_str(), "acct");
    assert!(account.mode.is_test());
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

/// A handler's body is decoded by the handler, not the SDK: `Body<T>`'s SDK
/// `Deserialize` never fails — it keeps the verdict — so a malformed body
/// reaches the handler and leaves it as the structured `invalid_input` fault
/// (400, `{code, message}`) with serde's message, naming the field when there
/// is one — never the SDK's plain-text `Cannot decode input payload`.
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
        CorrectRequest, CreateRequest, DeleteProformaRequest, QueryRequest, SetPaymentsRequest,
        StornoRequest,
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
        "acct:ORD-1:invoice",
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
        (Fault::account_mismatch("x"), 409, "account_mismatch"),
        (Fault::unavailable("x"), 503, "unavailable"),
        (
            Fault::credentials_rejected(&namespace(), "3", "x"),
            503,
            "credentials_rejected",
        ),
        (Fault::unknown_account("x"), 400, "unknown_account"),
    ];
    for (fault, status, code) in cases {
        let error = TerminalError::from(fault);
        assert_eq!(error.code(), status);
        let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
        assert_eq!(body["code"], code);
        assert_eq!(body.get("order"), None);
    }
}

/// The fault a credential rejection raises names the szamlazz.hu code, tells
/// the caller nothing was issued, and carries the document identity when one
/// is attached.
#[test]
fn credentials_rejected_fault_names_the_code_and_the_document() {
    use restate_sdk::errors::TerminalError;

    use super::support::Fault;
    use crate::contract::IssuedKind;
    use crate::identity::OrderKey;

    let order = OrderKey::parse("ORD-1").expect("order");
    let fault = Fault::credentials_rejected(&namespace(), "136", "Bejelentkezés letiltva").about(
        &order,
        Some(IssuedKind::Invoice),
        "acct:ORD-1:invoice",
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
    assert!(message.contains("issued nothing"), "{message}");
}

/// The agent key never reaches the operator's warning or the caller's fault
/// body: both are built from what szamlazz.hu answered, tagged with the
/// namespace and the code only. Every event the crate emits during the
/// exchange and the fault construction is captured at `TRACE`, and the key is
/// demonstrably on the wire when the rejection is observed.
#[tokio::test]
async fn credentials_rejected_never_leaks_the_agent_key() {
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    use restate_sdk::errors::TerminalError;
    use tracing_subscriber::fmt::MakeWriter;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::support::Fault;
    use crate::gateway::QueryOutcome;

    /// A `MakeWriter` collecting formatted events into a shared buffer.
    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl Write for Capture {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("capture").extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Capture {
        type Writer = Self;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

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
    let order = Order::from_parts(accounts(&server.uri(), KEY), WorkerConfig::new(namespace()));

    let capture = Capture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(capture.clone())
        .with_ansi(false)
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);

    // Pin the warning's callsite to this thread's subscriber. tracing caches
    // a callsite's interest on its first hit — and only once a subscriber has
    // raised the global max level, so it cannot be pre-registered — and a
    // first hit from a parallel test thread, which has no subscriber, would
    // cache it as disabled. Hitting it here registers it; the rebuild
    // re-evaluates it against this thread's subscriber in case a parallel
    // thread was first. The warm-up event is told apart by its namespace.
    drop(Fault::credentials_rejected(
        &"warmup".parse().expect("namespace"),
        "0",
        "warm-up",
    ));
    tracing::callsite::rebuild_interest_cache();

    // What the prologue does: resolve, fetch, open — then the gateway
    // observes the code and the fault is built.
    let account = order.accounts().resolve(None).await.expect("account");
    let credentials = order.accounts().fetch(&account).await.expect("credentials");
    let gateway = Gateway::open(account, credentials).expect("gateway");
    let outcome = gateway.verify("SZ-1").await;
    let Ok(QueryOutcome::CredentialsRejected { code, message }) = outcome.clone() else {
        panic!("expected CredentialsRejected, got {outcome:?}");
    };
    assert_eq!(code, "3");
    let error = TerminalError::from(Fault::credentials_rejected(
        &order.config().namespace,
        code,
        message,
    ));
    drop(guard);

    let sent = server.received_requests().await.expect("requests");
    assert!(
        String::from_utf8_lossy(&sent[0].body).contains(KEY),
        "the sentinel key must have been on the wire for the test to mean anything"
    );
    assert!(!format!("{outcome:?}").contains(KEY), "{outcome:?}");

    let logs = String::from_utf8(capture.0.lock().expect("capture").clone()).expect("utf-8");
    assert!(logs.contains("WARN"), "{logs}");
    assert!(logs.contains("namespace=acct"), "{logs}");
    assert!(logs.contains("code=3"), "{logs}");
    assert!(!logs.contains(KEY), "{logs}");
    assert_eq!(error.code(), 503);
    assert!(!error.message().contains(KEY), "{}", error.message());
    let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
    assert_eq!(body["code"], "credentials_rejected");
}

/// The agent key never reaches the `account_mismatch` fault body either: it is
/// built from the pins of the found document and the resolved account, and no
/// document carries the key. The key is demonstrably on the wire when the
/// document is found, and demonstrably absent from what the gateway returns
/// and what the fault says.
#[tokio::test]
async fn account_mismatch_never_leaks_the_agent_key() {
    use restate_sdk::errors::TerminalError;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::support::check_pins;
    use crate::gateway::QueryOutcome;

    const KEY: &str = "sentinel-agent-key-4b8e1d";
    let server = MockServer::start().await;
    // A live-account document of another supplier — what a test account
    // configured as live, or the wrong account's key, finds by number.
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            szamla_xml(1, &[("szamlaszam", "SZ-2"), ("teszt", "false")]),
            "application/xml",
        ))
        .expect(1)
        .mount(&server)
        .await;
    let order = Order::from_parts(accounts(&server.uri(), KEY), WorkerConfig::new(namespace()));

    let account = order.accounts().resolve(None).await.expect("account");
    let credentials = order.accounts().fetch(&account).await.expect("credentials");
    let gateway = Gateway::open(account, credentials).expect("gateway");
    let verified = gateway.verify("SZ-2").await;
    let Ok(QueryOutcome::Found(found)) = verified.clone() else {
        panic!("expected Found, got {verified:?}");
    };
    let mismatch = TerminalError::from(check_pins(gateway.account(), &found).expect_err("a fault"));

    let sent = server.received_requests().await.expect("requests");
    assert!(
        String::from_utf8_lossy(&sent[0].body).contains(KEY),
        "the sentinel key must have been on the wire for the test to mean anything"
    );
    assert!(!format!("{verified:?}").contains(KEY), "{verified:?}");
    assert_eq!(mismatch.code(), 409);
    assert!(!mismatch.message().contains(KEY), "{}", mismatch.message());
    let body: serde_json::Value = serde_json::from_str(mismatch.message()).expect("json body");
    assert_eq!(body["code"], "account_mismatch");
    let message = body["message"].as_str().expect("message");
    assert!(message.contains("SZ-2"), "{message}");
    assert!(message.contains("teszt = false"), "{message}");
    assert!(message.contains("supplier Some(1)"), "{message}");
}

#[test]
fn lookup_classifies_query_outcomes() {
    use super::support::Lookup;
    use crate::contract::IssuedKind;
    use crate::gateway::QueryOutcome;
    use crate::identity::OrderKey;

    let order = OrderKey::parse("ORD-1").expect("order");
    let namespace = namespace();
    let classify = |outcome: QueryOutcome, supplier: Option<u64>| {
        Lookup::classify(
            outcome,
            &namespace,
            &order,
            IssuedKind::Invoice,
            true,
            supplier,
        )
    };

    assert_eq!(
        classify(QueryOutcome::NotFound, None).expect("classified"),
        Lookup::Absent
    );
    let ours = found(SUPPLIER, &[]);
    let lookup = classify(QueryOutcome::Found(ours.clone()), Some(SUPPLIER)).expect("classified");
    assert_eq!(lookup, Lookup::Ours(ours));

    let reversed = found(SUPPLIER, &[("sztornozott", "true")]);
    let lookup = classify(QueryOutcome::Found(reversed.clone()), None).expect("classified");
    assert_eq!(lookup, Lookup::Ours(reversed));

    for (label, other) in [
        ("order", found(SUPPLIER, &[("rendelesszam", "ORD-2")])),
        ("kind", found(SUPPLIER, &[("tipus", "D")])),
        ("test", found(SUPPLIER, &[("teszt", "false")])),
        ("supplier", found(1, &[])),
    ] {
        let lookup = classify(QueryOutcome::Found(other.clone()), Some(SUPPLIER)).expect(label);
        assert_eq!(lookup, Lookup::Collision(other), "{label}");
    }
    // Another szamlazz.hu code is an answer the handler cannot conclude from:
    // the `unavailable` fault naming the code, as before the read policy.
    let fault = classify(
        QueryOutcome::Api {
            code: "57".to_owned(),
            message: "Ismeretlen hiba".to_owned(),
        },
        None,
    )
    .expect_err("a fault");
    let error = restate_sdk::errors::TerminalError::from(fault);
    assert_eq!(error.code(), 503);
    let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
    assert_eq!(body["code"], "unavailable");
    let message = body["message"].as_str().expect("message");
    assert!(message.contains("57"), "{message}");
    assert!(message.contains("Ismeretlen hiba"), "{message}");

    // Rejected credentials are a fault of their own, not `unavailable`.
    let fault = classify(
        QueryOutcome::CredentialsRejected {
            code: "3".to_owned(),
            message: "login".to_owned(),
        },
        None,
    )
    .expect_err("a fault");
    let error = restate_sdk::errors::TerminalError::from(fault);
    assert_eq!(error.code(), 503);
    let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
    assert_eq!(body["code"], "credentials_rejected");
}

/// A read step that ended without an answer — the read policy exhausted
/// (500 carrying the last `Unanswered`) or the invocation cancelled (409) —
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
    let about = fault.about(&order, Some(IssuedKind::Invoice), "acct:ORD-1:invoice");
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

/// The Virtual Object key must arrive trimmed (design §3): Restate's per-key
/// lock is on the *raw* key, so `ORD-1` and ` ORD-1` would be two instances
/// with two locks mapping to one szamlazz.hu order and identical external ids
/// — two concurrent creates under them would both pass their lookup and both
/// send. The handler refuses a key whose trimmed form differs from the raw
/// one as `invalid_input` naming the rule; [`OrderKey::parse`] itself stays
/// lenient for the places that parse an order number rather than a key.
#[test]
fn the_order_key_must_arrive_trimmed() {
    use restate_sdk::errors::TerminalError;

    use super::support::order_key;
    use crate::identity::OrderKey;

    let key = order_key("ORD-1").expect("a trimmed key");
    assert_eq!(key.as_str(), "ORD-1");
    let key = order_key("rendelés #42").expect("inner single spaces are fine");
    assert_eq!(key.as_str(), "rendelés #42");

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

    // The type's own rules still apply to a trimmed key, with its message.
    let fault = order_key("a  b").expect_err("a whitespace run");
    let body: serde_json::Value =
        serde_json::from_str(TerminalError::from(fault).message()).expect("json body");
    assert_eq!(body["code"], "invalid_input");
    assert!(
        body["message"]
            .as_str()
            .expect("message")
            .contains("consecutive whitespace"),
        "{body}"
    );
}

/// Every handler that finds a document checks it against the account the
/// invocation resolved to (design §3): `teszt` must equal the account's mode
/// and, when the account pins a supplier id, `szallito/id` must match it. A
/// mismatch is the `account_mismatch` fault (409) naming the observed pins and
/// the resolved account's — a test account configured as live fails loudly on
/// its first found document.
#[test]
fn a_found_document_must_belong_to_the_resolved_account() {
    use restate_sdk::errors::TerminalError;

    use super::support::check_pins;
    use crate::account::Account;
    use crate::config::AccountMode;

    let mut account = Account::new("acct", "acct");
    account.mode = AccountMode::Test;
    account.supplier_id = Some(SUPPLIER);

    check_pins(&account, &found(SUPPLIER, &[])).expect("ours");
    check_pins(&account, &found(SUPPLIER, &[("rendelesszam", "OTHER")]))
        .expect("the order number is not a pin of the account");

    // A live document on a test account, or a test document on a live one.
    let fault = check_pins(&account, &found(SUPPLIER, &[("teszt", "false")])).expect_err("a fault");
    let error = TerminalError::from(fault);
    assert_eq!(error.code(), 409);
    let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
    assert_eq!(body["code"], "account_mismatch");
    let message = body["message"].as_str().expect("message");
    assert!(message.contains("SZ-1"), "{message}");
    assert!(message.contains("teszt = false"), "{message}");
    assert!(message.contains("teszt = true"), "{message}");
    assert_eq!(
        body.get("order"),
        None,
        "no order identity on a by-number check"
    );

    let mut live = account.clone();
    live.mode = AccountMode::Live;
    let fault = check_pins(&live, &found(SUPPLIER, &[])).expect_err("a fault");
    let body: serde_json::Value =
        serde_json::from_str(TerminalError::from(fault).message()).expect("json body");
    assert_eq!(body["code"], "account_mismatch");

    // Another supplier, when the account pins one.
    let fault = check_pins(&account, &found(1, &[])).expect_err("a fault");
    let body: serde_json::Value =
        serde_json::from_str(TerminalError::from(fault).message()).expect("json body");
    assert_eq!(body["code"], "account_mismatch");
    let message = body["message"].as_str().expect("message");
    assert!(message.contains("supplier Some(1)"), "{message}");
    assert!(
        message.contains(&format!("supplier Some({SUPPLIER})")),
        "{message}"
    );

    // No supplier pin: the supplier id is not checked.
    let mut unpinned = account.clone();
    unpinned.supplier_id = None;
    check_pins(&unpinned, &found(1, &[])).expect("unpinned");
}
