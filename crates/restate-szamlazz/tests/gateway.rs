//! Wiremock tests of the `gateway` module: the lookup and create steps,
//! storno validation, deletion, credit entries, credential rejections and
//! failed exchanges (`Unanswered` on the reads, `Unconfirmed` on the writes)
//! against synthetic szamlazz.hu responses, the shared fixtures of
//! `tests/common` (the document renderer, the response templates, the
//! selector matchers).
//!
//! What a test here proves is the **wire**: which requests a step sends and
//! in what order (`expect(n)`, the recorded bodies), what it puts in them, and
//! that each answer szamlazz.hu can give, in headers or in the body alone, is
//! read into the outcome the gateway's classifiers name. The classifiers
//! themselves (`settle_create`, `settle_storno`, `classify_failure`,
//! `QueryError::answered`, `is_foreign`, the credential codes) are pure and
//! table-tested in the module's unit tests; a decision they make is pinned
//! here once per step, as a row of a table with one gateway per row, not once
//! per code.

mod common;

use common::{
    CreditRecord, Doc, ORIGINAL_TELJ, api_error, body_error, create, created,
    created_but_notification_failed, created_without_a_number, credit, delete, external_id_query,
    http_builder, http_client, not_found, number_query, order_query, original_telj_tag,
    proforma_deleted, proforma_gone, storno, szlahu_down, taxpayer_known, taxpayer_nav_error,
    taxpayer_query, taxpayer_unknown,
};
use jiff::civil::date;
use restate_szamlazz::account::{Account, Endpoint};
use restate_szamlazz::contract::{
    BuyerInput, DocumentInput, IssuedKind, LineItemInput, PaymentEntry, PaymentMethod, Selector,
};
use restate_szamlazz::gateway::{
    CreateOutcome, CreateStepRequest, DeleteOutcome, DocumentRefs, Gateway, LookupOutcome,
    LookupRequest, OwnershipOutcome, ProbeOutcome, QueryOutcome, Rejection, RejectionCode,
    SetPaymentsOutcome, StornoLookupOutcome, StornoOutcome, StornoStepRequest, SzamlazzAnswer,
    TaxpayerOutcome, Unanswered, Unconfirmed,
};
use restate_szamlazz::{ExternalId, OrderKey};
use rust_decimal::dec;
use szamlazz_agent::ops::taxpayer::TaxpayerPrefix;
use szamlazz_agent::{Credentials, reqwest};
use wiremock::matchers::{body_string_contains, method};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ----- fixtures --------------------------------------------------------------

/// A gateway for the test account, opened as the prologue would open it.
fn gateway(server: &MockServer) -> Gateway {
    open(server, "acct", "key")
}

fn order() -> OrderKey {
    OrderKey::parse("ORD-1").expect("order")
}

fn external_id() -> ExternalId {
    ExternalId::new("acct:ORD-1:invoice")
}

fn storno_id() -> ExternalId {
    ExternalId::new("acct:ORD-1:storno:SZ-1")
}

fn document() -> DocumentInput {
    DocumentInput::new(
        BuyerInput::new("Kovács Bt.", "2030", "Érd", "Tárnoki út 23."),
        vec![LineItemInput::new(
            "Elado izé",
            dec!(1),
            "db",
            dec!(1000),
            "27",
        )],
        date(2026, 9, 3),
        date(2026, 9, 11),
        PaymentMethod::Transfer,
    )
}

struct Harness {
    server: MockServer,
    gateway: Gateway,
}

impl Harness {
    async fn start() -> Self {
        let server = MockServer::start().await;
        let gateway = gateway(&server);
        Self { server, gateway }
    }

    /// The lookup step for an invoice of `ORD-1`, answered.
    async fn lookup(&self, our_numbers: &[String]) -> LookupOutcome {
        self.lookup_kind(IssuedKind::Invoice, &external_id(), our_numbers)
            .await
    }

    /// The lookup step for an invoice of `ORD-1`, as the run sees it:
    /// `Err(Unanswered)` is what the read policy re-executes.
    async fn try_lookup(&self, our_numbers: &[String]) -> Result<LookupOutcome, Unanswered> {
        self.gateway
            .lookup(LookupRequest {
                external_id: &external_id(),
                kind: IssuedKind::Invoice,
                order: &order(),
                our_numbers,
            })
            .await
    }

