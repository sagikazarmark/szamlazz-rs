//! Cancellation at the public ingress: structured reads and uncertain writes.

use std::sync::Arc;
use std::time::Duration;

use restate_e2e_harness::Call;
use restate_szamlazz::contract::TerminalCode;
use serde_json::json;
use tokio::sync::Notify;

use crate::harness::Harness;
use crate::harness::szamlazz::{Doc, not_found, number_query, order_query};

/// The ordinary query and both best-effort storno reads speak one contract.
pub(crate) async fn cancelled_reads_are_structured(h: &Harness) {
    for (number, order, handler) in [
        ("SZ-CANCEL-READ", "E2E-CANCEL-READ", "query"),
        ("SZ-CANCEL-HINT", "E2E-CANCEL-HINT", "storno_invoice"),
        ("SZ-CANCEL-LOOKUP", "", "storno"),
    ] {
        let received = Arc::new(Notify::new());
        let signal = Arc::clone(&received);
        let slow = move |_: &wiremock::Request| {
            signal.notify_one();
            not_found().set_delay(Duration::from_secs(4))
        };
        if handler == "query" {
            number_query(number).respond_with(slow).mount(&h.mock).await;
        } else {
            number_query(number)
                .respond_with(
                    Doc {
                        reversed: true,
                        ..Doc::of(number, "SZ", order)
                    }
                    .response(),
                )
                .mount(&h.mock)
                .await;
            if handler == "storno_invoice" {
                order_query(order).respond_with(slow).mount(&h.mock).await;
            } else {
                crate::harness::szamlazz::external_id_query(&format!(
                    "acct:by-number:{number}:storno"
                ))
                .respond_with(slow)
                .mount(&h.mock)
                .await;
            }
        }
        let call = if handler == "storno_invoice" {
            Call::object("Szamlazz.Order", order, handler)
        } else {
            Call::service("Szamlazz.Agent", handler)
        };
        let body = if handler == "query" {
            json!({"selector": {"invoice_number": number}})
        } else {
            json!({"invoice_number": number})
        };
        let submitted = h.invoke(&call.send(), Some(&body), Some(number)).await;
        assert_eq!(submitted.status, 202, "{}", submitted.body);
        tokio::time::timeout(Duration::from_secs(30), received.notified())
            .await
            .expect("read reached szamlazz.hu");
        h.admin().cancel(submitted.invocation_id()).await;
        let reply = h.invoke(&call, Some(&body), Some(number)).await;
        assert_eq!(reply.status, 409, "{}", reply.body);
        let fault = reply.fault();
        assert_eq!(fault.code, TerminalCode::Cancelled);
        assert_eq!(fault.is_cancelled(), Some(true));
        assert_eq!(fault.code.is_outcome_unknown(), Some(false));
        assert!(!fault.message.contains("retry"), "{fault:?}");
        if handler == "storno_invoice" {
            h.assert_state_absent(None, order).await;
        }
    }
}

/// Cancellation while resolution retries never becomes dependency unavailability.
pub(crate) async fn cancelled_resolution_is_structured(h: &Harness) {
    h.reset().await;
    let before = h.multi().resolutions("beta");
    h.multi().fail_next_resolutions("beta", 100);
    let call = Call::service("Szamlazz.Agent", "query").scoped("beta");
    let body = json!({"selector": {"invoice_number": "SZ-CANCEL-RESOLVE"}});
    let key = "cancel-resolve";
    let submitted = h.invoke(&call.send(), Some(&body), Some(key)).await;
    tokio::time::timeout(Duration::from_secs(30), async {
        while h.multi().resolutions("beta") == before {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("resolver reached");
    h.admin().cancel(submitted.invocation_id()).await;
    let reply = h.invoke(&call, Some(&body), Some(key)).await;
    h.multi().fail_next_resolutions("beta", 0);
    assert_eq!(reply.status, 409, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, TerminalCode::Cancelled);
    assert_eq!(fault.is_cancelled(), Some(true));
    assert!(
        h.mock
            .received_requests()
            .await
            .expect("requests")
            .is_empty()
    );
}

/// Both storno shells keep cancellation distinct while a reversal can land.
pub(crate) async fn cancelled_storno_is_uncertain(h: &Harness) {
    use crate::harness::szamlazz::{created, external_id_query, storno_of_number};
    for (number, order) in [
        ("SZ-CANCEL-STORNO", "E2E-CANCEL-STORNO"),
        ("SZ-CANCEL-UNMANAGED", ""),
    ] {
        number_query(number)
            .respond_with(Doc::of(number, "SZ", order).response())
            .mount(&h.mock)
            .await;
        let external_id = if order.is_empty() {
            format!("acct:by-number:{number}:storno")
        } else {
            format!("acct:{order}:storno:{number}")
        };
        external_id_query(&external_id)
            .respond_with(not_found())
            .mount(&h.mock)
            .await;
        let received = Arc::new(Notify::new());
        let signal = Arc::clone(&received);
        storno_of_number(number)
            .respond_with(move |_: &wiremock::Request| {
                signal.notify_one();
                created("SS-CANCELLED", "-1000", "-1270").set_delay(Duration::from_secs(4))
            })
            .expect(1)
            .mount(&h.mock)
            .await;
        let call = if order.is_empty() {
            Call::service("Szamlazz.Agent", "storno")
        } else {
            Call::object("Szamlazz.Order", order, "storno_invoice")
        };
        let reply = crate::policies::cancel_after_send(
            h,
            call,
            &json!({"invoice_number": number}),
            number,
            &received,
        )
        .await;
        let fault = reply.fault();
        if call.service == "Szamlazz.Agent" {
            assert!(
                fault.message.contains("query the original invoice"),
                "{fault:?}"
            );
            assert!(fault.message.contains("only after settlement"), "{fault:?}");
            assert!(
                fault.message.contains("absence and elapsed time"),
                "{fault:?}"
            );
        } else {
            assert!(
                fault.message.contains("unresolved marker is retained"),
                "{fault:?}"
            );
            h.expect_unresolved(None, order).await;
        }
    }
}
