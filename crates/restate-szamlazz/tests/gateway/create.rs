//! The public one-send create contract: an explicit consumed permission, the
//! leading holder check, and read-only reconciliation after uncertainty.

use super::common::{
    Doc, api_error, body_error, create, created, created_but_notification_failed,
    created_without_a_number, external_id_query, not_found, order_query, storno, szlahu_down,
};
use super::harness::*;
use restate_szamlazz::ExternalId;
use restate_szamlazz::contract::IssuedKind;
use restate_szamlazz::gateway::{
    CreateOutcome, CreatePermission, DocumentRefs, LookupOutcome, OwnershipOutcome, Rejection,
    SzamlazzAnswer, Unconfirmed,
};
use rust_decimal::dec;
use wiremock::ResponseTemplate;
use wiremock::matchers::body_string_contains;

static_assertions::assert_not_impl_any!(CreatePermission: Clone, Copy, serde::Serialize, serde::de::DeserializeOwned);

#[tokio::test]
async fn an_uncertain_create_stays_one_send_through_empty_reads_until_it_lands() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .up_to_n_times(4)
        .expect(4)
        .mount(&h.server)
        .await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(Doc::new("SZ-1", "SZ").response())
        .expect(1)
        .mount(&h.server)
        .await;
    create()
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&h.server)
        .await;

    assert!(matches!(
        h.create(None).await,
        Err(Unconfirmed::Transport(_))
    ));
    // Neither the immediate re-query nor further empty reads renew permission.
    for _ in 0..2 {
        assert_eq!(h.observe_create().await, Ok(OwnershipOutcome::Absent));
    }
    match h.observe_create().await {
        Ok(OwnershipOutcome::Live(found)) => assert_eq!(found.number, "SZ-1"),
        other => panic!("expected late document evidence, got {other:?}"),
    }
    assert_eq!(h.bodies().await.len(), 6, "five reads and exactly one send");
}

#[tokio::test]
async fn a_missing_expected_reissue_target_never_becomes_an_ordinary_create() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;
    create()
        .respond_with(created("SZ-2", "1000", "1270"))
        .expect(0)
        .mount(&h.server)
        .await;
    let outcome = h.create(Some("SZ-1")).await.expect("answered query");
    assert_eq!(outcome, CreateOutcome::TargetChanged);
}

#[tokio::test]
async fn a_lost_reissue_followed_by_absence_stops_without_another_send() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(Doc::reversed("SZ-1", "SZ").response())
        .up_to_n_times(1)
        .expect(1)
        .mount(&h.server)
        .await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .expect(2)
        .mount(&h.server)
        .await;
    create()
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&h.server)
        .await;
    assert!(matches!(
        h.create(Some("SZ-1")).await,
        Err(Unconfirmed::Transport(_))
    ));
    assert_eq!(h.observe_create().await, Ok(OwnershipOutcome::Absent));
    assert_eq!(h.bodies().await.len(), 4);
}

#[tokio::test]
async fn corrective_with_a_live_base_under_the_order_is_issued() {
    // Lookup, then create: the base invoice is the newest document under the
    // order throughout and is never queried; the corrective is issued.
    let h = Harness::start().await;
    let corrective_id = ExternalId::new("acct:ORD-1:corrective:c1");
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
        .and(body_string_contains(
            "<helyesbitoszamla>true</helyesbitoszamla>",
        ))
        .and(body_string_contains(
            "<helyesbitettSzamlaszam>SZ-1</helyesbitettSzamlaszam>",
        ))
        .respond_with(created("HS-1", "-1000", "-1270"))
        .expect(1)
        .mount(&h.server)
        .await;

    assert_eq!(
        h.lookup_kind(IssuedKind::Corrective, &corrective_id, &[])
            .await,
        LookupOutcome::Absent
    );
    match h
        .create_kind(IssuedKind::Corrective, &corrective_id, None)
        .await
    {
        Ok(CreateOutcome::Issued(issued)) => assert_eq!(issued.number, "HS-1"),
        other => panic!("expected Issued, got {other:?}"),
    }
}

