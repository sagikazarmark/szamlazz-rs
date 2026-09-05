//! Discovery and binding tests of the Restate adapters (design §11): the
//! service names, the handler set with its shared flags and the per-handler
//! retry policy, plus an `Endpoint` build — and the fault → `TerminalError`
//! mapping the handlers share, including the sentinel that the agent key
//! reaches neither the `credentials_rejected` warning nor the fault body.

use restate_sdk::discovery::{HandlerType, RetryPolicyOnMaxAttempts, ServiceType};
use restate_sdk::endpoint::Endpoint;
use restate_sdk::service::Discoverable;
use serde_json::json;

use super::{Agent, Order};
use crate::account::ResolveError;
use crate::config::{Config, Namespace, WorkerConfig};
use crate::gateway::Gateway;

fn config() -> Config {
    serde_json::from_value(json!({
        "account": {
            "slug": "acct",
            "agent_key": "key",
            "endpoint": "http://127.0.0.1:1/",
            "mode": "test",
        },
    }))
    .expect("config")
}

fn namespace() -> Namespace {
    "acct".parse().expect("namespace")
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
fn agent_discovers_as_a_service_with_three_handlers() {
    let discovery = <Agent as Discoverable>::discover();
    assert_eq!(discovery.name.as_str(), "Szamlazz.Agent");
    assert_eq!(discovery.ty, ServiceType::Service);

    let mut names: Vec<_> = discovery
        .handlers
        .iter()
        .map(|handler| handler.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["query", "set_payments", "storno"]);

    for handler in &discovery.handlers {
        let name = handler.name.as_str();
        assert_eq!(handler.ingress_private, None, "{name} is public");
        assert_eq!(
            handler.retry_policy_on_max_attempts,
            Some(RetryPolicyOnMaxAttempts::Kill),
            "{name}"
        );
        if name == "query" {
            // Read-only: a short 10s → 1m back-off, three attempts, no
            // idempotency retention (nothing to replay); an explicit journal
            // retention so the journal is inspectable.
            assert_eq!(handler.retry_policy_initial_interval, Some(10_000));
            assert_eq!(handler.retry_policy_max_interval, Some(60_000));
            assert_eq!(handler.retry_policy_exponentiation_factor, Some(2.0));
            assert_eq!(handler.retry_policy_max_attempts, Some(3));
            assert_eq!(handler.inactivity_timeout, None);
            assert_eq!(handler.abort_timeout, None);
            assert_eq!(handler.journal_retention, Some(24 * 3_600_000));
            assert_eq!(handler.idempotency_retention, None);
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
    let config = config();
    let order = Order::new(&config).expect("order");
    let agent = Agent::from_parts(order.accounts().clone(), order.config().clone());
    assert_eq!(order.config(), agent.config());
    assert_eq!(*order.config(), WorkerConfig::from(&config));
    assert_eq!(order.config().namespace.as_str(), "acct");
    // The adapter: the single account, unscoped, with the inline key.
    let account = order.accounts().resolve(None).await.expect("account");
    assert_eq!(account.id.as_str(), "acct");
    assert!(account.mode.is_test());
    assert!(order.accounts().fetch(&account).await.is_ok());
    assert!(
        matches!(
            agent.accounts().resolve(Some("tenant")).await,
            Err(ResolveError::Unknown { scope }) if scope == "tenant"
        ),
        "a single-account deployment knows no scope"
    );
    let _endpoint = Endpoint::builder().bind(order).bind(agent).build();
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
    let config: Config = serde_json::from_value(json!({
        "account": {
            "slug": "acct",
            "agent_key": KEY,
            "endpoint": server.uri(),
            "mode": "test",
        },
    }))
    .expect("config");
    let order = Order::new(&config).expect("order");

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
    let QueryOutcome::CredentialsRejected { code, message } = outcome.clone() else {
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

#[test]
fn lookup_classifies_query_outcomes() {
    use szamlazz_agent::InvoiceNumber;
    use szamlazz_agent::ops::query_pdf::InvoiceSelector;
    use szamlazz_agent::ops::query_xml::{InvoiceDocument, QueryInvoiceXml};
    use szamlazz_agent::wire::{AgentRequest as _, RawResponse};

    use super::support::Lookup;
    use crate::contract::IssuedKind;
    use crate::gateway::QueryOutcome;
    use crate::identity::OrderKey;

    /// A live `SZ-1` of `ORD-1` from a test account, with the given `alap`
    /// elements overridden.
    fn found(supplier_id: u64, alap_overrides: &[(&str, &str)]) -> Box<InvoiceDocument> {
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
        let body = format!(
            r#"<szamla xmlns="http://www.szamlazz.hu/szamla">
              <szallito><id>{supplier_id}</id><nev>Seller</nev><cim><irsz>1111</irsz><telepules>Budapest</telepules><cim>Fő u. 1.</cim></cim></szallito>
              <alap><id>1</id>{alap}</alap>
              <vevo><nev>Buyer</nev></vevo><tetelek></tetelek>
              <osszegek><totalossz><netto>0</netto><afa>0</afa><brutto>0</brutto></totalossz></osszegek>
              </szamla>"#
        );

        Box::new(
            QueryInvoiceXml::new(InvoiceSelector::InvoiceNumber(InvoiceNumber::new("SZ-1")))
                .parse(&RawResponse::new::<&str, &str>([], body.into_bytes()))
                .expect("parse"),
        )
    }

    const SUPPLIER: u64 = 972_720;
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
    assert!(classify(QueryOutcome::Transport("down".to_owned()), None).is_err());

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
