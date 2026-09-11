//! The one-shot writes (`delete_proforma`, `set_credit_entries`): every szamlazz.hu
//! answer as data, and the *Lost answer* (a send szamlazz.hu did not answer,
//! by transport or `szlahu_down`) as data too, since the step runs once.

use super::common::{
    CreditRecord, Doc, api_error, body_error, created, credit, delete, number_query,
    proforma_deleted, proforma_gone, szlahu_down,
};
use super::harness::*;
use jiff::civil::date;
use restate_szamlazz::contract::{CreditEntryInput, PaymentMethod};
use restate_szamlazz::gateway::{
    DeleteOutcome, Rejection, RejectionCode, SetCreditEntriesOutcome, SzamlazzAnswer, Unanswered,
};
use rust_decimal::dec;
use wiremock::ResponseTemplate;
use wiremock::matchers::body_string_contains;

#[tokio::test]
async fn deletion_checks_fresh_credit_entries_on_the_pinned_number() {
    let h = Harness::start().await;
    let pinned = project(&Doc::new("D-PINNED", "D"));
    number_query("D-PINNED")
        .respond_with(
            Doc {
                credit_entries: &[CreditRecord::transfer("1")],
                ..Doc::new("D-PINNED", "D")
            }
            .response(),
        )
        .mount(&h.server)
        .await;
    delete()
        .respond_with(proforma_deleted())
        .expect(0)
        .mount(&h.server)
        .await;

    assert_eq!(
        h.gateway.delete_proforma(&pinned, &order(), false).await,
        DeleteOutcome::Paid,
    );
}

#[tokio::test]
async fn deletion_never_reselects_a_replacement_or_bypasses_identity_with_force() {
    use super::common::{external_id_query, not_found};
    let pinned = project(&Doc::new("D-PINNED", "D"));
    let changed_id = Doc::new("D-PINNED", "D")
        .xml()
        .replace("924307338", "924307339");
    for force in [false, true] {
        for (response, expected) in [
            (
                Doc::new("D-OTHER", "D").response(),
                DeleteOutcome::GuardFailed(Unanswered::Transport(
                    "query: document number differs from the requested number".to_owned(),
                )),
            ),
            (
                Doc::of("D-PINNED", "D", "ORD-OTHER").response(),
                DeleteOutcome::TargetChanged,
            ),
            (
                Doc::new("D-PINNED", "SZ").response(),
                DeleteOutcome::TargetChanged,
            ),
            (
                ResponseTemplate::new(200).set_body_string(changed_id.clone()),
                DeleteOutcome::TargetChanged,
            ),
            (not_found(), DeleteOutcome::AlreadyGone),
            (
                api_error("57", "query refused"),
                DeleteOutcome::Api(SzamlazzAnswer::new("57", "query refused")),
            ),
            (
                api_error("3", "login"),
                DeleteOutcome::CredentialsRejected(SzamlazzAnswer::new("3", "login")),
            ),
        ] {
            let h = Harness::start().await;
            number_query("D-PINNED")
                .respond_with(response)
                .expect(1)
                .mount(&h.server)
                .await;
            external_id_query("acct:ORD-1:proforma")
                .respond_with(Doc::new("D-REPLACEMENT", "D").response())
                .expect(0)
                .mount(&h.server)
                .await;
            delete()
                .respond_with(proforma_deleted())
                .expect(0)
                .mount(&h.server)
                .await;
            assert_eq!(
                h.gateway.delete_proforma(&pinned, &order(), force).await,
                expected,
                "force={force}"
            );
        }
    }
    let h = Harness::start().await;
    number_query("D-PINNED")
        .respond_with(
            Doc {
                credit_entries: &[CreditRecord::transfer("1270")],
                ..Doc::new("D-PINNED", "D")
            }
            .response(),
        )
        .mount(&h.server)
        .await;
    external_id_query("acct:ORD-1:proforma")
        .respond_with(Doc::new("D-REPLACEMENT", "D").response())
        .expect(0)
        .mount(&h.server)
        .await;
    super::common::delete_of("D-PINNED")
        .respond_with(proforma_deleted())
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.delete_proforma(&pinned, &order(), true).await,
        DeleteOutcome::Deleted
    );
}

