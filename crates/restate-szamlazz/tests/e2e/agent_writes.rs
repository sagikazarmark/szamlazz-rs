//! The `Szamlazz.Agent` writes under the multi-account deployment, each on
//! the scoped account: `storno` of an unmanaged document through its path
//! (verify, the by-number storno lookup, the storno step) with the
//! original's `telj` repeated and that account's key on the wire, and
//! `set_credit_entries` through its one step with the flag, the entries and the key
//! on the wire and the totals answered. The verdicts (`managed_by_order`, a
//! `telj`-less original, the storno's `eszamla`), the refusals (a sixth
//! entry, an empty replace) and the lost-reply advice are unit tests of
//! `service::agent` and the storno intent; the wire shapes of both
//! operations are `tests/gateway/`'s.

use serde_json::json;
use wiremock::matchers::body_string_contains;

use crate::harness::Harness;
use crate::harness::accounts::AGENT_KEY;
use crate::harness::szamlazz::{
    Doc, agent_key_tag, created, credit_of, credited, external_id_query, holds, not_found,
    number_query, original_telj_tag, storno_of, storno_of_number_repeating_telj,
};

/// Inconclusive credit-entry answers never claim refusal and never repeat the
/// one-shot write; the same ingress key replays the stored fault in both modes.
pub(crate) async fn inconclusive_credit_entry_answers_are_stored_unknown_outcomes(h: &Harness) {
    use crate::common::body_error;
    use restate_e2e_harness::Call;
    use restate_szamlazz::contract::TerminalCode;
    h.reset().await;
    for additive in [false, true] {
        for code in ["99999", "", "1", "57", "463", "3"] {
            let number = format!("SZ-OPEN-{}-{code}", u8::from(additive));
            credit_of(&number)
                .respond_with(body_error(code, "vendor cause"))
                .expect(1)
                .mount(&h.mock)
                .await;
            let call = Call::service("Szamlazz.Agent", "set_credit_entries").scoped("acme");
            let body = json!({"invoice_number": number, "entries": [{"date": "2026-09-05", "title": "transfer", "amount": "1"}], "additive": additive});
            let key = format!("credit-{number}");
            let reply = h.invoke(&call, Some(&body), Some(&key)).await;
            let fault = reply.fault();
            assert_eq!(
                fault.szamlazz_code.as_deref(),
                Some(if code.is_empty() { "absent" } else { code })
            );
            assert!(
                fault.message.contains(if code == "3" {
                    "credentials rejected"
                } else {
                    "vendor cause"
                }),
                "{fault:?}"
            );
            match code {
                "57" | "463" => {
                    assert_eq!(reply.status, 422);
                    assert_eq!(fault.code, TerminalCode::SzamlazzError);
                }
                "3" => {
                    assert_eq!(reply.status, 503);
                    assert_eq!(fault.code, TerminalCode::CredentialsRejected);
                }
                _ => {
                    assert_eq!(reply.status, 500);
                    assert_eq!(fault.code, TerminalCode::OutcomeUnknown);
                    assert!(!fault.message.contains("refused"), "{fault:?}");
                    assert!(
                        fault.message.contains(if additive {
                            "send only those entries"
                        } else {
                            "current intended snapshot"
                        }),
                        "{fault:?}"
                    );
                }
            }
            let stored = h.invoke(&call, Some(&body), Some(&key)).await;
            assert_eq!(stored.invocation_id(), reply.invocation_id());
            assert_eq!(stored.fault(), fault);
            assert_eq!(
                h.requests_mentioning(&format!("<szamlaszam>{number}</szamlaszam>"))
                    .await
                    .len(),
                1,
                "one send, no query or repeat"
            );
        }
    }
}

