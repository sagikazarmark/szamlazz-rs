//! The *Lookup step* and the *Ownership lookup*: what one read-only step sees
//! under the order's external id and the order-number hint, and what it
//! answers as data (absent, ours live or reversed, a collision, a foreign
//! document, another code) or as `Unanswered`.

use super::common::{
    Doc, api_error, body_error, external_id_query, not_found, order_query, szlahu_down,
};
use super::harness::*;
use restate_szamlazz::ExternalId;
use restate_szamlazz::contract::IssuedKind;
use restate_szamlazz::gateway::{LookupOutcome, OwnershipOutcome, SzamlazzAnswer, Unanswered};
use rust_decimal::dec;
use wiremock::ResponseTemplate;

#[tokio::test]
async fn lookup_with_nothing_under_the_id_or_the_order_is_absent() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;
    order_query("ORD-1")
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;

    assert_eq!(h.lookup(&[]).await, LookupOutcome::Absent);
    assert_eq!(h.bodies().await.len(), 2, "the external id, then the hint");
}

#[tokio::test]
async fn lookup_finds_our_live_document_and_takes_no_hint() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(Doc::new("SZ-1", "SZ").response())
        .expect(1)
        .mount(&h.server)
        .await;
    order_query("ORD-1")
        .respond_with(Doc::new("SZ-77", "SZ").response())
        .expect(0)
        .mount(&h.server)
        .await;

    match h.lookup(&[]).await {
        LookupOutcome::Live(found) => {
            assert_eq!(found.number, "SZ-1");
            assert_eq!(found.document_type, szamlazz_agent::DocumentType::Invoice);
            assert!(found.is_live());
            assert_eq!(found.order_number.as_deref(), Some("ORD-1"));
            assert_eq!(found.gross_total, dec!(1270));
            assert_eq!(found.net_total, dec!(1000));
            assert_eq!(found.test, Some(true));
            // The journaled document is the worker's projection: the seller
            // and buyer blocks szamlazz.hu returned with it are not in it.
            let json = serde_json::to_value(&found).expect("serialises");
            assert!(json.get("supplier").is_none(), "{json}");
            assert!(json.get("buyer").is_none(), "{json}");
        }
        other => panic!("expected Live, got {other:?}"),
    }
    assert_eq!(h.bodies().await.len(), 1, "settled without the hint");
}

#[tokio::test]
async fn lookup_of_an_invalid_document_under_our_id_is_a_collision() {
    let other_order = Doc {
        order: Some("ORD-2"),
        ..Doc::new("SZ-9", "SZ")
    };
    let other_kind = Doc::new("D-9", "D");
    for (label, doc) in [("order", other_order), ("kind", other_kind)] {
        let h = Harness::start().await;
        external_id_query("acct:ORD-1:invoice")
            .respond_with(doc.response())
            .mount(&h.server)
            .await;
        order_query("ORD-1")
            .respond_with(not_found())
            .expect(0)
            .mount(&h.server)
            .await;
        match h.lookup(&[]).await {
            LookupOutcome::Collision(found) => assert_eq!(found.number, doc.number, "{label}"),
            other => panic!("{label}: expected Collision, got {other:?}"),
        }
    }
}