    async fn lookup_kind(
        &self,
        kind: IssuedKind,
        external_id: &ExternalId,
        our_numbers: &[String],
    ) -> LookupOutcome {
        self.gateway
            .lookup(LookupRequest {
                external_id,
                kind,
                order: &order(),
                our_numbers,
            })
            .await
            .expect("szamlazz.hu answered")
    }

    /// The create step for an invoice of `ORD-1`; `reversed` is the number
    /// the lookup step saw reversed under the id.
    async fn create(&self, reversed: Option<&str>) -> Result<CreateOutcome, Unconfirmed> {
        self.create_kind(IssuedKind::Invoice, &external_id(), reversed)
            .await
    }

    async fn create_kind(
        &self,
        kind: IssuedKind,
        external_id: &ExternalId,
        reversed: Option<&str>,
    ) -> Result<CreateOutcome, Unconfirmed> {
        let refs = if kind == IssuedKind::Corrective {
            DocumentRefs {
                corrected: Some("SZ-1"),
                ..DocumentRefs::default()
            }
        } else {
            DocumentRefs::default()
        };
        self.create_with_refs(kind, external_id, reversed, refs)
            .await
    }

    /// The create step for a document of `ORD-1` carrying `refs`: what the
    /// handler's steps 1–2 resolved.
    async fn create_with_refs(
        &self,
        kind: IssuedKind,
        external_id: &ExternalId,
        reversed: Option<&str>,
        refs: DocumentRefs<'_>,
    ) -> Result<CreateOutcome, Unconfirmed> {
        let order = order();
        let create = self
            .gateway
            .build_create(kind, &document(), &order, external_id, refs)
            .expect("build");
        self.gateway
            .create(CreateStepRequest {
                external_id,
                kind,
                order: &order,
                create: &create,
                reversed,
            })
            .await
    }

    async fn bodies(&self) -> Vec<String> {
        self.server
            .received_requests()
            .await
            .expect("requests")
            .iter()
            .map(|request| String::from_utf8_lossy(&request.body).into_owned())
            .collect()
    }
}

// ----- lookup ----------------------------------------------------------------

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
            assert_eq!(found.document_type, "SZ");
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
    // agent crate (`ParseError::HttpStatus`, a `Parse` error): szamlazz.hu did
    // not answer, so the step's result is its retryable error, never a
    // journaled outcome. The external id first; then the hint, whose own
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
/// query the exclusivity, proforma-link, `get` and delete reads journal. The
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