#[tokio::test]
async fn a_failed_fresh_read_is_data_and_sends_no_delete() {
    for response in [ResponseTemplate::new(500), szlahu_down()] {
        let h = Harness::start().await;
        number_query("D-PINNED")
            .respond_with(response)
            .expect(1)
            .mount(&h.server)
            .await;
        delete()
            .respond_with(proforma_deleted())
            .expect(0)
            .mount(&h.server)
            .await;
        let outcome = h
            .gateway
            .delete_proforma(&project(&Doc::new("D-PINNED", "D")), &order(), true)
            .await;
        assert!(
            matches!(outcome, DeleteOutcome::GuardFailed(_)),
            "{outcome:?}"
        );
    }
}

/// Vendor XML-input refusals are shared (53/57); 463 is observed for credit
/// registration and 335 for deletion. Invoice-creation codes are not evidence
/// of refusal of either one-shot write. No post-action error was live-probed.
#[tokio::test]
async fn one_shot_answers_require_operation_specific_refusal_evidence() {
    let entry = CreditEntryInput::new(date(2026, 9, 3), PaymentMethod::Transfer, dec!(1));
    for code in [
        "53", "57", "335", "463", "1", "55", "56", "71", "259", "99999", "FUTURE", "",
    ] {
        let h = Harness::start().await;
        delete().respond_with(ResponseTemplate::new(200).set_body_string(format!(
            "<xmlszamladbkdelvalasz xmlns=\"http://www.szamlazz.hu/xmlszamladbkdelvalasz\"><sikeres>false</sikeres><hibakod>{code}</hibakod><hibauzenet>cause retained</hibauzenet></xmlszamladbkdelvalasz>"
        ))).expect(1).mount(&h.server).await;
        credit()
            .respond_with(body_error(code, "cause retained"))
            .expect(1)
            .mount(&h.server)
            .await;
        let answer = SzamlazzAnswer::new(
            if code.is_empty() { "absent" } else { code },
            "cause retained",
        );
        let deletion = h.delete("D-1").await;
        let registration = h
            .gateway
            .set_credit_entries("SZ-1", std::slice::from_ref(&entry), true)
            .await;
        let expected_delete = match code {
            "53" | "57" => DeleteOutcome::Rejected(Rejection::from(answer.clone())),
            "335" => DeleteOutcome::AlreadyGone,
            _ => DeleteOutcome::Inconclusive(answer.clone()),
        };
        let expected_credit = match code {
            "53" | "57" | "463" => SetCreditEntriesOutcome::Rejected(Rejection::from(answer)),
            _ => SetCreditEntriesOutcome::Inconclusive(answer),
        };
        assert_eq!(deletion, expected_delete, "delete code {code}");
        assert_eq!(registration, expected_credit, "credit code {code}");
        assert_eq!(
            h.bodies().await.len(),
            3,
            "one fresh read and two one-shot sends"
        );
    }
}

