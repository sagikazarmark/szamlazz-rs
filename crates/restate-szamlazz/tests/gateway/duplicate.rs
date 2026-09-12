//! The *Create step* answered 71/152 (a duplicate order number): the
//! external-id re-query settles it (reconciled, a collision, a reversal the
//! lookup did not see) or the order-number query names the existing document;
//! nothing under the order is settled without a number; never a re-send.

use super::common::{Doc, api_error, create, external_id_query, not_found, order_query};
use super::harness::*;
use restate_szamlazz::ExternalId;
use restate_szamlazz::contract::IssuedKind;
use restate_szamlazz::gateway::{CreateOutcome, Rejection, SzamlazzAnswer};
use wiremock::ResponseTemplate;

const DUPLICATE_MESSAGE: &str = "M%C3%A1r+l%C3%A9tez%C5%91+rendel%C3%A9ssz%C3%A1m";

/// The leading query misses, the create answers 152, and the re-query sees
/// `under_id`.
async fn duplicate_harness(under_id: ResponseTemplate) -> Harness {
    duplicate_harness_after(not_found(), under_id).await
}

async fn duplicate_harness_after(leading: ResponseTemplate, under_id: ResponseTemplate) -> Harness {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(leading)
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(under_id)
        .mount(&h.server)
        .await;
    create()
        .respond_with(api_error("152", DUPLICATE_MESSAGE))
        .expect(1)
        .mount(&h.server)
        .await;
    h
}

#[tokio::test]
async fn duplicate_order_number_with_our_live_document_under_the_id_is_reconciled() {
    let h = duplicate_harness(Doc::new("SZ-3", "SZ").response()).await;
    order_query("ORD-1")
        .respond_with(not_found())
        .expect(0)
        .mount(&h.server)
        .await;

    match h.create(None).await {
        Ok(CreateOutcome::Reconciled(found)) => assert_eq!(found.number, "SZ-3"),
        other => panic!("expected Reconciled, got {other:?}"),
    }
}

#[tokio::test]
async fn duplicate_order_number_with_an_invalid_document_under_the_id_is_a_collision() {
    let h = duplicate_harness(
        Doc {
            order: Some("ORD-2"),
            ..Doc::new("SZ-9", "SZ")
        }
        .response(),
    )
    .await;

    match h.create(None).await {
        Ok(CreateOutcome::Collision(found)) => assert_eq!(found.number, "SZ-9"),
        other => panic!("expected Collision, got {other:?}"),
    }
}

#[tokio::test]
async fn duplicate_order_number_with_a_reversal_the_lookup_did_not_see_is_reversed() {
    // The re-query after 152 finds a reversed document of ours that the
    // lookup did not see: answered as `Reversed` like the leading query
    // would, without the order-number naming query.
    let h = duplicate_harness(Doc::reversed("SZ-3", "SZ").response()).await;
    order_query("ORD-1")
        .respond_with(not_found())
        .expect(0)
        .mount(&h.server)
        .await;

    match h.create(None).await {
        Ok(CreateOutcome::Reversed(found)) => assert_eq!(found.number, "SZ-3"),
        other => panic!("expected Reversed, got {other:?}"),
    }
}

#[tokio::test]
async fn duplicate_order_number_names_the_existing_document_when_our_kind_is_newest() {
    // Nothing under the id, or only our reversed document (a reissue): the
    // live duplicate is not ours; the order-number query names it when it is
    // a live document of the kind being issued.
    let absent = (not_found(), None);
    let reversed = (Doc::reversed("SZ-1", "SZ").response(), Some("SZ-1"));
    for (label, (under_id, reversed)) in [("absent", absent), ("reversed", reversed)] {
        let h = duplicate_harness_after(under_id.clone(), under_id).await;
        order_query("ORD-1")
            .respond_with(Doc::new("SZ-77", "SZ").response())
            .expect(1)
            .mount(&h.server)
            .await;

        assert_eq!(
            h.create(reversed).await,
            Ok(CreateOutcome::DuplicateOrderNumber {
                answer: SzamlazzAnswer::new("152", "Már létező rendelésszám"),
                existing_number: Some("SZ-77".to_owned()),
            }),
            "{label}"
        );
    }
}

#[tokio::test]
async fn duplicate_order_number_has_no_existing_number_when_another_kind_is_newest() {
    // The newest document under the order is a proforma, a storno, or a
    // reversed document of our kind: none of them is the live duplicate.
    let proforma = Doc::new("D-1", "D");
    let storno = Doc {
        referenced_invoice: Some("SZ-0"),
        ..Doc::new("SS-0", "SS")
    };
    let reversed_of_our_kind = Doc::reversed("SZ-0", "SZ");
    for (label, newest) in [
        ("proforma", proforma),
        ("storno", storno),
        ("reversed", reversed_of_our_kind),
    ] {
        let h = duplicate_harness(not_found()).await;
        order_query("ORD-1")
            .respond_with(newest.response())
            .mount(&h.server)
            .await;

        assert_eq!(
            h.create(None).await,
            Ok(CreateOutcome::DuplicateOrderNumber {
                answer: SzamlazzAnswer::new("152", "Már létező rendelésszám"),
                existing_number: None,
            }),
            "{label}"
        );
    }
}