/// `Szamlazz.Agent.storno` under `acme` reverses a document carrying no order
/// number through `verify-original-{number}`, `lookup-storno-{number}` and
/// `storno-{number}`, the storno carrying the original's `telj` and `acme`'s
/// key and no `keltDatum`; `Szamlazz.Agent.set_credit_entries` under `acme` puts
/// `<additiv>false</additiv>` (replacing), the entries as sent and `acme`'s
/// key on the wire in its one `set-credit-entries-{number}` step with no query
/// before it, and answers the invoice's totals as szamlazz.hu reported them,
/// `outstanding` distinct from `gross_total`.
pub(crate) async fn agent_storno_and_set_credit_entries_run_on_the_scoped_account(h: &Harness) {
    h.reset().await;
    holds(&h.mock, &Doc::unmanaged("SZ-23", "SZ")).await;
    external_id_query("acct:by-number:SZ-23:storno")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    storno_of_number_repeating_telj("SZ-23")
        .and(body_string_contains(agent_key_tag(AGENT_KEY)))
        .respond_with(created("SS-23", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    // Synthetic positive-gross reply: confirm by identity, not arithmetic.
    number_query("SS-23")
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-23"),
                ..Doc::unmanaged("SS-23", "SS")
            }
            .response(),
        )
        .expect(1)
        .mount(&h.mock)
        .await;
    credit_of("SZ-50")
        .and(body_string_contains("<additiv>false</additiv>"))
        .and(body_string_contains(agent_key_tag(AGENT_KEY)))
        .and(body_string_contains("<osszeg>1000</osszeg>"))
        .and(body_string_contains("<jogcim>átutalás</jogcim>"))
        .and(body_string_contains("<leiras>first instalment</leiras>"))
        .respond_with(credited("SZ-50", "1270", "270"))
        .expect(1)
        .mount(&h.mock)
        .await;

    let reply = h
        .call_agent_scoped("acme", "storno", &storno_of("SZ-23"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "reversed", "{}", reply.body);
    assert_eq!(reply.body["storno_number"], "SS-23", "{}", reply.body);
    assert_eq!(
        h.admin().runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "verify-original-SZ-23",
            "lookup-storno-SZ-23",
            "storno-SZ-23"
        ]
    );
    let invocation = h.admin().invocation(reply.invocation_id()).await;
    assert_eq!(invocation.scope.as_deref(), Some("acme"), "{invocation:?}");
    let stornos = h.storno_bodies_of("SZ-23").await;
    assert_eq!(stornos.len(), 1);
    assert!(stornos[0].contains(&original_telj_tag()), "{}", stornos[0]);
    assert!(!stornos[0].contains("<keltDatum>"), "{}", stornos[0]);

    let reply = h
        .call_agent_scoped(
            "acme",
            "set_credit_entries",
            &json!({
                "invoice_number": "SZ-50",
                "entries": [{
                    "date": "2026-09-05",
                    "title": "transfer",
                    "amount": "1000",
                    "comment": "first instalment",
                }],
                "additive": false,
            }),
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "SZ-50", "{}", reply.body);
    assert_eq!(reply.body["outstanding"], "270", "{}", reply.body);
    assert_eq!(reply.body["gross_total"], "1270", "{}", reply.body);
    assert_eq!(
        h.admin().runs(reply.invocation_id()).await,
        ["namespace", "account", "set-credit-entries-SZ-50"],
        "one step, no query before it"
    );
    assert_eq!(
        h.requests_mentioning("<szamlaszam>SZ-50</szamlaszam>")
            .await
            .len(),
        1,
        "the one send, nothing read"
    );
}

/// Both credit-entry modes can land before cancellation. The fault is stored
/// under the same retry identity; its advice distinguishes an additive send
/// from replacing the current intended snapshot. No re-query or resend is
/// performed inside either one-shot step.
pub(crate) async fn cancelled_credit_entries_are_unknown_with_mode_specific_guidance(h: &Harness) {
    use restate_e2e_harness::Call;
    use std::{sync::Arc, time::Duration};

    h.reset().await;
    for (number, additive, key) in [
        ("SZ-CANCEL-ADD", true, "cancel-credit-add"),
        ("SZ-CANCEL-REPLACE", false, "cancel-credit-replace"),
    ] {
        let received = Arc::new(tokio::sync::Notify::new());
        let signal = Arc::clone(&received);
        credit_of(number)
            .respond_with(move |_: &wiremock::Request| {
                signal.notify_one();
                credited(number, "1270", "270").set_delay(Duration::from_secs(4))
            })
            .expect(1)
            .mount(&h.mock)
            .await;
        let call = Call::service("Szamlazz.Agent", "set_credit_entries").scoped("acme");
        let body = json!({
            "invoice_number": number,
            "entries": [{"date": "2026-09-05", "title": "transfer", "amount": "1000"}],
            "additive": additive,
        });
        let reply = crate::policies::cancel_after_send(h, call, &body, key, &received).await;
        let fault = reply.fault();
        let guidance = if additive {
            "query the invoice before re-sending"
        } else {
            "current intended snapshot and a new Idempotency-Key"
        };
        assert!(fault.message.contains(guidance), "{fault:?}");
        assert_eq!(
            h.admin().runs(reply.invocation_id()).await,
            [
                "namespace".to_owned(),
                "account".to_owned(),
                format!("set-credit-entries-{number}")
            ]
        );
        let stored = h.invoke(&call, Some(&body), Some(key)).await;
        assert_eq!(stored.invocation_id(), reply.invocation_id());
        assert_eq!(stored.fault(), fault);
        assert_eq!(
            h.requests_mentioning(&format!("<szamlaszam>{number}</szamlaszam>"))
                .await
                .len(),
            1,
            "one send and no read, including after replay"
        );
    }
    // Invalid reported order numbers stop after the verify, as data. This
    // also proves the new open token survives the real Restate response path.
    holds(&h.mock, &Doc::of("SZ-UNSUPPORTED", "SZ", "legacy:order")).await;
    let reply = h
        .call_agent_scoped("acme", "storno", &storno_of("SZ-UNSUPPORTED"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "unsupported_order_number");
    assert_eq!(reply.body["order_key"], "legacy:order");
    assert_eq!(
        h.admin().runs(reply.invocation_id()).await,
        ["namespace", "account", "verify-original-SZ-UNSUPPORTED"]
    );
    assert!(h.storno_bodies_of("SZ-UNSUPPORTED").await.is_empty());
    h.mock.verify().await;
}
