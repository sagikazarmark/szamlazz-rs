//! The storno lookup and the storno step: the storno of ours under its id,
//! the reversal established (reply heuristic or queried identity, the original's
//! `telj` and appearance on the wire), the echo as `NotStornoable`, the
//! leading query, the lost reply and its re-query.

use super::common::{
    Doc, api_error, body_error, created, created_without_totals, external_id_query, not_found,
    number_query, original_telj_tag, storno, szlahu_down,
};
use super::harness::*;
use restate_szamlazz::gateway::{
    QueryOutcome, Rejection, StornoLookupOutcome, StornoOutcome, StornoStepRequest, SzamlazzAnswer,
    Unanswered, Unconfirmed,
};
use rust_decimal::dec;
use wiremock::ResponseTemplate;

#[tokio::test]
async fn storno_lookup_finds_our_storno_under_the_id() {
    let h = Harness::start().await;
    let storno_id = storno_id();
    external_id_query(storno_id.as_str())
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-1"),
                ..Doc::new("SS-1", "SS")
            }
            .response(),
        )
        .expect(1)
        .mount(&h.server)
        .await;

    assert_eq!(
        h.gateway.lookup_storno(&storno_id, "SZ-1").await,
        Ok(StornoLookupOutcome::AlreadyReversed {
            storno_number: "SS-1".to_owned(),
        })
    );
    assert_eq!(h.bodies().await.len(), 1, "read-only: one query");
}

#[tokio::test]
async fn storno_lookup_is_absent_on_a_miss_or_another_holder() {
    // Code 7, and a holder that is not the storno of `SZ-1` (a storno is
    // idempotent server-side, so proceeding past a stray holder is safe).
    let h = Harness::start().await;
    let storno_id = storno_id();
    external_id_query(storno_id.as_str())
        .respond_with(not_found())
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.lookup_storno(&storno_id, "SZ-1").await,
        Ok(StornoLookupOutcome::Absent)
    );

    let h = Harness::start().await;
    external_id_query(storno_id.as_str())
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-9"),
                ..Doc::new("SS-9", "SS")
            }
            .response(),
        )
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.lookup_storno(&storno_id, "SZ-1").await,
        Ok(StornoLookupOutcome::Absent)
    );
}

#[tokio::test]
async fn storno_lookup_answers_another_code_as_data_and_no_answer_as_unanswered() {
    // Another szamlazz.hu code is an answer: data.
    let h = Harness::start().await;
    external_id_query(storno_id().as_str())
        .respond_with(body_error("57", "Ismeretlen hiba"))
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.lookup_storno(&storno_id(), "SZ-1").await,
        Ok(StornoLookupOutcome::Api(SzamlazzAnswer::new(
            "57",
            "Ismeretlen hiba"
        )))
    );

    // No answer is the read's retryable error.
    let h = Harness::start().await;
    let storno_id = storno_id();
    external_id_query(storno_id.as_str())
        .respond_with(ResponseTemplate::new(500))
        .mount(&h.server)
        .await;
    assert!(matches!(
        h.gateway.lookup_storno(&storno_id, "SZ-1").await,
        Err(Unanswered::Transport(_))
    ));
}

#[tokio::test]
async fn storno_reversed_is_validated() {
    let h = Harness::start().await;
    let storno_id = storno_id();
    external_id_query(storno_id.as_str())
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;
    storno()
        .respond_with(created("SS-1", "-1000", "-1270"))
        .expect(1)
        .mount(&h.server)
        .await;

    match h.gateway.storno(storno_request(&storno_id)).await {
        Ok(StornoOutcome::Reversed(storno)) => {
            assert_eq!(storno.number, "SS-1");
            assert_eq!(storno.gross_total, Some(dec!(-1270)));
            assert_eq!(storno.document_id, Some(924_307_747));
        }
        other => panic!("expected Reversed, got {other:?}"),
    }
    let body = &h.bodies().await[1];
    assert!(body.contains("<szamlaszam>SZ-1</szamlaszam>"));
    assert!(body.contains("<szamlaKulsoAzon>acct:ORD-1:storno:SZ-1</szamlaKulsoAzon>"));
    assert!(body.contains("<megjegyzes>wrong buyer</megjegyzes>"));
    assert!(body.contains("<eszamla>true</eszamla>"));
    assert!(
        body.contains(&original_telj_tag()),
        "the storno repeats the original's fulfillment date: {body}"
    );
    assert!(!body.contains("<keltDatum>"), "352 otherwise");
}