#[tokio::test]
async fn create_with_nothing_under_the_id_sends_the_create_and_is_issued() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;
    create()
        .respond_with(created("SZ-2", "1000", "1270"))
        .expect(1)
        .mount(&h.server)
        .await;

    match h.create(None).await {
        Ok(CreateOutcome::Issued(issued)) => {
            assert_eq!(issued.number, "SZ-2");
            assert_eq!(issued.net_total, Some(dec!(1000)));
            assert_eq!(issued.gross_total, Some(dec!(1270)));
            assert_eq!(issued.outstanding, Some(dec!(1270)));
            assert_eq!(issued.document_id, Some(924_307_747));
            assert!(!issued.notification_delivery_failed);
        }
        other => panic!("expected Issued, got {other:?}"),
    }
    let bodies = h.bodies().await;
    assert_eq!(bodies.len(), 2, "the leading query, then the create");
    let create_body = &bodies[1];
    assert!(create_body.contains("<szamlaKulsoAzon>acct:ORD-1:invoice</szamlaKulsoAzon>"));
    assert!(create_body.contains("<rendelesSzam>ORD-1</rendelesSzam>"));
    assert!(create_body.contains("<szamlaLetoltes>false</szamlaLetoltes>"));
    assert!(create_body.contains("<nev>Kovács Bt.</nev>"));
}

/// The prepayment invoice consuming the order's proforma carries the
/// reference like the plain invoice does (#69): `dijbekeroSzamlaszam` beside
/// the `elolegszamla` flag on the create body, so szamlazz.hu links the
/// proforma explicitly rather than by shared order number. (The element's
/// XSD position is the Agent crate's own test.)
#[tokio::test]
async fn prepayment_consuming_a_proforma_sends_the_reference() {
    let h = Harness::start().await;
    let prepayment_id = ExternalId::new("acct:ORD-1:prepayment");
    external_id_query(prepayment_id.as_str())
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;
    create()
        .and(body_string_contains(
            "<dijbekeroSzamlaszam>D-1</dijbekeroSzamlaszam>",
        ))
        .and(body_string_contains("<elolegszamla>true</elolegszamla>"))
        .respond_with(created("ES-1", "1000", "1270"))
        .expect(1)
        .mount(&h.server)
        .await;

    let refs = DocumentRefs {
        proforma: Some("D-1"),
        ..DocumentRefs::default()
    };
    match h
        .create_with_refs(IssuedKind::Prepayment, &prepayment_id, None, refs)
        .await
    {
        Ok(CreateOutcome::Issued(issued)) => assert_eq!(issued.number, "ES-1"),
        other => panic!("expected Issued, got {other:?}"),
    }
    let bodies = h.bodies().await;
    assert_eq!(bodies.len(), 2, "the leading query, then the create");
    let create_body = &bodies[1];
    assert!(create_body.contains("<szamlaKulsoAzon>acct:ORD-1:prepayment</szamlaKulsoAzon>"));
    assert!(create_body.contains("<dijbekeroSzamlaszam>D-1</dijbekeroSzamlaszam>"));
}

#[tokio::test]
async fn read_only_reconciliation_after_a_lost_create_finds_the_document() {
    // One permission: the reply is lost (500) and the immediate re-query sees
    // nothing. A later read finds the document, without any fresh permission,
    // including when the original call was an expected-document reissue.
    for (label, reversed) in [("plain", None), ("reissue", Some("SZ-0"))] {
        let h = Harness::start().await;
        if let Some(number) = reversed {
            external_id_query("acct:ORD-1:invoice")
                .respond_with(Doc::reversed(number, "SZ").response())
                .up_to_n_times(1)
                .mount(&h.server)
                .await;
        }
        external_id_query("acct:ORD-1:invoice")
            .respond_with(not_found())
            .up_to_n_times(if reversed.is_some() { 1 } else { 2 })
            .mount(&h.server)
            .await;
        external_id_query("acct:ORD-1:invoice")
            .respond_with(Doc::new("SZ-1", "SZ").response())
            .mount(&h.server)
            .await;
        create()
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&h.server)
            .await;

        assert!(
            matches!(h.create(reversed).await, Err(Unconfirmed::Transport(_))),
            "{label}: first execution"
        );
        match h.observe_create().await {
            Ok(OwnershipOutcome::Live(found)) => assert_eq!(found.number, "SZ-1", "{label}"),
            other => panic!("{label}: expected Live, got {other:?}"),
        }
        assert_eq!(
            h.bodies().await.len(),
            4,
            "{label}: query, create, re-query; query"
        );
    }
}