// ----- create ----------------------------------------------------------------

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
async fn create_re_executed_after_a_lost_reply_finds_the_document_and_sends_nothing() {
    // The step, driven twice: the first execution's reply is lost (500), its
    // immediate re-query still sees nothing, so it is unconfirmed; the second
    // execution's leading query finds the document that landed and sends no
    // create, also when the lookup step had seen a reversed document
    // (`reissue: true`) that this live one is not.
    for (label, reversed) in [("plain", None), ("reissue", Some("SZ-0"))] {
        let h = Harness::start().await;
        external_id_query("acct:ORD-1:invoice")
            .respond_with(not_found())
            .up_to_n_times(2)
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
        match h.create(reversed).await {
            Ok(CreateOutcome::Found(found)) => assert_eq!(found.number, "SZ-1", "{label}"),
            other => panic!("{label}: expected Found, got {other:?}"),
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
    // `Reversed`, not `Unconfirmed`, which would re-execute the step into a
    // second send.
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

    // Re-executed anyway (the run policy re-dispatching a step whose result
    // was not journaled): the leading query settles it again, nothing sent.
    match h.create(None).await {
        Ok(CreateOutcome::Reversed(found)) => assert_eq!(found.number, "SZ-1"),
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
            "Unavailable maintenance",
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
            Unconfirmed::Unavailable("maintenance".to_owned()),
        ),
        (
            "no number",
            created_without_a_number(),
            Unconfirmed::Transport("missing szamlaszam in response".to_owned()),
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

/// What the run journals as its last failure (`Unconfirmed`'s display) names
/// the cause it stands for: `szlahu_down` after a send is unavailability, not
/// an "open code", and an open answer without a code is szamlazz.hu's success
/// without a document number, never `szlahu_down` (#63).
#[test]
fn unconfirmed_displays_name_their_cause() {
    let open = Unconfirmed::Open {
        code: Some("56".to_owned()),
        message: "signing".to_owned(),
    }
    .to_string();
    assert_eq!(open, "open code 56: signing");

    let no_number = Unconfirmed::Open {
        code: None,
        message: "create succeeded without a document number".to_owned(),
    }
    .to_string();
    assert!(!no_number.contains("szlahu_down"), "{no_number}");
    assert!(no_number.contains("document number"), "{no_number}");

    let down = Unconfirmed::Unavailable("maintenance".to_owned()).to_string();
    assert!(down.contains("szlahu_down"), "{down}");
    assert!(down.contains("maintenance"), "{down}");

    let transport = Unconfirmed::Transport("empty response".to_owned()).to_string();
    assert_eq!(transport, "transport failure: empty response");
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
            "maintenance",
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

// ----- create: the duplicate order number (71/152) --------------------------

const DUPLICATE_MESSAGE: &str = "M%C3%A1r+l%C3%A9tez%C5%91+rendel%C3%A9ssz%C3%A1m";

/// The leading query misses, the create answers 152, and the re-query sees
/// `under_id`.
async fn duplicate_harness(under_id: ResponseTemplate) -> Harness {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
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
        let h = duplicate_harness(under_id).await;
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

/// The 71/152 re-query is what settles whether the duplicate is ours; when it
/// fails itself the step is unconfirmed naming both the refusal and the
/// re-query's failure (#63), and the order-number query (which names, but
/// cannot settle) is not taken.
#[tokio::test]
async fn duplicate_order_number_whose_re_query_fails_is_unconfirmed_naming_both() {
    let h = duplicate_harness(ResponseTemplate::new(500)).await;
    order_query("ORD-1")
        .respond_with(not_found())
        .expect(0)
        .mount(&h.server)
        .await;

    let error = h.create(None).await.expect_err("unconfirmed");
    match &error {
        Unconfirmed::ReQueryFailed { sent, re_query } => {
            assert!(sent.contains("152"), "{sent}");
            assert!(sent.contains("Már létező rendelésszám"), "{sent}");
            assert!(re_query.contains("HTTP 500"), "{re_query}");
        }
        other => panic!("expected ReQueryFailed, got {other:?}"),
    }
    assert_eq!(h.bodies().await.len(), 3, "query, create, re-query");
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
        .respond_with(Doc::new("HS-1", "HS").response())
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

// ----- credentials -----------------------------------------------------------

/// A credential code (3, 135, 136, 164) is `CredentialsRejected` on **every**
/// operation, read off the wire where szamlazz.hu puts it: in the body alone
/// (the XML query's shape) or in the headers and the body (the create's, the
/// storno's, the delete's, the taxpayer query's). Which codes are credential
/// codes is `ErrorCode::is_credential_error`'s table, unit-tested in the
/// agent crate; that each step consults it before doing anything else is
/// pinned here with one code per operation and the operation's own shape:
/// `expect(1)` on the answering mock, `expect(0)` on the send that must not
/// follow (the hint after a rejected external-id query, the create after a
/// rejected leading query), and no re-query after a rejected send (settled
/// data, never `Unconfirmed`: the run retry policy is not spent on a wrong
/// key).
#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "one table: every operation, one gateway each"
)]
async fn a_credential_code_on_any_operation_is_credentials_rejected() {
    let entry = PaymentEntry {
        date: date(2026, 9, 3),
        method: PaymentMethod::Card,
        amount: dec!(1000),
        description: None,
    };
    let rejected = |code: &str| SzamlazzAnswer::new(code, "login");

    // The lookup's external-id query, in the body: no hint follows.
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(body_error("3", "login"))
        .expect(1)
        .mount(&h.server)
        .await;
    order_query("ORD-1")
        .respond_with(Doc::new("SZ-77", "SZ").response())
        .expect(0)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.lookup(&[]).await,
        LookupOutcome::CredentialsRejected(rejected("3")),
        "lookup, external id"
    );

    // The lookup's hint: conclusive, unlike a miss or another code on it.
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;
    order_query("ORD-1")
        .respond_with(body_error("135", "login"))
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.lookup(&[]).await,
        LookupOutcome::CredentialsRejected(rejected("135")),
        "lookup, hint"
    );

    // The create's send, in the headers: settled without a re-query.
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;
    create()
        .respond_with(api_error("136", "login"))
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.create(None).await,
        Ok(CreateOutcome::CredentialsRejected(rejected("136"))),
        "create, send"
    );
    assert_eq!(h.bodies().await.len(), 2, "create: no re-query");

    // The three reads of a query, in the body.
    let h = Harness::start().await;
    common::op("action-szamla_agent_xml")
        .respond_with(body_error("164", "login"))
        .mount(&h.server)
        .await;
    let expected = Ok(QueryOutcome::CredentialsRejected(rejected("164")));
    assert_eq!(h.gateway.verify("SZ-1").await, expected, "verify");
    assert_eq!(h.gateway.hint(&order()).await, expected, "hint");
    assert_eq!(
        h.gateway
            .query(&Selector::ExternalId("acct:ORD-1:invoice".to_owned()))
            .await,
        expected,
        "query"
    );

    // The probe: data, one request.
    let h = Harness::start().await;
    external_id_query(probe_id().as_str())
        .respond_with(body_error("3", "login"))
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.probe(&probe_id()).await,
        Ok(ProbeOutcome::CredentialsRejected(rejected("3"))),
        "probe"
    );

    // The storno lookup.
    let h = Harness::start().await;
    external_id_query(storno_id().as_str())
        .respond_with(body_error("135", "login"))
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.lookup_storno(&storno_id(), "SZ-1").await,
        Ok(StornoLookupOutcome::CredentialsRejected(rejected("135"))),
        "storno lookup"
    );

    // The storno's send, in the headers: settled without a re-query. (The
    // storno's leading query is a row of its leading-query table.)
    let h = Harness::start().await;
    external_id_query(storno_id().as_str())
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;
    storno()
        .respond_with(api_error("136", "login"))
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.storno(storno_request(&storno_id())).await,
        Ok(StornoOutcome::CredentialsRejected(rejected("136"))),
        "storno, send"
    );
    assert_eq!(h.bodies().await.len(), 2, "storno: no re-query");

    // The delete and the credit entries.
    let h = Harness::start().await;
    delete()
        .respond_with(api_error("164", "login"))
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.delete_proforma("D-1").await,
        DeleteOutcome::CredentialsRejected(rejected("164")),
        "delete"
    );
    let h = Harness::start().await;
    credit()
        .respond_with(body_error("3", "login"))
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway
            .set_payments("SZ-1", std::slice::from_ref(&entry), false)
            .await,
        SetPaymentsOutcome::CredentialsRejected(rejected("3")),
        "set_payments"
    );

    // The taxpayer query, in the headers.
    let h = Harness::start().await;
    taxpayer_query("12345678")
        .respond_with(api_error("135", "login"))
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.query_taxpayer(&prefix()).await,
        Ok(TaxpayerOutcome::CredentialsRejected(rejected("135"))),
        "taxpayer"
    );
}