/// No account pin: a document of this order and kind under our id is ours
/// whatever its `teszt` and `szallito/id` say (or whether `teszt` says
/// anything; absent is `None` since #70), live, and it settles the lookup
/// without the hint. `teszt` is carried by the projection and compared with
/// nothing; the seller block is not carried at all.
#[tokio::test]
async fn lookup_holds_no_account_pin() {
    for (label, doc) in [
        (
            "teszt",
            Doc {
                test: Some(false),
                ..Doc::new("SZ-1", "SZ")
            },
        ),
        (
            "no teszt",
            Doc {
                test: None,
                ..Doc::new("SZ-1", "SZ")
            },
        ),
        (
            "szallito/id",
            Doc {
                supplier_id: 1,
                ..Doc::new("SZ-1", "SZ")
            },
        ),
    ] {
        let h = Harness::start().await;
        external_id_query("acct:ORD-1:invoice")
            .respond_with(doc.response())
            .expect(1)
            .mount(&h.server)
            .await;
        order_query("ORD-1")
            .respond_with(not_found())
            .expect(0)
            .mount(&h.server)
            .await;
        match h.lookup(&[]).await {
            LookupOutcome::Live(found) => {
                assert_eq!(found.number, "SZ-1", "{label}");
                assert_eq!(found.test, doc.test, "{label}: carried");
            }
            other => panic!("{label}: expected Live, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn lookup_of_our_reversed_document_names_its_storno_from_the_hint() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(Doc::reversed("SZ-1", "SZ").response())
        .expect(1)
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
        .expect(1)
        .mount(&h.server)
        .await;

    match h.lookup(&[]).await {
        LookupOutcome::Reversed {
            document,
            storno_number,
        } => {
            assert_eq!(document.number, "SZ-1");
            assert!(!document.is_live());
            assert_eq!(storno_number.as_deref(), Some("SS-1"));
        }
        other => panic!("expected Reversed, got {other:?}"),
    }
}

#[tokio::test]
async fn lookup_of_our_reversed_document_has_no_storno_number_when_the_hint_is_not_its_storno() {
    // The newest document under the order is the reversed document itself
    // (nothing newer exists) or the storno of another document: no storno
    // number is known, and neither is foreign.
    let same = Doc::reversed("SZ-1", "SZ");
    let other_storno = Doc {
        referenced_invoice: Some("SZ-0"),
        ..Doc::new("SS-0", "SS")
    };
    for (label, hint) in [("same", same), ("other storno", other_storno)] {
        let h = Harness::start().await;
        external_id_query("acct:ORD-1:invoice")
            .respond_with(Doc::reversed("SZ-1", "SZ").response())
            .mount(&h.server)
            .await;
        order_query("ORD-1")
            .respond_with(hint.response())
            .mount(&h.server)
            .await;

        match h.lookup(&[]).await {
            LookupOutcome::Reversed {
                document,
                storno_number,
            } => {
                assert_eq!(document.number, "SZ-1", "{label}");
                assert_eq!(storno_number, None, "{label}");
            }
            other => panic!("{label}: expected Reversed, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn lookup_reports_a_live_invoice_under_the_order_that_is_not_ours_as_foreign() {
    // Plain foreign; a conversion of our proforma not reachable under our id
    // (not issued by this service, so nothing is adopted); and a foreign
    // document beside our own reversed one, where no create (reissue or
    // not) may proceed.
    let plain = (not_found(), Doc::new("SZ-77", "SZ"), "SZ-77");
    let conversion = (
        not_found(),
        Doc {
            referenced_proforma: Some("D-1"),
            ..Doc::new("SZ-78", "SZ")
        },
        "SZ-78",
    );
    let beside_reversed = (
        Doc::reversed("SZ-1", "SZ").response(),
        Doc::new("ES-79", "ES"),
        "ES-79",
    );
    for (label, (under_id, hint, expected)) in [
        ("plain", plain),
        ("conversion", conversion),
        ("beside reversed", beside_reversed),
    ] {
        let h = Harness::start().await;
        external_id_query("acct:ORD-1:invoice")
            .respond_with(under_id)
            .mount(&h.server)
            .await;
        order_query("ORD-1")
            .respond_with(hint.response())
            .expect(1)
            .mount(&h.server)
            .await;

        match h.lookup(&["D-1".to_owned()]).await {
            LookupOutcome::Foreign(found) => assert_eq!(found.number, expected, "{label}"),
            other => panic!("{label}: expected Foreign, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn lookup_hint_ignores_our_documents_non_invoices_and_its_own_failure() {
    let ours = Doc::new("D-1", "D");
    let ours_by_number = Doc::new("SZ-1", "SZ");
    let reversed = Doc::reversed("SZ-5", "SZ");
    let storno = Doc {
        referenced_invoice: Some("SZ-5"),
        ..Doc::new("SS-5", "SS")
    };
    let proforma = Doc::new("D-9", "D");
    let cases = [
        ("our proforma", ours.response()),
        ("our number", ours_by_number.response()),
        ("reversed", reversed.response()),
        ("storno", storno.response()),
        ("proforma", proforma.response()),
        ("hint miss", not_found()),
        ("hint error", body_error("57", "malformed")),
    ];
    for (label, hint) in cases {
        let h = Harness::start().await;
        external_id_query("acct:ORD-1:invoice")
            .respond_with(not_found())
            .mount(&h.server)
            .await;
        order_query("ORD-1")
            .respond_with(hint)
            .expect(1)
            .mount(&h.server)
            .await;

        assert_eq!(
            h.lookup(&["D-1".to_owned(), "SZ-1".to_owned()]).await,
            LookupOutcome::Absent,
            "{label}"
        );
    }
}

#[tokio::test]
async fn lookup_without_an_answer_is_unanswered_not_data() {
    // A bare 500 with no `szlahu_*` header is refused by its status in the
    // agent crate (`ResponseError::HttpStatus`, the endpoint's answer, not
    // szamlazz.hu's): szamlazz.hu did not answer, so the step's result is its
    // retryable error, never a journaled outcome. The external id first; then the hint, whose own
    // failure is not conclusive either.
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(ResponseTemplate::new(500))
        .mount(&h.server)
        .await;
    assert!(matches!(
        h.try_lookup(&[]).await,
        Err(Unanswered::Transport(message)) if message.contains("HTTP 500")
    ));

    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .mount(&h.server)
        .await;
    order_query("ORD-1")
        .respond_with(ResponseTemplate::new(500))
        .mount(&h.server)
        .await;
    assert!(matches!(
        h.try_lookup(&[]).await,
        Err(Unanswered::Transport(_))
    ));

    // `szlahu_down` is szamlazz.hu saying it is not answering: the same.
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(
            ResponseTemplate::new(503)
                .insert_header("szlahu_down", "maintenance")
                .set_body_raw("", "text/plain"),
        )
        .mount(&h.server)
        .await;
    assert!(matches!(
        h.try_lookup(&[]).await,
        Err(Unanswered::Unavailable(_))
    ));
}

#[tokio::test]
async fn lookup_answered_with_another_code_is_data() {
    // Another szamlazz.hu code on the external-id query is an answer the
    // handler cannot conclude from: data, not a retryable failure. On the
    // hint it says nothing about foreign documents and the lookup continues.
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(body_error("57", "Ismeretlen hiba"))
        .mount(&h.server)
        .await;
    assert_eq!(
        h.try_lookup(&[]).await,
        Ok(LookupOutcome::Api(SzamlazzAnswer::new(
            "57",
            "Ismeretlen hiba"
        )))
    );

    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .mount(&h.server)
        .await;
    order_query("ORD-1")
        .respond_with(body_error("57", "Ismeretlen hiba"))
        .mount(&h.server)
        .await;
    assert_eq!(h.try_lookup(&[]).await, Ok(LookupOutcome::Absent));
}

#[tokio::test]
async fn lookup_of_a_corrective_takes_no_hint() {
    // The live base invoice under the order is expected, not foreign.
    let h = Harness::start().await;
    let corrective_id = ExternalId::new("acct:ORD-1:corrective:c1");
    external_id_query(corrective_id.as_str())
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;
    order_query("ORD-1")
        .respond_with(Doc::new("SZ-1", "SZ").response())
        .expect(0)
        .mount(&h.server)
        .await;

    assert_eq!(
        h.lookup_kind(IssuedKind::Corrective, &corrective_id, &[])
            .await,
        LookupOutcome::Absent
    );
    assert_eq!(h.bodies().await.len(), 1);
}

/// The ownership read (`lookup_ours`): the one "is this document ours?"
/// query the exclusivity, proforma-link, `get` and delete reads journal (every one a `lookup-{kind}` step). The
/// same validation as the lookup step's external-id query
/// (`FoundDocument::is_ours`): a document of this order and kind is `Live` or
/// `Reversed`, code 7 is `Absent`, another order's or kind's document is a
/// `Collision`, a credential code and another code are data; no hint is
/// taken. An exchange without an answer is `Unanswered`, never an outcome.
#[tokio::test]
async fn lookup_ours_validates_the_holder_and_takes_no_hint() {
    let h = Harness::start().await;
    let ours = || async {
        h.gateway
            .lookup_ours(&external_id(), &order(), IssuedKind::Invoice)
            .await
    };
    order_query("ORD-1")
        .respond_with(not_found())
        .expect(0)
        .mount(&h.server)
        .await;

    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    assert_eq!(ours().await, Ok(OwnershipOutcome::Absent));

    external_id_query("acct:ORD-1:invoice")
        .respond_with(Doc::default().response())
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    match ours().await {
        Ok(OwnershipOutcome::Live(found)) => assert_eq!(found.number, "SZ-1"),
        other => panic!("expected Live, got {other:?}"),
    }

    external_id_query("acct:ORD-1:invoice")
        .respond_with(
            Doc {
                reversed: true,
                ..Doc::default()
            }
            .response(),
        )
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    match ours().await {
        Ok(OwnershipOutcome::Reversed(found)) => {
            assert_eq!(found.number, "SZ-1");
            assert_eq!(found.reversed, Some(true));
        }
        other => panic!("expected Reversed, got {other:?}"),
    }

    for doc in [
        Doc {
            order: Some("ORD-2"),
            ..Doc::new("SZ-9", "SZ")
        },
        Doc::new("D-9", "D"),
    ] {
        external_id_query("acct:ORD-1:invoice")
            .respond_with(doc.response())
            .up_to_n_times(1)
            .mount(&h.server)
            .await;
        match ours().await {
            Ok(OwnershipOutcome::Collision(found)) => assert_eq!(found.number, doc.number),
            other => panic!("{}: expected Collision, got {other:?}", doc.number),
        }
    }

    external_id_query("acct:ORD-1:invoice")
        .respond_with(body_error("57", "Ismeretlen hiba"))
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        ours().await,
        Ok(OwnershipOutcome::Api(SzamlazzAnswer::new(
            "57",
            "Ismeretlen hiba"
        )))
    );

    external_id_query("acct:ORD-1:invoice")
        .respond_with(api_error("3", "Sikertelen bejelentkezés."))
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        ours().await,
        Ok(OwnershipOutcome::CredentialsRejected(SzamlazzAnswer::new(
            "3",
            "Sikertelen bejelentkezés."
        )))
    );

    external_id_query("acct:ORD-1:invoice")
        .respond_with(szlahu_down())
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    assert!(
        matches!(ours().await, Err(Unanswered::Unavailable(_))),
        "szlahu_down is unanswered"
    );
}