#[tokio::test]
async fn create_with_a_lost_reply_whose_re_query_finds_the_document_reversed_is_settled() {
    // The send lands, its reply is lost, and the document is reversed before
    // the immediate re-query sees it. The re-query settles the step as
    // `Reversed`, not `Unconfirmed`. Neither grants another send permission.
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(Doc::reversed("SZ-1", "SZ").response())
        .mount(&h.server)
        .await;
    create()
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&h.server)
        .await;

    match h.create(None).await {
        Ok(CreateOutcome::Reversed(found)) => assert_eq!(found.number, "SZ-1"),
        other => panic!("expected Reversed, got {other:?}"),
    }
    assert_eq!(
        h.bodies().await.len(),
        3,
        "the leading query, the create, the re-query"
    );

    // A later observation uses only the read surface, never another create.
    match h.observe_create().await {
        Ok(OwnershipOutcome::Reversed(found)) => assert_eq!(found.number, "SZ-1"),
        other => panic!("expected Reversed, got {other:?}"),
    }
}

/// What the create step's **leading query** decides, on the wire: each answer
/// the external id can give, against the number the lookup step saw reversed
/// (`reversed`), and whether a create is sent (the create mock's `expect`).
/// The decision is `settle_create`'s, table-tested in the gateway's unit
/// tests; this asserts that the step consults it before the send, and that each
/// answer is read off the wire (code 7 and the credential code in the body
/// alone, the document, a bare 500, `szlahu_down`) into the outcome it names.
/// An *answer* that is neither 7 nor a credential code is settled data,
/// never `Unconfirmed` (#63); only a lost reply is.
#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "one table: twelve rows, one gateway each"
)]
async fn the_create_steps_leading_query_settles_or_proceeds() {
    let other_order = Doc {
        order: Some("ORD-2"),
        ..Doc::new("SZ-9", "SZ")
    };
    // (what the leading query answers, what the lookup saw reversed, creates
    // sent, the outcome)
    let rows: [(&str, ResponseTemplate, Option<&str>, u64, &str); 12] = [
        ("a clean miss sends", not_found(), None, 1, "Issued SZ-2"),
        (
            "a live document is an earlier execution's",
            Doc::new("SZ-1", "SZ").response(),
            None,
            0,
            "Found SZ-1",
        ),
        (
            "a live document that is not the lookup's reversed one",
            Doc::new("SZ-1", "SZ").response(),
            Some("SZ-0"),
            0,
            "Found SZ-1",
        ),
        (
            "the lookup's reversed document, still reversed, sends",
            Doc::reversed("SZ-1", "SZ").response(),
            Some("SZ-1"),
            1,
            "Issued SZ-2",
        ),
        (
            "a reversed document the lookup did not see",
            Doc::reversed("SZ-1", "SZ").response(),
            None,
            0,
            "Reversed SZ-1",
        ),
        (
            "a reversed document that is not the lookup's",
            Doc::reversed("SZ-1", "SZ").response(),
            Some("SZ-0"),
            0,
            "Reversed SZ-1",
        ),
        (
            "the lookup's reversed document reported live",
            Doc::new("SZ-1", "SZ").response(),
            Some("SZ-1"),
            0,
            "LiveAgain SZ-1",
        ),
        (
            "another order's document under the id",
            other_order.response(),
            None,
            0,
            "Collision SZ-9",
        ),
        (
            "another code is data",
            body_error("57", "Ismeretlen hiba"),
            None,
            0,
            "Api 57",
        ),
        (
            "szlahu_down is data",
            szlahu_down(),
            None,
            0,
            "Unavailable query: szlahu_down",
        ),
        (
            "a credential code never sends",
            body_error("3", "login"),
            None,
            0,
            "CredentialsRejected 3",
        ),
        (
            "a lost reply is the one Unconfirmed before a send",
            ResponseTemplate::new(500),
            None,
            0,
            "Unconfirmed Transport",
        ),
    ];
    for (label, under_id, reversed, sends, expected) in rows {
        let h = Harness::start().await;
        external_id_query("acct:ORD-1:invoice")
            .respond_with(under_id)
            .expect(1)
            .mount(&h.server)
            .await;
        create()
            .respond_with(created("SZ-2", "1000", "1270"))
            .expect(sends)
            .mount(&h.server)
            .await;

        let outcome = h.create(reversed).await;
        assert_eq!(describe_create(&outcome), expected, "{label}: {outcome:?}");
        assert_eq!(
            h.bodies().await.len(),
            1 + usize::try_from(sends).expect("0 or 1"),
            "{label}: the leading query, then the send or nothing"
        );
    }
}