// ----- queries ---------------------------------------------------------------

#[tokio::test]
async fn verify_query_and_hint() {
    let h = Harness::start().await;
    number_query("SZ-1")
        .respond_with(
            Doc {
                payments: &[CreditRecord::transfer("500"), CreditRecord::transfer("770")],
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
            assert_eq!(found.payment_amounts(), vec![dec!(500), dec!(770)]);
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
            assert_eq!(found.document_type, "SS");
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
            assert_eq!(found.payments.len(), 2);
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

// ----- probe (`Szamlazz.Agent.check_account`) ---------------------------------

/// The sentinel id the probe queries: `{namespace}:check-account`.
fn probe_id() -> ExternalId {
    ExternalId::for_probe(&"acct".parse().expect("namespace"))
}

/// The probe is one external-id query of the sentinel id and nothing else;
/// szamlazz.hu's code 7 is the expected answer and means the credentials
/// were accepted.
#[tokio::test]
async fn probe_sends_one_query_of_the_sentinel_id_and_accepts_not_found() {
    let h = Harness::start().await;
    external_id_query(probe_id().as_str())
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;

    assert_eq!(
        h.gateway.probe(&probe_id()).await,
        Ok(ProbeOutcome::Accepted)
    );

    let sent = h.bodies().await;
    assert_eq!(sent.len(), 1, "exactly one request: {sent:?}");
    assert!(
        sent[0].contains("name=\"action-szamla_agent_xml\""),
        "a query"
    );
    assert!(
        sent[0].contains("<szamlaKulsoAzon>acct:check-account</szamlaKulsoAzon>"),
        "of the sentinel id: {}",
        sent[0]
    );
    assert!(sent[0].contains("<szamlaagentkulcs>key</szamlaagentkulcs>"));
}

/// A document under the sentinel id (someone issued one by hand) still
/// proves the credentials; the probe issues nothing and reports `Accepted`.
#[tokio::test]
async fn probe_accepts_a_document_under_the_sentinel_id() {
    let h = Harness::start().await;
    external_id_query(probe_id().as_str())
        .respond_with(Doc::new("SZ-9", "SZ").response())
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.probe(&probe_id()).await,
        Ok(ProbeOutcome::Accepted)
    );
    assert_eq!(h.bodies().await.len(), 1);
}

/// A non-credential szamlazz.hu code on the probe still proves the key:
/// szamlazz.hu answers the credential codes before anything else.
#[tokio::test]
async fn probe_accepts_any_other_szamlazz_code() {
    let h = Harness::start().await;
    external_id_query(probe_id().as_str())
        .respond_with(api_error("57", "Rendszerhiba"))
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.probe(&probe_id()).await,
        Ok(ProbeOutcome::Accepted)
    );
    assert_eq!(h.bodies().await.len(), 1);
}

/// A failed exchange (a transport failure, `szlahu_down`) settles nothing
/// about the credentials: it is the read's retryable error, not an outcome.
#[tokio::test]
async fn probe_without_an_answer_is_unanswered() {
    let h = Harness::start().await;
    external_id_query(probe_id().as_str())
        .respond_with(ResponseTemplate::new(503))
        .mount(&h.server)
        .await;
    assert!(matches!(
        h.gateway.probe(&probe_id()).await,
        Err(Unanswered::Transport(_))
    ));

    let h = Harness::start().await;
    external_id_query(probe_id().as_str())
        .respond_with(ResponseTemplate::new(503).insert_header("szlahu_down", "maintenance"))
        .mount(&h.server)
        .await;
    assert!(matches!(
        h.gateway.probe(&probe_id()).await,
        Err(Unanswered::Unavailable(message)) if message.contains("maintenance")
    ));
}

// ----- storno ----------------------------------------------------------------

/// The storno step request of `SZ-1`, repeating the original's `telj`
/// ([`ORIGINAL_TELJ`]) as its `fulfillment_date`.
fn storno_request(external_id: &ExternalId) -> StornoStepRequest<'_> {
    StornoStepRequest {
        invoice_number: "SZ-1",
        external_id,
        comment: Some("wrong buyer"),
        e_invoice: true,
        fulfillment_date: ORIGINAL_TELJ,
    }
}

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
async fn storno_of_a_zero_gross_invoice_is_reversed() {
    // The storno of a 0-HUF invoice (a free ticket) lands as a document
    // with a new number and a gross of 0: a reversal, not the echo of a
    // proforma or delivery note, which keeps the requested number.
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
        .mount(&h.server)
        .await;
    storno()
        .respond_with(created("SZ-1", "1000", "1270"))
        .mount(&h.server)
        .await;

    assert_eq!(
        h.gateway.storno(storno_request(&storno_id)).await,
        Ok(StornoOutcome::NotStornoable)
    );
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
            "Unavailable maintenance",
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

// ----- delete / credit -------------------------------------------------------

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
        DeleteOutcome::Transport(_)
    ));
    assert_eq!(
        h.gateway.delete_proforma("D-5").await,
        DeleteOutcome::Rejected(Rejection::from(SzamlazzAnswer::new("57", "malformed")))
    );
}