/// A storno of an e-invoice (`<eszamla>2</eszamla>` or `3` in the verified
/// original) goes out with `<eszamla>true</eszamla>`, one of a paper invoice
/// (`1`) with `false`: `Gateway::verify` reads the code, `e_invoice()` turns
/// it into the flag and `Gateway::storno` puts it on the wire unchanged. The
/// gateway's half of the derivation; `StornoIntent::from_verified` (the
/// handlers' half, with the account default for a code that is not an
/// invoice appearance) is unit-tested beside it. szamlazz.hu accepts a
/// mismatch silently and issues the storno in the *request's* form (P73), so
/// nothing downstream corrects a wrong flag.
#[tokio::test]
async fn storno_carries_the_verified_originals_appearance() {
    for (code, expected) in [(2, true), (3, true), (1, false)] {
        let h = Harness::start().await;
        let storno_id = storno_id();
        number_query("SZ-1")
            .respond_with(
                Doc {
                    eszamla: Some(code),
                    ..Doc::new("SZ-1", "SZ")
                }
                .response(),
            )
            .expect(1)
            .mount(&h.server)
            .await;
        external_id_query(storno_id.as_str())
            .respond_with(not_found())
            .expect(1)
            .mount(&h.server)
            .await;
        storno()
            .respond_with(created("SS-1", "-1000", "-1270"))
            .expect(1)
            .mount(&h.server)
            .await;

        let original = match h.gateway.verify("SZ-1").await {
            Ok(QueryOutcome::Found(document)) => document,
            other => panic!("eszamla {code}: expected Found, got {other:?}"),
        };
        let e_invoice = original
            .e_invoice()
            .unwrap_or_else(|| panic!("eszamla {code} is an invoice appearance"));
        assert_eq!(e_invoice, expected, "eszamla {code}");

        let request = StornoStepRequest {
            e_invoice,
            ..storno_request(&storno_id)
        };
        assert!(
            matches!(
                h.gateway.storno(request).await,
                Ok(StornoOutcome::Reversed(_))
            ),
            "eszamla {code}"
        );
        let body = &h.bodies().await[2];
        assert!(
            body.contains(&format!("<eszamla>{expected}</eszamla>")),
            "eszamla {code}: the storno is issued in the original's form: {body}"
        );
        assert!(body.contains(&original_telj_tag()), "eszamla {code}");
    }
}

#[tokio::test]
async fn synthetic_changed_number_zero_gross_retains_the_reversal_fast_path() {
    // Synthetic comparison control: retain the <= 0 policy. This does not
    // establish szamlazz.hu's acceptance of a zero-total original.
    let h = Harness::start().await;
    let storno_id = storno_id();
    external_id_query(storno_id.as_str())
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;
    storno()
        .respond_with(created("SS-1", "0", "0"))
        .expect(1)
        .mount(&h.server)
        .await;

    match h.gateway.storno(storno_request(&storno_id)).await {
        Ok(StornoOutcome::Reversed(storno)) => {
            assert_eq!(storno.number, "SS-1");
            assert_eq!(storno.gross_total, Some(dec!(0)));
        }
        other => panic!("expected Reversed, got {other:?}"),
    }
}

#[tokio::test]
async fn storno_echo_is_not_stornoable() {
    let h = Harness::start().await;
    let storno_id = storno_id();
    external_id_query(storno_id.as_str())
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;
    storno()
        .respond_with(created("SZ-1", "1000", "1270"))
        .expect(1)
        .mount(&h.server)
        .await;

    assert_eq!(
        h.gateway.storno(storno_request(&storno_id)).await,
        Ok(StornoOutcome::NotStornoable)
    );
    assert_eq!(h.bodies().await.len(), 2, "an echo needs no further query");
}