/// A create step's outcome in one line, for the table above: the variant and
/// what it names. The document-carrying variants are `#[non_exhaustive]`
/// projections a test cannot build to compare with, so the tables compare
/// this line.
fn describe_create(outcome: &Result<CreateOutcome, Unconfirmed>) -> String {
    match outcome {
        Ok(CreateOutcome::Issued(issued)) => format!("Issued {}", issued.number),
        Ok(CreateOutcome::Found(found)) => format!("Found {}", found.number),
        Ok(CreateOutcome::Reversed(found)) => format!("Reversed {}", found.number),
        Ok(CreateOutcome::LiveAgain(found)) => format!("LiveAgain {}", found.number),
        Ok(CreateOutcome::Collision(found)) => format!("Collision {}", found.number),
        Ok(CreateOutcome::Api(answer)) => format!("Api {}", answer.code),
        Ok(CreateOutcome::Unavailable { message }) => format!("Unavailable {message}"),
        Ok(CreateOutcome::CredentialsRejected(answer)) => {
            format!("CredentialsRejected {}", answer.code)
        }
        Err(Unconfirmed::Transport(_)) => "Unconfirmed Transport".to_owned(),
        other => format!("{other:?}"),
    }
}

#[tokio::test]
async fn create_rejection_is_settled_without_a_re_query() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;
    create()
        .respond_with(api_error("259", "net"))
        .mount(&h.server)
        .await;

    assert_eq!(
        h.create(None).await,
        Ok(CreateOutcome::Rejected(Rejection::from(
            SzamlazzAnswer::new("259", "net")
        )))
    );
}

/// Code 56 **with** a number is not an open code: szamlazz.hu issued the
/// document and says which, so the step is `Issued` with
/// `notification_delivery_failed` set, the number and the totals, after
/// exactly one query and one send: no re-query, nothing unconfirmed. (Only
/// 56 without a number is open, the case below.)
#[tokio::test]
async fn create_answered_56_with_a_number_is_issued_with_the_notification_flag() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;
    create()
        .respond_with(created_but_notification_failed("SZ-2", "1000", "1270"))
        .expect(1)
        .mount(&h.server)
        .await;

    match h.create(None).await {
        Ok(CreateOutcome::Issued(issued)) => {
            assert_eq!(issued.number, "SZ-2");
            assert!(issued.notification_delivery_failed);
            assert_eq!(issued.net_total, Some(dec!(1000)));
            assert_eq!(issued.gross_total, Some(dec!(1270)));
            assert_eq!(issued.outstanding, Some(dec!(1270)));
            assert_eq!(issued.document_id, Some(924_307_747));
        }
        other => panic!("expected Issued, got {other:?}"),
    }
    assert_eq!(
        h.bodies().await.len(),
        2,
        "the leading query, then the create; no re-query"
    );
}

#[tokio::test]
async fn create_with_an_open_outcome_re_queries_once_and_is_unconfirmed_when_nothing_landed() {
    let cases: [(&str, ResponseTemplate, Unconfirmed); 4] = [
        (
            "maintenance",
            api_error("1", "maintenance"),
            Unconfirmed::Open {
                code: Some("1".to_owned()),
                message: "maintenance".to_owned(),
            },
        ),
        (
            "signing",
            api_error("55", "signing"),
            Unconfirmed::Open {
                code: Some("55".to_owned()),
                message: "signing".to_owned(),
            },
        ),
        (
            "down",
            ResponseTemplate::new(503).insert_header("szlahu_down", "maintenance"),
            Unconfirmed::Unavailable("create: szlahu_down".to_owned()),
        ),
        (
            "no number",
            created_without_a_number(),
            Unconfirmed::Transport("create: parse: missing response field".to_owned()),
        ),
    ];
    for (label, response, expected) in cases {
        let h = Harness::start().await;
        external_id_query("acct:ORD-1:invoice")
            .respond_with(not_found())
            .expect(2)
            .mount(&h.server)
            .await;
        create().respond_with(response).mount(&h.server).await;

        assert_eq!(h.create(None).await, Err(expected), "{label}");
    }
}