#[tokio::test]
async fn set_payments_outcomes() {
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
    let entry = PaymentEntry {
        date: date(2026, 9, 3),
        method: PaymentMethod::Card,
        amount: dec!(1000),
        description: Some("card".to_owned()),
    };

    match h
        .gateway
        .set_payments("SZ-1", std::slice::from_ref(&entry), true)
        .await
    {
        SetPaymentsOutcome::Done { outstanding, gross } => {
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
            .set_payments("SZ-2", std::slice::from_ref(&entry), false)
            .await,
        SetPaymentsOutcome::Rejected(Rejection::from(SzamlazzAnswer::new("463", "reversed")))
    );
    assert!(matches!(
        h.gateway
            .set_payments("SZ-3", std::slice::from_ref(&entry), false)
            .await,
        SetPaymentsOutcome::Transport(_)
    ));
    let six = vec![entry; 6];
    assert!(matches!(
        h.gateway.set_payments("SZ-9", &six, false).await,
        SetPaymentsOutcome::Rejected(Rejection {
            code: RejectionCode::Request,
            ..
        })
    ));
    // A replacing call with no entries would clear the invoice's payments:
    // refused by the agent crate before the wire, the caller's request.
    match h.gateway.set_payments("SZ-9", &[], false).await {
        SetPaymentsOutcome::Rejected(Rejection { code, message, .. }) => {
            assert_eq!(code, RejectionCode::Request);
            assert_eq!(code.as_str(), RejectionCode::REQUEST);
            assert!(message.contains("at least one entry"), "{message}");
        }
        other => panic!("expected Rejected, got {other:?}"),
    }
    assert_eq!(
        h.bodies().await.len(),
        3,
        "six entries and an empty replace never reach the wire"
    );
}

// ----- Gateway::open_with_http -----------------------------------------------

/// An [`Account`] on `server`, and the gateway opened for it with `key` over
/// a fresh [`http_client`], as the prologue opens one per execution.
fn open(server: &MockServer, id: &str, key: &str) -> Gateway {
    let mut account = Account::new(id, id);
    account.endpoint = Endpoint::parse(&server.uri()).expect("endpoint");
    Gateway::open_with_http(account, Credentials::agent_key(key), http_client()).expect("gateway")
}

fn agent_key_on_the_wire(body: &str) -> Option<&str> {
    let start = body.find("<szamlaagentkulcs>")? + "<szamlaagentkulcs>".len();
    let end = body[start..].find("</szamlaagentkulcs>")? + start;
    Some(&body[start..end])
}

/// The caller-built client is the transport: what it is configured with (a
/// header here; a proxy or a TLS setup in an embedder) is on every request
/// the gateway sends. The embedder's hook of `Gateway::open_with_http`.
#[tokio::test]
async fn a_gateway_opened_over_a_caller_built_client_sends_through_it() {
    let server = MockServer::start().await;
    external_id_query("acme:ORD-1:invoice")
        .respond_with(not_found())
        .mount(&server)
        .await;
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("x-embedder", "proxy-of-acme".parse().expect("header"));
    let http = http_builder()
        .default_headers(headers)
        .build()
        .expect("http client");
    let mut account = Account::new("acme", "acme");
    account.endpoint = Endpoint::parse(&server.uri()).expect("endpoint");
    let gateway = Gateway::open_with_http(account, Credentials::agent_key("key-acme"), http)
        .expect("gateway");

    let outcome = gateway
        .query(&Selector::ExternalId("acme:ORD-1:invoice".to_owned()))
        .await;
    assert!(matches!(outcome, Ok(QueryOutcome::NotFound)), "{outcome:?}");

    let sent = server.received_requests().await.expect("requests");
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0]
            .headers
            .get("x-embedder")
            .map(|value| value.to_str().expect("ascii")),
        Some("proxy-of-acme"),
        "the request went through the caller's client"
    );
    let body = String::from_utf8_lossy(&sent[0].body);
    assert_eq!(agent_key_on_the_wire(&body), Some("key-acme"));
}