#[tokio::test]
async fn ambiguous_storno_reply_is_confirmed_by_its_document_identity() {
    // Synthetic allowed reply shapes, not evidence of live negative-original acceptance.
    for (reply, gross) in [
        (created_without_totals("SS-1"), None),
        (created("SS-1", "1000", "1270"), Some(dec!(1270))),
    ] {
        let h = Harness::start().await;
        let storno_id = storno_id();
        external_id_query(storno_id.as_str())
            .respond_with(not_found())
            .expect(1)
            .mount(&h.server)
            .await;
        storno()
            .respond_with(reply)
            .expect(1)
            .mount(&h.server)
            .await;
        number_query("SS-1")
            .respond_with(
                Doc {
                    referenced_invoice: Some("SZ-1"),
                    ..Doc::new("SS-1", "SS")
                }
                .response(),
            )
            .expect(1)
            .mount(&h.server)
            .await;

        match h.gateway.storno(storno_request(&storno_id)).await {
            Ok(StornoOutcome::Reversed(document)) => {
                assert_eq!(document.number, "SS-1");
                assert_eq!(
                    document.gross_total, gross,
                    "keep the reply's optional metadata"
                );
            }
            other => panic!("expected identity-confirmed Reversed, got {other:?}"),
        }
    }
}

#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "one table of inconclusive identity checks, each with and without successful reconciliation"
)]
async fn inconclusive_storno_identity_uses_external_id_reconciliation() {
    let matching = Doc {
        referenced_invoice: Some("SZ-1"),
        ..Doc::new("SS-1", "SS")
    };
    let checks = [
        (
            "wrong type",
            Doc {
                tipus: "HS",
                ..matching.clone()
            }
            .response(),
            "document type HS",
        ),
        (
            "wrong reference",
            Doc {
                referenced_invoice: Some("SZ-9"),
                ..matching.clone()
            }
            .response(),
            "SZ-9",
        ),
        (
            "missing reference",
            Doc {
                referenced_invoice: None,
                ..matching.clone()
            }
            .response(),
            "None",
        ),
        (
            "wrong number",
            Doc {
                number: "SS-9",
                ..matching.clone()
            }
            .response(),
            "SS-9",
        ),
        ("not found", not_found(), "code 7"),
        (
            "unanswered",
            ResponseTemplate::new(500),
            "transport failure",
        ),
        (
            "malformed",
            ResponseTemplate::new(200).set_body_string("<szamla>"),
            "transport failure",
        ),
        (
            "credentials",
            body_error("3", "login"),
            "credentials (3: credentials rejected: invalid credentials)",
        ),
        (
            "unavailable",
            szlahu_down(),
            "unavailable: query: szlahu_down",
        ),
        ("another code", body_error("57", "unknown"), "57: unknown"),
    ];
    for (label, verification, cause) in checks {
        for landed in [false, true] {
            let h = Harness::start().await;
            let storno_id = storno_id();
            external_id_query(storno_id.as_str())
                .respond_with(not_found())
                .up_to_n_times(1)
                .expect(1)
                .mount(&h.server)
                .await;
            external_id_query(storno_id.as_str())
                .respond_with(if landed {
                    matching.response()
                } else {
                    not_found()
                })
                .expect(1)
                .mount(&h.server)
                .await;
            storno()
                .respond_with(created_without_totals("SS-1"))
                .expect(1)
                .mount(&h.server)
                .await;
            number_query("SS-1")
                .respond_with(verification.clone())
                .expect(1)
                .mount(&h.server)
                .await;

            let outcome = h.gateway.storno(storno_request(&storno_id)).await;
            if landed {
                assert_eq!(
                    outcome,
                    Ok(StornoOutcome::AlreadyReversed {
                        storno_number: "SS-1".to_owned(),
                    }),
                    "{label}"
                );
            } else {
                match outcome {
                    Err(Unconfirmed::StornoVerification { number, message }) => {
                        assert_eq!(number, "SS-1", "{label}");
                        assert!(message.contains(cause), "{label}: {message}");
                    }
                    other => panic!("{label}: expected Unconfirmed, got {other:?}"),
                }
            }
        }
    }
}