#[tokio::test]
async fn delete_proforma_outcomes() {
    let h = Harness::start().await;
    delete()
        .and(body_string_contains("<szamlaszam>D-1</szamlaszam>"))
        .respond_with(proforma_deleted())
        .mount(&h.server)
        .await;
    delete()
        .and(body_string_contains("<szamlaszam>D-2</szamlaszam>"))
        .respond_with(proforma_gone())
        .mount(&h.server)
        .await;
    delete()
        .and(body_string_contains("<szamlaszam>D-3</szamlaszam>"))
        .respond_with(api_error("3", "login"))
        .mount(&h.server)
        .await;
    delete()
        .and(body_string_contains("<szamlaszam>D-4</szamlaszam>"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&h.server)
        .await;
    delete()
        .and(body_string_contains("<szamlaszam>D-6</szamlaszam>"))
        .respond_with(szlahu_down())
        .mount(&h.server)
        .await;
    delete()
        .and(body_string_contains("<szamlaszam>D-5</szamlaszam>"))
        .respond_with(api_error("57", "malformed"))
        .mount(&h.server)
        .await;

    assert_eq!(h.delete("D-1").await, DeleteOutcome::Deleted);
    assert_eq!(h.delete("D-2").await, DeleteOutcome::AlreadyGone);
    assert_eq!(
        h.delete("D-3").await,
        DeleteOutcome::CredentialsRejected(SzamlazzAnswer::new("3", "login"))
    );
    assert!(matches!(
        h.delete("D-4").await,
        DeleteOutcome::Lost(Unanswered::Transport(_))
    ));
    assert_eq!(
        h.delete("D-5").await,
        DeleteOutcome::Rejected(Rejection::from(SzamlazzAnswer::new("57", "malformed")))
    );
    assert!(
        matches!(
            h.delete("D-6").await,
            DeleteOutcome::Lost(Unanswered::Unavailable(_))
        ),
        "szlahu_down after a send is a lost answer of its own kind, not a transport failure"
    );
}

#[tokio::test]
async fn set_credit_entries_outcomes() {
    let h = Harness::start().await;
    credit()
        .and(body_string_contains("<szamlaszam>SZ-1</szamlaszam>"))
        .respond_with(created("SZ-1", "1000", "1270").insert_header("szlahu_kintlevoseg", "270"))
        .mount(&h.server)
        .await;
    credit()
        .and(body_string_contains("<szamlaszam>SZ-2</szamlaszam>"))
        .respond_with(body_error("463", "reversed"))
        .mount(&h.server)
        .await;
    credit()
        .and(body_string_contains("<szamlaszam>SZ-3</szamlaszam>"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&h.server)
        .await;
    credit()
        .and(body_string_contains("<szamlaszam>SZ-4</szamlaszam>"))
        .respond_with(szlahu_down())
        .mount(&h.server)
        .await;
    let entry = CreditEntryInput {
        date: date(2026, 9, 3),
        title: PaymentMethod::Card,
        amount: dec!(1000),
        comment: Some("card".to_owned()),
    };

    match h
        .gateway
        .set_credit_entries("SZ-1", std::slice::from_ref(&entry), true)
        .await
    {
        SetCreditEntriesOutcome::Done { outstanding, gross } => {
            // The body's <kintlevoseg> takes precedence over the header.
            assert_eq!(outstanding, Some(dec!(1270)));
            assert_eq!(gross, Some(dec!(1270)));
        }
        other => panic!("expected Done, got {other:?}"),
    }
    let body = &h.bodies().await[0];
    assert!(body.contains("<additiv>true</additiv>"));
    assert!(body.contains("<jogcim>bankkártya</jogcim>"));
    assert!(body.contains("<osszeg>1000</osszeg>"));
    assert!(body.contains("<leiras>card</leiras>"));

    assert_eq!(
        h.gateway
            .set_credit_entries("SZ-2", std::slice::from_ref(&entry), false)
            .await,
        SetCreditEntriesOutcome::Rejected(Rejection::from(SzamlazzAnswer::new("463", "reversed")))
    );
    assert!(matches!(
        h.gateway
            .set_credit_entries("SZ-3", std::slice::from_ref(&entry), false)
            .await,
        SetCreditEntriesOutcome::Lost(Unanswered::Transport(_))
    ));
    assert!(matches!(
        h.gateway
            .set_credit_entries("SZ-4", std::slice::from_ref(&entry), false)
            .await,
        SetCreditEntriesOutcome::Lost(Unanswered::Unavailable(_))
    ));
    let six = vec![entry; 6];
    assert!(matches!(
        h.gateway.set_credit_entries("SZ-9", &six, false).await,
        SetCreditEntriesOutcome::Rejected(Rejection {
            code: RejectionCode::Request,
            ..
        })
    ));
    // A replacing call with no entries would clear the invoice's credit entries:
    // refused by the agent crate before the wire, the caller's request.
    match h.gateway.set_credit_entries("SZ-9", &[], false).await {
        SetCreditEntriesOutcome::Rejected(Rejection { code, message, .. }) => {
            assert_eq!(code, RejectionCode::Request);
            assert_eq!(code.as_str(), RejectionCode::REQUEST);
            assert!(message.contains("at least one entry"), "{message}");
        }
        other => panic!("expected Rejected, got {other:?}"),
    }
    assert_eq!(
        h.bodies().await.len(),
        4,
        "six entries and an empty replace never reach the wire"
    );
}
