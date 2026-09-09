//! The one-shot writes (`delete_proforma`, `set_credit_entries`): every szamlazz.hu
//! answer as data, and the *Lost answer* (a send szamlazz.hu did not answer,
//! by transport or `szlahu_down`) as data too, since the step runs once.

use super::common::{
    api_error, body_error, created, credit, delete, proforma_deleted, proforma_gone, szlahu_down,
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

    assert_eq!(
        h.gateway.delete_proforma("D-1").await,
        DeleteOutcome::Deleted
    );
    assert_eq!(
        h.gateway.delete_proforma("D-2").await,
        DeleteOutcome::AlreadyGone
    );
    assert_eq!(
        h.gateway.delete_proforma("D-3").await,
        DeleteOutcome::CredentialsRejected(SzamlazzAnswer::new("3", "login"))
    );
    assert!(matches!(
        h.gateway.delete_proforma("D-4").await,
        DeleteOutcome::Lost(Unanswered::Transport(_))
    ));
    assert_eq!(
        h.gateway.delete_proforma("D-5").await,
        DeleteOutcome::Rejected(Rejection::from(SzamlazzAnswer::new("57", "malformed")))
    );
    assert!(
        matches!(
            h.gateway.delete_proforma("D-6").await,
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
