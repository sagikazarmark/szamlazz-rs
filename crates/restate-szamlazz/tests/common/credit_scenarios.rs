//! Same Agent credit-entry behavior on native `RequestResponse` and workerd.
use super::common;
use restate_e2e_harness::{Call, Restate};
use serde_json::{Value, json};
use std::{
    future::Future,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

fn body(number: &str, additive: bool) -> Value {
    json!({"invoice_number":number,"additive":additive,"entries":[{"date":"2026-09-14","title":"transfer","amount":"9007199254740993.01","comment":"CREDIT-PRIVATE-COMMENT"}]})
}

pub async fn run<F, Fut>(
    server: &Restate,
    mock: &wiremock::MockServer,
    scope: Option<&str>,
    interrupt: F,
) where
    F: Fn() -> Fut,
    Fut: Future<Output = ()>,
{
    let mut call = Call::service("Szamlazz.Agent", "set_credit_entries");
    if let Some(scope) = scope {
        call = call.scoped(scope);
    }
    // Input refusals precede credential acquisition and all provider calls.
    let before = mock.received_requests().await.expect("requests").len();
    for entries in [
        json!([]),
        json!([{"date":"2026-09-14","title":"transfer","amount":"1.00000000000000000000000000001"}]),
    ] {
        let response = server
            .invoke(
                &call,
                Some(&json!({"invoice_number":"CREDIT-BAD","entries":entries,"additive":false})),
                None,
            )
            .await;
        assert_eq!(response.status, 400, "{response:?}");
    }
    assert_eq!(
        mock.received_requests().await.expect("requests").len(),
        before
    );
    for additive in [false, true] {
        for scenario in [
            "success",
            "lost",
            "refused",
            "credentials",
            "mismatch",
            "interrupted",
            "later-refusal",
            "cancelled",
        ] {
            let number = format!("CREDIT-{scenario}-{}", u8::from(additive));
            let sends = Arc::new(AtomicUsize::new(0));
            // Independent fake-provider records: one number per received entry.
            let entries = Arc::new(Mutex::new(Vec::<&'static str>::new()));
            let count = sends.clone();
            let stored = entries.clone();
            let target = number.clone();
            common::credit_of(&number)
                .respond_with(move |request: &wiremock::Request| {
                    let xml = String::from_utf8_lossy(&request.body);
                    assert!(xml.contains("<osszeg>9007199254740993.01</osszeg>"));
                    assert!(xml.contains(&format!("<additiv>{additive}</additiv>")));
                    let n = count.fetch_add(1, Ordering::SeqCst);
                    if scenario == "refused" || (scenario == "later-refusal" && n > 0) {
                        return common::body_error("463", "scripted refusal");
                    }
                    if scenario == "credentials" {
                        return common::api_error("135", "scripted credentials");
                    }
                    let mut current = stored.lock().expect("provider entries");
                    if !additive {
                        current.clear();
                    }
                    current.push("submitted");
                    match scenario {
                        "lost" => wiremock::ResponseTemplate::new(500),
                        "mismatch" => {
                            common::credited("PRIVATE-WRONG-NUMBER", "9007199254740994.01", "1.00")
                        }
                        "interrupted" | "later-refusal" if n == 0 => {
                            common::credited(&target, "9007199254740994.01", "1.00")
                                .set_delay(Duration::from_secs(20))
                        }
                        "cancelled" => {
                            wiremock::ResponseTemplate::new(500).set_delay(Duration::from_secs(2))
                        }
                        _ => common::credited(&target, "9007199254740994.01", "1.00"),
                    }
                })
                .mount(mock)
                .await;
            let input = body(&number, additive);
            let started = server
                .invoke(&call.send(), Some(&input), Some(&number))
                .await;
            if matches!(scenario, "interrupted" | "later-refusal" | "cancelled") {
                tokio::time::timeout(Duration::from_secs(10), async {
                    while sends.load(Ordering::SeqCst) == 0 {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                })
                .await
                .expect("provider received credit request");
                if scenario == "cancelled" {
                    server.admin().cancel(started.invocation_id()).await;
                } else {
                    // Replacement mode can overwrite an independently newer
                    // provider state when its unfinished execution is replayed.
                    if !additive && scenario == "interrupted" {
                        *entries.lock().expect("provider entries") = vec!["newer external entry"];
                    }
                    interrupt().await;
                }
            }
            server
                .admin()
                .await_status(started.invocation_id(), &["completed"])
                .await;
            let response = server.invoke(&call, Some(&input), Some(&number)).await;
            let expected = if matches!(scenario, "interrupted" | "later-refusal") {
                2
            } else {
                1
            };
            assert_eq!(sends.load(Ordering::SeqCst), expected, "{number}");
            if matches!(scenario, "success" | "interrupted") {
                assert_eq!(response.status, 200, "{number}: {response:?}");
                assert_eq!(response.body["gross_total"], "9007199254740994.01");
                assert_eq!(response.body["outstanding"], "1.00");
                let stored = entries.lock().expect("provider entries");
                assert_eq!(
                    *stored,
                    if additive && scenario == "interrupted" {
                        vec!["submitted", "submitted"]
                    } else {
                        vec!["submitted"]
                    }
                );
            } else {
                let fault = response.fault::<restate_szamlazz::contract::Fault>();
                assert_eq!(
                    fault.code,
                    if scenario == "credentials" {
                        restate_szamlazz::contract::TerminalCode::CredentialsRejected
                    } else {
                        restate_szamlazz::contract::TerminalCode::OutcomeUnknown
                    },
                    "{number}: {fault:?}"
                );
                assert!(!fault.message.contains("nothing was sent"));
                if scenario == "cancelled" {
                    assert_eq!(
                        fault.is_cancelled(),
                        Some(true),
                        "must observe cancellation, not merely the delayed HTTP failure"
                    );
                }
            }
            let before = mock.received_requests().await.expect("requests").len();
            assert_eq!(
                server.invoke(&call, Some(&input), Some(&number)).await.body,
                response.body
            );
            assert_eq!(
                mock.received_requests().await.expect("requests").len(),
                before,
                "completed answer replays without provider calls"
            );
            let journal = server.admin().journal(started.invocation_id()).await;
            if scenario != "cancelled" {
                let result = restate_e2e_harness::run_result(
                    &journal,
                    &format!("set-credit-entries-{number}"),
                )
                .expect("recorded credit exchange");
                assert!(
                    !result.raw_contains("CREDIT-PRIVATE-COMMENT"),
                    "operation projection must omit caller comment"
                );
            }
            for secret in [
                "PRIVATE-WRONG-NUMBER",
                "EXPERIMENT-NOT-A-REAL-KEY",
                "WORKER-ALPHA-TEST-KEY",
            ] {
                assert!(
                    journal.iter().all(|entry| !entry.raw_contains(secret)),
                    "{number}: journal leak"
                );
            }
            let mut observe = Call::object("Szamlazz.Order", &number, "observe_unresolved");
            if let Some(scope) = scope {
                observe = observe.scoped(scope);
            }
            assert_eq!(
                server.invoke(&observe, None, None).await.body["state"],
                "absent",
                "unkeyed Agent writes introduce no Order marker"
            );
        }
    }
}
