//! The single-document reads (`verify`, `query`, `hint`): a document is
//! `Found`, an answer is data, no answer is `Unanswered`.

use super::common::{
    CreditRecord, Doc, ORIGINAL_TELJ, body_error, not_found, number_query, order_query,
};
use super::harness::*;
use restate_szamlazz::contract::Selector;
use restate_szamlazz::gateway::{QueryOutcome, SzamlazzAnswer, Unanswered};
use rust_decimal::dec;
use wiremock::ResponseTemplate;

#[tokio::test]
async fn verify_query_and_hint() {
    let h = Harness::start().await;
    number_query("SZ-1")
        .respond_with(
            Doc {
                credit_entries: &[CreditRecord::transfer("500"), CreditRecord::transfer("770")],
                ..Doc::new("SZ-1", "SZ")
            }
            .response(),
        )
        .mount(&h.server)
        .await;
    number_query("SZ-404")
        .respond_with(not_found())
        .mount(&h.server)
        .await;
    number_query("SZ-500")
        .respond_with(ResponseTemplate::new(500))
        .mount(&h.server)
        .await;
    number_query("SZ-57")
        .respond_with(body_error("57", "Ismeretlen hiba"))
        .mount(&h.server)
        .await;
    order_query("ORD-1")
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-1"),
                ..Doc::new("SS-1", "SS")
            }
            .response(),
        )
        .mount(&h.server)
        .await;

    match h.gateway.verify("SZ-1").await {
        Ok(QueryOutcome::Found(found)) => {
            assert_eq!(found.number, "SZ-1");
            assert_eq!(found.credit_entry_amounts(), vec![dec!(500), dec!(770)]);
            // The `telj` the storno handlers repeat.
            assert_eq!(found.fulfillment_date, Some(ORIGINAL_TELJ));
        }
        other => panic!("expected Found, got {other:?}"),
    }
    assert_eq!(h.gateway.verify("SZ-404").await, Ok(QueryOutcome::NotFound));
    // A lost reply is the retryable error of the read, never data.
    assert!(matches!(
        h.gateway.verify("SZ-500").await,
        Err(Unanswered::Transport(_))
    ));
    // Another szamlazz.hu code is an answer: data.
    assert_eq!(
        h.gateway.verify("SZ-57").await,
        Ok(QueryOutcome::Api(SzamlazzAnswer::new(
            "57",
            "Ismeretlen hiba"
        )))
    );
    match h.gateway.hint(&order()).await {
        Ok(QueryOutcome::Found(found)) => {
            assert_eq!(found.document_type, szamlazz_agent::DocumentType::Storno);
            assert!(found.is_storno_of("SZ-1"));
        }
        other => panic!("expected Found, got {other:?}"),
    }
    match h
        .gateway
        .query(&Selector::InvoiceNumber(
            "SZ-1".parse().expect("valid number"),
        ))
        .await
    {
        Ok(QueryOutcome::Found(found)) => {
            assert_eq!(found.number, "SZ-1");
            assert_eq!(found.credit_entries.len(), 2);
        }
        other => panic!("expected Found, got {other:?}"),
    }
    assert_eq!(
        h.gateway
            .query(&Selector::InvoiceNumber(
                "SZ-404".parse().expect("valid number")
            ))
            .await,
        Ok(QueryOutcome::NotFound)
    );
    // No mock matches the external id query: wiremock answers 404 with an
    // empty body, which the agent crate reports as a parse failure.
    assert!(matches!(
        h.gateway
            .query(&Selector::ExternalId("acct:ORD-1:invoice".to_owned()))
            .await,
        Err(Unanswered::Transport(_))
    ));

    // The hint alone: a lost reply is its `Unanswered` too.
    let h = Harness::start().await;
    order_query("ORD-1")
        .respond_with(ResponseTemplate::new(500))
        .mount(&h.server)
        .await;
    assert!(matches!(
        h.gateway.hint(&order()).await,
        Err(Unanswered::Transport(_))
    ));
}

#[tokio::test]
async fn verify_of_a_document_without_telj_has_no_fulfillment_date() {
    // szamlazz.hu's schema has `telj` mandatory; a document without the
    // element still parses: the gateway reports the fact, and the storno
    // handlers refuse to send on it.
    let h = Harness::start().await;
    number_query("SZ-1")
        .respond_with(
            Doc {
                fulfillment_date: None,
                ..Doc::new("SZ-1", "SZ")
            }
            .response(),
        )
        .mount(&h.server)
        .await;
    match h.gateway.verify("SZ-1").await {
        Ok(QueryOutcome::Found(found)) => {
            assert_eq!(found.number, "SZ-1");
            assert_eq!(found.fulfillment_date, None);
        }
        other => panic!("expected Found, got {other:?}"),
    }
}
