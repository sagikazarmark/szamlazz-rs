//! Discovery and binding tests of the Restate adapters (design §11): the
//! service names, the handler set with its shared flags and the per-handler
//! retry policy, plus an `Endpoint` build.

use std::sync::Arc;

use restate_sdk::discovery::{HandlerType, RetryPolicyOnMaxAttempts, ServiceType};
use restate_sdk::endpoint::Endpoint;
use restate_sdk::service::Discoverable;
use serde_json::json;

use super::{Agent, Order};
use crate::config::{Config, WorkerConfig};

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
            // three attempts, no retention.
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
            assert_eq!(handler.journal_retention, None);
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
            // retention (nothing to replay).
            assert_eq!(handler.retry_policy_initial_interval, Some(10_000));
            assert_eq!(handler.retry_policy_max_interval, Some(60_000));
            assert_eq!(handler.retry_policy_exponentiation_factor, Some(2.0));
            assert_eq!(handler.retry_policy_max_attempts, Some(3));
            assert_eq!(handler.inactivity_timeout, None);
            assert_eq!(handler.abort_timeout, None);
            assert_eq!(handler.journal_retention, None);
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

/// Both services hold the same gateway and the same deployment-level
/// settings, and nothing else.
#[test]
fn services_bind_to_an_endpoint() {
    let config = config();
    let order = Order::new(&config).expect("order");
    let agent = Agent::from_parts(Arc::clone(order.gateway()), order.config().clone());
    assert!(Arc::ptr_eq(order.gateway(), agent.gateway()));
    assert_eq!(order.config(), agent.config());
    assert_eq!(*order.config(), WorkerConfig::from(&config));
    assert_eq!(order.config().namespace.as_str(), "acct");
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
    ];
    for (fault, status, code) in cases {
        let error = TerminalError::from(fault);
        assert_eq!(error.code(), status);
        let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
        assert_eq!(body["code"], code);
        assert_eq!(body.get("order"), None);
    }
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
    let classify = |outcome: QueryOutcome, supplier: Option<u64>| {
        Lookup::classify(outcome, &order, IssuedKind::Invoice, true, supplier)
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
}