#[tokio::test]
async fn a_gateway_opened_from_an_account_sends_that_accounts_key() {
    let server = MockServer::start().await;
    external_id_query("acme:ORD-1:invoice")
        .respond_with(not_found())
        .mount(&server)
        .await;
    let gateway = open(&server, "acme", "key-acme");

    let outcome = gateway
        .query(&Selector::ExternalId("acme:ORD-1:invoice".to_owned()))
        .await;
    assert!(matches!(outcome, Ok(QueryOutcome::NotFound)), "{outcome:?}");

    let sent = server.received_requests().await.expect("requests");
    assert_eq!(sent.len(), 1);
    let body = String::from_utf8_lossy(&sent[0].body);
    assert_eq!(agent_key_on_the_wire(&body), Some("key-acme"));
    assert_eq!(gateway.account().id.as_str(), "acme");
}

/// Two accounts on one szamlazz.hu: each gateway's requests carry its own
/// key, and a session cookie szamlazz.hu sets for the first never travels
/// with the second: a fresh client per gateway, here the one [`open`] builds
/// per call, as the prologue builds one per execution. The first gateway's
/// second request *does* carry the cookie, proving the cookie store is live
/// and the test would catch a client shared between the two.
#[tokio::test]
async fn two_gateways_opened_from_two_accounts_share_no_key_and_no_session() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(not_found().insert_header("set-cookie", "JSESSIONID=session-of-acme; Path=/"))
        .mount(&server)
        .await;
    let acme = open(&server, "acme", "key-acme");
    let beta = open(&server, "beta", "key-beta");

    let selector = Selector::OrderNumber("ORD-1".to_owned());
    assert!(matches!(
        acme.query(&selector).await,
        Ok(QueryOutcome::NotFound)
    ));
    assert!(matches!(
        beta.query(&selector).await,
        Ok(QueryOutcome::NotFound)
    ));
    assert!(matches!(
        acme.query(&selector).await,
        Ok(QueryOutcome::NotFound)
    ));

    let sent = server.received_requests().await.expect("requests");
    assert_eq!(sent.len(), 3);
    let cookie = |i: usize| {
        sent[i]
            .headers
            .get("cookie")
            .map(|value| value.to_str().expect("ascii").to_owned())
    };
    let key = |i: usize| {
        agent_key_on_the_wire(&String::from_utf8_lossy(&sent[i].body)).map(str::to_owned)
    };

    assert_eq!(key(0).as_deref(), Some("key-acme"));
    assert_eq!(cookie(0), None, "acme's first request: no session yet");
    assert_eq!(key(1).as_deref(), Some("key-beta"));
    assert_eq!(cookie(1), None, "beta never saw acme's Set-Cookie");
    assert_eq!(key(2).as_deref(), Some("key-acme"));
    assert_eq!(
        cookie(2).as_deref(),
        Some("JSESSIONID=session-of-acme"),
        "acme's own client keeps its own session"
    );
}

