//! Real retained bytes and run-failure observations, not just field-name scans.

use std::sync::atomic::{AtomicUsize, Ordering};

use restate_e2e_harness::run_result;
use restate_szamlazz::contract::{IssuedKind, TerminalCode};
use rust_decimal::dec;
use serde_json::json;
use wiremock::ResponseTemplate;

use crate::harness::accounts::AGENT_KEY;
use crate::harness::szamlazz::{
    api_error, create_for, credit_of, external_id_query, not_found, number_query, order_query,
};
use crate::harness::{Harness, create_body};

const PRIVATE: &str = "PRIVATE-DOCUMENT-DIAGNOSTIC-215";

fn safe(text: &str) {
    for sentinel in [AGENT_KEY, PRIVATE] {
        assert!(
            !text.contains(sentinel),
            "sensitive diagnostic retained: {text}"
        );
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "one privacy scenario: nested write, read, and one-shot outcomes"
)]
pub(crate) async fn diagnostics_are_safe_in_run_failures_journals_and_ingress(h: &Harness) {
    credential_requery_keeps_the_send(h).await;
    let order = "E2E-PRIVACY-215";
    h.absent(order, &["prepayment", "final", "proforma"]).await;
    order_query(order)
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    let calls = AtomicUsize::new(0);
    external_id_query("acct:E2E-PRIVACY-215:invoice")
        .respond_with(move |_: &wiremock::Request| {
            let n = calls.fetch_add(1, Ordering::SeqCst);
            // Target and full lookup, then each execution's leading query.
            if n < 3 || n == 4 {
                not_found()
            } else {
                ResponseTemplate::new(200)
                    .set_body_string(format!("<{PRIVATE}>{AGENT_KEY}</{PRIVATE}>"))
            }
        })
        .expect(6)
        .mount(&h.mock)
        .await;
    create_for(order)
        .respond_with(ResponseTemplate::new(502).set_body_string(format!("{AGENT_KEY} {PRIVATE}")))
        .expect(2)
        .mount(&h.mock)
        .await;
    let watch = h.watch(order);
    let reply = h
        .call(
            order,
            "create_invoice",
            &create_body(dec!(1000)),
            "privacy-215-create",
        )
        .await;
    let observed = watch.finish().await;
    assert_eq!(reply.status, 500, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, TerminalCode::OutcomeUnknown);
    assert_eq!(fault.order.as_deref(), Some(order));
    assert_eq!(fault.kind, Some(IssuedKind::Invoice));
    assert_eq!(
        fault.external_id.as_deref(),
        Some("acct:E2E-PRIVACY-215:invoice")
    );
    assert!(fault.message.contains("create: HTTP 502"), "{fault:?}");
    assert!(fault.message.contains("query: parse:"), "{fault:?}");
    safe(&reply.body.to_string());
    assert!(
        !observed.failures.is_empty(),
        "nonvacuous run-failure scan: {observed:?}"
    );
    assert!(
        observed
            .failing_commands
            .iter()
            .any(|name| name == "create-invoice")
    );
    for failure in &observed.failures {
        safe(failure);
        assert!(
            failure.contains("HTTP 502") && failure.contains("query: parse:"),
            "{failure}"
        );
    }
    inspect(h, reply.invocation_id(), "create-invoice", "HTTP 502").await;

    // A failed read is still unavailable; no write was attempted.
    number_query("SZ-PRIVACY-READ")
        .respond_with(
            ResponseTemplate::new(503)
                .insert_header("szlahu_down", format!("{AGENT_KEY} {PRIVATE}")),
        )
        .expect(3)
        .mount(&h.mock)
        .await;
    let read = h
        .call_agent(
            "query",
            &json!({"selector": {"invoice_number": "SZ-PRIVACY-READ"}}),
        )
        .await;
    assert_eq!(read.status, 503, "{}", read.body);
    assert_eq!(read.fault().code, TerminalCode::Unavailable);
    assert!(read.fault().message.contains("query: szlahu_down"));
    safe(&read.body.to_string());
    inspect(h, read.invocation_id(), "query", "szlahu_down").await;

    for (number, response, status, code, category) in [
        (
            "SZ-PRIVACY-LOST",
            ResponseTemplate::new(502).set_body_string(format!("{AGENT_KEY} {PRIVATE}")),
            500,
            TerminalCode::OutcomeUnknown,
            "HTTP 502",
        ),
        (
            "SZ-PRIVACY-KEY",
            api_error("3", &format!("{AGENT_KEY} {PRIVATE}")),
            503,
            TerminalCode::CredentialsRejected,
            "credentials rejected",
        ),
    ] {
        credit_of(number)
            .respond_with(response)
            .expect(1)
            .mount(&h.mock)
            .await;
        let reply = h
            .call_agent(
                "set_credit_entries",
                &json!({"invoice_number": number, "entries": [], "additive": true}),
            )
            .await;
        assert_eq!(reply.status, status, "{}", reply.body);
        assert_eq!(reply.fault().code, code);
        assert!(reply.fault().message.contains(category));
        if code == TerminalCode::CredentialsRejected {
            assert_eq!(reply.fault().szamlazz_code.as_deref(), Some("3"));
        }
        safe(&reply.body.to_string());
        inspect(
            h,
            reply.invocation_id(),
            &format!("set-credit-entries-{number}"),
            category,
        )
        .await;
        if code == TerminalCode::OutcomeUnknown {
            let journal = h.admin().journal(reply.invocation_id()).await;
            let result = run_result(&journal, &format!("set-credit-entries-{number}"))
                .expect("lost outcome");
            assert!(result.raw_contains("Lost") && result.raw_contains("Transport"));
        }
    }
}

async fn credential_requery_keeps_the_send(h: &Harness) {
    let order = "E2E-PRIVACY-CREDENTIAL";
    h.absent(order, &["prepayment", "final", "proforma"]).await;
    order_query(order)
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-PRIVACY-CREDENTIAL:invoice")
        .respond_with(not_found())
        .up_to_n_times(3)
        .expect(3)
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-PRIVACY-CREDENTIAL:invoice")
        .respond_with(api_error("135", &format!("{AGENT_KEY} {PRIVATE}")))
        .expect(1)
        .mount(&h.mock)
        .await;
    create_for(order)
        .respond_with(ResponseTemplate::new(502).set_body_string(format!("{AGENT_KEY} {PRIVATE}")))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call(
            order,
            "create_invoice",
            &create_body(dec!(1000)),
            "privacy-215-credential",
        )
        .await;
    assert_eq!(reply.status, 503, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, TerminalCode::CredentialsRejected);
    assert_eq!(fault.szamlazz_code.as_deref(), Some("135"));
    assert!(fault.message.contains("create: HTTP 502"));
    assert!(
        fault
            .message
            .contains("credentials rejected: browser session active")
    );
    safe(&reply.body.to_string());
    inspect(
        h,
        reply.invocation_id(),
        "create-invoice",
        "create: HTTP 502",
    )
    .await;
}

async fn inspect(h: &Harness, id: &str, step: &str, category: &str) {
    let journal = h.admin().journal(id).await;
    let result = run_result(&journal, step).expect("retained run completion");
    assert!(
        result.raw_contains(category),
        "safe category in retained bytes: {result:?}"
    );
    for entry in &journal {
        for sentinel in [AGENT_KEY, PRIVATE] {
            assert!(
                !entry.raw_contains(sentinel),
                "entry {} retained {sentinel}",
                entry.index
            );
        }
    }
    let invocation = h.admin().invocation(id).await;
    let failure = invocation.completion_failure.expect("retained fault");
    safe(&failure);
    assert!(failure.contains(category));
}