#[tokio::test]
async fn post_send_storno_checks_preserve_uncertainty_and_both_causes() {
    for reconciliation in [body_error("135", "expired key"), szlahu_down()] {
        for reply in [created_without_totals("SS-1"), ResponseTemplate::new(500)] {
            let h = Harness::start().await;
            let storno_id = storno_id();
            external_id_query(storno_id.as_str())
                .respond_with(not_found())
                .up_to_n_times(1)
                .expect(1)
                .mount(&h.server)
                .await;
            external_id_query(storno_id.as_str())
                .respond_with(reconciliation.clone())
                .expect(1)
                .mount(&h.server)
                .await;
            storno()
                .respond_with(reply)
                .expect(1)
                .mount(&h.server)
                .await;
            number_query("SS-1")
                .respond_with(szlahu_down())
                .mount(&h.server)
                .await;

            let error = h
                .gateway
                .storno(storno_request(&storno_id))
                .await
                .expect_err("a post-send credential failure cannot settle the send");
            match error {
                Unconfirmed::ReQueryFailed { sent, re_query } => {
                    assert!(
                        sent.contains("transport failure")
                            || (sent.contains("SS-1")
                                && sent.contains("unavailable")
                                && sent.contains("szlahu_down")),
                        "preserve how the send or its identity check ended: {sent}"
                    );
                    assert!(
                        (re_query.contains("135") && re_query.contains("browser session active"))
                            || (re_query.contains("unavailable")
                                && re_query.contains("szlahu_down")),
                        "{re_query}"
                    );
                }
                other => panic!("expected both causes, got {other:?}"),
            }
        }
    }
}

#[tokio::test]
async fn storno_rejections_are_typed() {
    for (code, message) in [("14", "storno of storno"), ("221", "has corrective")] {
        let h = Harness::start().await;
        let storno_id = storno_id();
        external_id_query(storno_id.as_str())
            .respond_with(not_found())
            .mount(&h.server)
            .await;
        storno()
            .respond_with(api_error(code, message))
            .mount(&h.server)
            .await;
        assert_eq!(
            h.gateway.storno(storno_request(&storno_id)).await,
            Ok(StornoOutcome::Rejected(Rejection::from(
                SzamlazzAnswer::new(code.to_owned(), message.to_owned())
            )))
        );
    }
}

/// A storno step's outcome in one line, for the table below: the twin of
/// [`describe_create`].
fn describe_storno(outcome: &Result<StornoOutcome, Unconfirmed>) -> String {
    match outcome {
        Ok(StornoOutcome::Reversed(storno)) => format!("Reversed {}", storno.number),
        Ok(StornoOutcome::AlreadyReversed { storno_number }) => {
            format!("AlreadyReversed {storno_number}")
        }
        Ok(StornoOutcome::Api(answer)) => format!("Api {}", answer.code),
        Ok(StornoOutcome::Unavailable { message }) => format!("Unavailable {message}"),
        Ok(StornoOutcome::CredentialsRejected(answer)) => {
            format!("CredentialsRejected {}", answer.code)
        }
        other => format!("{other:?}"),
    }
}

/// The storno step's twin of the create table: what its **leading query** of
/// the storno external id decides, and whether a storno is sent. The decision
/// is `settle_storno`'s, unit-tested; an answer that is neither 7 nor a
/// credential code is settled data, nothing sent, nothing unconfirmed (#63).
#[tokio::test]
async fn the_storno_steps_leading_query_settles_or_proceeds() {
    let our_storno = Doc {
        referenced_invoice: Some("SZ-1"),
        ..Doc::new("SS-1", "SS")
    };
    let another_storno = Doc {
        referenced_invoice: Some("SZ-9"),
        ..Doc::new("SS-9", "SS")
    };
    // (what the leading query answers, stornos sent, the outcome)
    let rows: [(&str, ResponseTemplate, u64, &str); 6] = [
        ("a clean miss sends", not_found(), 1, "Reversed SS-1"),
        (
            "the storno of the original is already reversed",
            our_storno.response(),
            0,
            "AlreadyReversed SS-1",
        ),
        (
            "a holder that is not its storno is a miss (the server's storno is idempotent)",
            another_storno.response(),
            1,
            "Reversed SS-1",
        ),
        (
            "another code is data",
            body_error("57", "Ismeretlen hiba"),
            0,
            "Api 57",
        ),
        (
            "szlahu_down is data",
            szlahu_down(),
            0,
            "Unavailable query: szlahu_down",
        ),
        (
            "a credential code never sends",
            body_error("3", "login"),
            0,
            "CredentialsRejected 3",
        ),
    ];
    for (label, under_id, sends, expected) in rows {
        let h = Harness::start().await;
        let storno_id = storno_id();
        external_id_query(storno_id.as_str())
            .respond_with(under_id)
            .expect(1)
            .mount(&h.server)
            .await;
        storno()
            .respond_with(created("SS-1", "-1000", "-1270"))
            .expect(sends)
            .mount(&h.server)
            .await;

        let outcome = h.gateway.storno(storno_request(&storno_id)).await;
        assert_eq!(describe_storno(&outcome), expected, "{label}: {outcome:?}");
        assert_eq!(
            h.bodies().await.len(),
            1 + usize::try_from(sends).expect("0 or 1"),
            "{label}: the leading query, then the send or nothing"
        );
    }
}