#[tokio::test]
async fn duplicate_order_number_with_nothing_under_the_order_is_settled_without_a_number() {
    // szamlazz.hu refused the order number, yet knows nothing under it: a
    // contradiction, but still a refusal, settled on the first occurrence
    // (the harness's create mock expects exactly one send), without a number
    // to name.
    let h = duplicate_harness(not_found()).await;
    order_query("ORD-1")
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;

    assert_eq!(
        h.create(None).await,
        Ok(CreateOutcome::DuplicateOrderNumber {
            answer: SzamlazzAnswer::new("152", "Már létező rendelésszám"),
            existing_number: None,
        })
    );
}

/// With one admitted send and no earlier unresolved write, 152 settles the
/// refusal. Failure of the optional diagnostic read cannot erase that answer.
#[tokio::test]
async fn duplicate_order_number_whose_re_query_fails_remains_a_refusal() {
    let h = duplicate_harness(ResponseTemplate::new(500)).await;
    order_query("ORD-1")
        .respond_with(not_found())
        .expect(0)
        .mount(&h.server)
        .await;

    assert_eq!(
        h.create(None).await,
        Ok(CreateOutcome::DuplicateOrderNumber {
            answer: SzamlazzAnswer::new("152", "Már létező rendelésszám"),
            existing_number: None,
        })
    );
    assert_eq!(h.bodies().await.len(), 3, "query, create, re-query");
}

#[tokio::test]
async fn duplicate_refusal_survives_diagnostic_codes_and_hint_failure() {
    for hint in [false, true] {
        for failed in [
            api_error("135", "private credentials"),
            api_error("57", "bad XML"),
            ResponseTemplate::new(500),
        ] {
            let h = duplicate_harness(if hint { not_found() } else { failed.clone() }).await;
            if hint {
                order_query("ORD-1")
                    .respond_with(failed)
                    .expect(1)
                    .mount(&h.server)
                    .await;
            }
            assert_eq!(
                h.create(None).await,
                Ok(CreateOutcome::DuplicateOrderNumber {
                    answer: SzamlazzAnswer::new("152", "Már létező rendelésszám"),
                    existing_number: None,
                })
            );
            assert_eq!(h.bodies().await.len(), if hint { 4 } else { 3 });
            h.server.verify().await;
        }
    }
    let h = Harness::start().await;
    let id = ExternalId::new("acct:ORD-1:corrective:c1");
    external_id_query(id.as_str())
        .respond_with(not_found())
        .up_to_n_times(1)
        .expect(1)
        .mount(&h.server)
        .await;
    external_id_query(id.as_str())
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&h.server)
        .await;
    create()
        .respond_with(api_error("71", "duplicate"))
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.create_kind(IssuedKind::Corrective, &id, None).await,
        Ok(CreateOutcome::Rejected(Rejection::from(
            SzamlazzAnswer::new("71", "duplicate")
        )))
    );
    assert_eq!(h.bodies().await.len(), 3);
    h.server.verify().await;
}

#[tokio::test]
async fn duplicate_order_number_on_a_corrective_is_rejected_without_an_order_query() {
    // Correctives are exempt from the order-number check (verified), so a
    // 71/152 the re-query cannot resolve is an ordinary rejection, and the
    // live base invoice under the order is never consulted; a re-query that
    // finds the corrective is still reconciled.
    let corrective_id = ExternalId::new("acct:ORD-1:corrective:c1");

    let h = Harness::start().await;
    external_id_query(corrective_id.as_str())
        .respond_with(not_found())
        .expect(2)
        .mount(&h.server)
        .await;
    order_query("ORD-1")
        .respond_with(Doc::new("SZ-1", "SZ").response())
        .expect(0)
        .mount(&h.server)
        .await;
    create()
        .respond_with(api_error("71", "duplicate"))
        .mount(&h.server)
        .await;
    assert_eq!(
        h.create_kind(IssuedKind::Corrective, &corrective_id, None)
            .await,
        Ok(CreateOutcome::Rejected(Rejection::from(
            SzamlazzAnswer::new("71", "duplicate")
        )))
    );

    let h = Harness::start().await;
    external_id_query(corrective_id.as_str())
        .respond_with(not_found())
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    external_id_query(corrective_id.as_str())
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-1"),
                ..Doc::new("HS-1", "HS")
            }
            .response(),
        )
        .mount(&h.server)
        .await;
    create()
        .respond_with(api_error("71", "duplicate"))
        .mount(&h.server)
        .await;
    match h
        .create_kind(IssuedKind::Corrective, &corrective_id, None)
        .await
    {
        Ok(CreateOutcome::Reconciled(found)) => assert_eq!(found.number, "HS-1"),
        other => panic!("expected Reconciled, got {other:?}"),
    }
}