// ----- taxpayer (`Szamlazz.Agent.query_taxpayer`) -----------------------------

fn prefix() -> TaxpayerPrefix {
    "12345678".parse().expect("prefix")
}

/// A known taxpayer is `Found` with NAV's registered data projected onto the
/// crate-owned response: one `xmltaxpayer` request of the prefix, carrying
/// the account's agent key.
#[tokio::test]
async fn taxpayer_query_of_a_known_prefix_is_found_with_the_registered_data() {
    let h = Harness::start().await;
    taxpayer_query("12345678")
        .respond_with(taxpayer_known())
        .expect(1)
        .mount(&h.server)
        .await;

    let outcome = h
        .gateway
        .query_taxpayer(&prefix())
        .await
        .expect("szamlazz.hu answered");
    let TaxpayerOutcome::Found(taxpayer) = outcome else {
        panic!("expected Found, got {outcome:?}");
    };
    assert!(taxpayer.valid);
    assert_eq!(taxpayer.name.as_deref(), Some("SYNTHETIC SOFTWARE KFT."));
    assert_eq!(taxpayer.tax_number.as_deref(), Some("12345678"));
    assert_eq!(taxpayer.vat_code.as_deref(), Some("2"));
    assert_eq!(taxpayer.addresses.len(), 1);
    let address = &taxpayer.addresses[0];
    assert_eq!(address.kind.as_deref(), Some("HQ"));
    assert_eq!(address.country_code.as_deref(), Some("HU"));
    assert_eq!(address.postal_code.as_deref(), Some("1111"));
    assert_eq!(address.city.as_deref(), Some("TESTVAROS"));
    assert_eq!(address.street_name.as_deref(), Some("MINTA"));
    assert_eq!(address.public_place_category.as_deref(), Some("UTCA"));
    assert_eq!(address.number.as_deref(), Some("1."));

    let sent = h.bodies().await;
    assert_eq!(sent.len(), 1, "exactly one request: {sent:?}");
    assert!(
        sent[0].contains("name=\"action-szamla_agent_taxpayer\""),
        "a taxpayer query: {}",
        sent[0]
    );
    assert!(
        sent[0].contains("<szamlaagentkulcs>key</szamlaagentkulcs>"),
        "with the account's key"
    );
}