#[tokio::test]
async fn storno_with_a_lost_reply_re_queries_once_and_is_unconfirmed_when_nothing_landed() {
    // An open code (55) and a lost reply (500): each is re-queried once,
    // immediately; nothing under the storno id leaves the step unconfirmed,
    // so the run retry policy re-executes it.
    let h = Harness::start().await;
    let storno_id = storno_id();
    external_id_query(storno_id.as_str())
        .respond_with(not_found())
        .expect(4)
        .mount(&h.server)
        .await;
    storno()
        .respond_with(api_error("55", "signing"))
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    storno()
        .respond_with(ResponseTemplate::new(500))
        .mount(&h.server)
        .await;

    assert_eq!(
        h.gateway.storno(storno_request(&storno_id)).await,
        Err(Unconfirmed::Open {
            code: Some("55".to_owned()),
            message: "signing".to_owned(),
        })
    );
    assert!(matches!(
        h.gateway.storno(storno_request(&storno_id)).await,
        Err(Unconfirmed::Transport(_))
    ));

    // Both executions built the storno from the same step request, so the
    // two sends are byte-identical, the date included.
    let bodies = h.bodies().await;
    assert_eq!(bodies.len(), 6, "query, storno, re-query; twice");
    assert_eq!(
        bodies[1], bodies[4],
        "the re-executed storno is byte-identical"
    );
    assert!(bodies[1].contains(&original_telj_tag()));
}

#[tokio::test]
async fn storno_re_executed_after_a_lost_reply_finds_the_storno_and_sends_nothing() {
    // The step, driven twice: the first execution's reply is lost, its
    // immediate re-query still sees nothing; the second execution's leading
    // query finds the storno that landed and sends nothing.
    let h = Harness::start().await;
    let storno_id = storno_id();
    external_id_query(storno_id.as_str())
        .respond_with(not_found())
        .up_to_n_times(2)
        .mount(&h.server)
        .await;
    external_id_query(storno_id.as_str())
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-1"),
                ..Doc::new("SS-1", "SS")
            }
            .response(),
        )
        .mount(&h.server)
        .await;
    storno()
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&h.server)
        .await;

    assert!(matches!(
        h.gateway.storno(storno_request(&storno_id)).await,
        Err(Unconfirmed::Transport(_))
    ));
    assert_eq!(
        h.gateway.storno(storno_request(&storno_id)).await,
        Ok(StornoOutcome::AlreadyReversed {
            storno_number: "SS-1".to_owned(),
        })
    );
    assert_eq!(h.bodies().await.len(), 4, "query, storno, re-query; query");
}

#[tokio::test]
async fn storno_lost_reply_whose_re_query_finds_the_storno_is_reversed() {
    // The reply is lost but the storno landed: the immediate re-query finds
    // the `SS` and settles the step without a second send.
    let h = Harness::start().await;
    let storno_id = storno_id();
    external_id_query(storno_id.as_str())
        .respond_with(not_found())
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    external_id_query(storno_id.as_str())
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-1"),
                ..Doc::new("SS-1", "SS")
            }
            .response(),
        )
        .mount(&h.server)
        .await;
    storno()
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&h.server)
        .await;

    assert_eq!(
        h.gateway.storno(storno_request(&storno_id)).await,
        Ok(StornoOutcome::AlreadyReversed {
            storno_number: "SS-1".to_owned(),
        })
    );
}