/// A post-send re-query that fails itself never hides how the send ended
/// (#63): the step is unconfirmed with both causes named (the send's
/// open code and the re-query's failure) whether the re-query lost its
/// reply, was answered with another code, or met `szlahu_down`. The storno
/// step composes the same way.
#[tokio::test]
async fn a_failed_post_send_re_query_names_both_the_send_and_its_own_failure() {
    let re_query_failures: [(&str, ResponseTemplate, &str); 3] = [
        ("lost", ResponseTemplate::new(500), "HTTP 500"),
        ("another code", body_error("57", "Hibás XML."), "57"),
        (
            "down",
            ResponseTemplate::new(503).insert_header("szlahu_down", "maintenance"),
            "szlahu_down",
        ),
    ];
    for (label, re_query, expected_in_re_query) in re_query_failures {
        let h = Harness::start().await;
        external_id_query("acct:ORD-1:invoice")
            .respond_with(not_found())
            .up_to_n_times(1)
            .mount(&h.server)
            .await;
        external_id_query("acct:ORD-1:invoice")
            .respond_with(re_query)
            .expect(1)
            .mount(&h.server)
            .await;
        create()
            .respond_with(api_error("56", "signing"))
            .expect(1)
            .mount(&h.server)
            .await;

        let error = h.create(None).await.expect_err(label);
        match &error {
            Unconfirmed::ReQueryFailed { sent, re_query } => {
                assert!(sent.contains("56"), "{label}: {sent}");
                assert!(sent.contains("signing"), "{label}: {sent}");
                assert!(
                    re_query.contains(expected_in_re_query),
                    "{label}: {re_query}"
                );
            }
            other => panic!("{label}: expected ReQueryFailed, got {other:?}"),
        }
        let display = error.to_string();
        assert!(display.contains("56"), "{label}: {display}");
        assert!(display.contains(expected_in_re_query), "{label}: {display}");
    }

    // A lost reply on the send, then a lost reply on the re-query: the
    // display says both, not just the query's.
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(body_error("57", "Hibás XML."))
        .mount(&h.server)
        .await;
    create()
        .respond_with(ResponseTemplate::new(500))
        .mount(&h.server)
        .await;
    let error = h.create(None).await.expect_err("unconfirmed");
    assert!(
        matches!(
            &error,
            Unconfirmed::ReQueryFailed { sent, re_query }
                if sent.contains("transport failure") && re_query.contains("57")
        ),
        "{error:?}"
    );

    // The storno step's twin.
    let h = Harness::start().await;
    let storno_id = storno_id();
    external_id_query(storno_id.as_str())
        .respond_with(not_found())
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    external_id_query(storno_id.as_str())
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&h.server)
        .await;
    storno()
        .respond_with(api_error("55", "signing"))
        .expect(1)
        .mount(&h.server)
        .await;
    let error = h
        .gateway
        .storno(storno_request(&storno_id))
        .await
        .expect_err("unconfirmed");
    assert!(
        matches!(
            &error,
            Unconfirmed::ReQueryFailed { sent, re_query }
                if sent.contains("55") && re_query.contains("HTTP 500")
        ),
        "{error:?}"
    );
}

#[tokio::test]
async fn create_with_an_open_outcome_is_found_when_the_re_query_sees_the_document() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(Doc::new("SZ-4", "SZ").response())
        .mount(&h.server)
        .await;
    create()
        .respond_with(api_error("55", "signing"))
        .mount(&h.server)
        .await;

    match h.create(None).await {
        Ok(CreateOutcome::Found(found)) => assert_eq!(found.number, "SZ-4"),
        other => panic!("expected Found, got {other:?}"),
    }
}

#[tokio::test]
async fn create_answered_with_a_code_the_crate_does_not_know_is_an_open_outcome() {
    // A code the agent crate does not know may be a refusal or a new "issued,
    // but…" code like 55 and 56: the create and the storno step treat it as
    // open (re-query once, then `Unconfirmed::Open` when nothing landed)
    // rather than claim `rejected`, which would assert that no document exists.
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .expect(2)
        .mount(&h.server)
        .await;
    create()
        .respond_with(api_error("999", "ismeretlen"))
        .expect(1)
        .mount(&h.server)
        .await;

    assert_eq!(
        h.create(None).await,
        Err(Unconfirmed::Open {
            code: Some("999".to_owned()),
            message: "ismeretlen".to_owned(),
        })
    );

    let h = Harness::start().await;
    let storno_id = storno_id();
    external_id_query(storno_id.as_str())
        .respond_with(not_found())
        .expect(2)
        .mount(&h.server)
        .await;
    storno()
        .respond_with(api_error("999", "ismeretlen"))
        .expect(1)
        .mount(&h.server)
        .await;

    assert_eq!(
        h.gateway.storno(storno_request(&storno_id)).await,
        Err(Unconfirmed::Open {
            code: Some("999".to_owned()),
            message: "ismeretlen".to_owned(),
        })
    );
}