/// A well-formed prefix NAV knows no taxpayer under is a normal answer,
/// `Found` with `valid: false` and nothing else, not a fault: the caller
/// asked whether the number is valid, and the answer is no.
#[tokio::test]
async fn taxpayer_query_of_an_unknown_prefix_is_found_invalid_as_data() {
    let h = Harness::start().await;
    taxpayer_query("12345678")
        .respond_with(taxpayer_unknown())
        .expect(1)
        .mount(&h.server)
        .await;

    let outcome = h
        .gateway
        .query_taxpayer(&prefix())
        .await
        .expect("szamlazz.hu answered");
    let TaxpayerOutcome::Found(taxpayer) = outcome else {
        panic!("expected Found, got {outcome:?}");
    };
    assert!(!taxpayer.valid);
    assert_eq!(taxpayer.name, None);
    assert_eq!(taxpayer.tax_number, None);
    assert_eq!(taxpayer.vat_code, None);
    assert!(taxpayer.addresses.is_empty());
    assert_eq!(h.bodies().await.len(), 1);
}

/// Any other `funcCode ≠ OK` is szamlazz.hu's *answer* (NAV's relayed
/// `errorCode` in the body, or a szamlazz.hu code of its own in the headers),
/// and is `Api` data, never `Unanswered`: an answer is not retried.
#[tokio::test]
async fn taxpayer_query_answered_with_another_code_is_api_data() {
    let h = Harness::start().await;
    taxpayer_query("12345678")
        .respond_with(taxpayer_nav_error("57", "Synthetic XML parsing error"))
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.query_taxpayer(&prefix()).await,
        Ok(TaxpayerOutcome::Api(SzamlazzAnswer::new(
            "57",
            "Synthetic XML parsing error"
        )))
    );
    assert_eq!(h.bodies().await.len(), 1);

    let h = Harness::start().await;
    taxpayer_query("12345678")
        .respond_with(api_error("57", "Rendszerhiba"))
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.query_taxpayer(&prefix()).await,
        Ok(TaxpayerOutcome::Api(SzamlazzAnswer::new(
            "57",
            "Rendszerhiba"
        )))
    );
}

/// A failed exchange (a transport failure, an unparseable body,
/// `szlahu_down`) settles nothing: it is the read's retryable `Unanswered`,
/// not an outcome.
#[tokio::test]
async fn taxpayer_query_without_an_answer_is_unanswered() {
    let h = Harness::start().await;
    taxpayer_query("12345678")
        .respond_with(ResponseTemplate::new(503))
        .mount(&h.server)
        .await;
    assert!(matches!(
        h.gateway.query_taxpayer(&prefix()).await,
        Err(Unanswered::Transport(_))
    ));

    let h = Harness::start().await;
    taxpayer_query("12345678")
        .respond_with(ResponseTemplate::new(200).set_body_raw("<garbage/>", "application/xml"))
        .mount(&h.server)
        .await;
    assert!(matches!(
        h.gateway.query_taxpayer(&prefix()).await,
        Err(Unanswered::Transport(_))
    ));

    let h = Harness::start().await;
    taxpayer_query("12345678")
        .respond_with(ResponseTemplate::new(503).insert_header("szlahu_down", "maintenance"))
        .mount(&h.server)
        .await;
    assert!(matches!(
        h.gateway.query_taxpayer(&prefix()).await,
        Err(Unanswered::Unavailable(message)) if message.contains("maintenance")
    ));
}
