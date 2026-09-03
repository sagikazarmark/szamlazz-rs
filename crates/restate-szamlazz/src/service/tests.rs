//! Discovery and binding tests of the Restate adapters (design §11): the
//! service names, the handler set with its shared / private flags and the
//! per-handler retry policy, plus an `Endpoint` build.

use std::sync::Arc;

use restate_sdk::discovery::{HandlerType, RetryPolicyOnMaxAttempts, ServiceType};
use restate_sdk::endpoint::Endpoint;
use restate_sdk::service::Discoverable;
use serde_json::json;

use super::{Order, SzamlaAgentService};
use crate::config::Config;

fn config() -> Arc<Config> {
    Arc::new(
        serde_json::from_value(json!({
            "account": {
                "slug": "acct",
                "agent_key": "key",
                "fp_secret": "fp",
                "endpoint": "http://127.0.0.1:1/",
                "mode": "test",
            },
        }))
        .expect("config"),
    )
}

#[test]
fn order_discovers_as_a_virtual_object_with_ten_handlers() {
    let discovery = <Order as Discoverable>::discover();
    assert_eq!(discovery.name.as_str(), "Order");
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
            "forget",
            "get",
            "record_reversal",
            "storno_invoice",
        ]
    );

    for handler in &discovery.handlers {
        let name = handler.name.as_str();
        // Exclusive is the Virtual Object default and left implicit (`None`);
        // only shared handlers are flagged.
        let expected_ty = (name == "get").then_some(HandlerType::Shared);
        assert_eq!(handler.ty, expected_ty, "{name}");
        let private = matches!(name, "record_reversal" | "forget");
        assert_eq!(handler.ingress_private, private.then_some(true), "{name}");
        if private {
            assert_eq!(handler.retry_policy_max_attempts, None, "{name}");
        } else {
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
            assert_eq!(
                handler.retry_policy_on_max_attempts,
                Some(RetryPolicyOnMaxAttempts::Kill),
                "{name}"
            );
            assert_eq!(handler.inactivity_timeout, Some(240_000), "{name}");
            assert_eq!(handler.abort_timeout, Some(180_000), "{name}");
            assert_eq!(
                handler.journal_retention,
                Some(3 * 24 * 3_600_000),
                "{name}"
            );
            assert_eq!(
                handler.idempotency_retention,
                Some(7 * 24 * 3_600_000),
                "{name}"
            );
        }
        assert!(handler.input.is_some(), "{name} takes an input");
        assert!(handler.output.is_some(), "{name} returns an output");
    }
}

#[test]
fn szamla_agent_discovers_as_a_service_with_three_handlers() {
    let discovery = <SzamlaAgentService as Discoverable>::discover();
    assert_eq!(discovery.name.as_str(), "SzamlaAgent");
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
                Some(7 * 24 * 3_600_000),
                "{name}"
            );
        }
    }
}

#[test]
fn services_bind_to_an_endpoint() {
    let config = config();
    let order = Order::new(Arc::clone(&config)).expect("order");
    let agent = SzamlaAgentService::from_parts(Arc::clone(order.agent()), config);
    assert!(Arc::ptr_eq(order.agent(), agent.agent()));
    let _endpoint = Endpoint::builder().bind(order).bind(agent).build();
}

#[test]
fn faults_serialise_their_code_and_status() {
    use restate_sdk::errors::TerminalError;

    use super::support::Fault;
    use crate::contract::{IssuedKind, TerminalCode};
    use crate::identity::OrderKey;

    let order = OrderKey::parse("ORD-1").expect("order");
    let request_id = "r-1".parse().expect("request id");
    let fault = Fault::outcome_unknown("exhausted").about(
        &order,
        IssuedKind::Invoice,
        0,
        "acct:ORD-1:invoice:0",
        Some(&request_id),
    );
    let error = TerminalError::from(fault);
    assert_eq!(error.code(), 500);
    let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
    assert_eq!(body["code"], TerminalCode::OutcomeUnknown.as_str());
    assert_eq!(body["order"], "ORD-1");
    assert_eq!(body["kind"], "invoice");
    assert_eq!(body["gen"], 0);
    assert_eq!(body["external_id"], "acct:ORD-1:invoice:0");
    assert_eq!(body["request_id"], "r-1");

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
