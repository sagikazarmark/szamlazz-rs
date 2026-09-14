//! Local identity validation must precede even the leading holder query.

use super::common::{create, created, external_id_query, not_found};
use super::harness::{Harness, document, external_id, order};
use restate_szamlazz::contract::IssuedKind;
use restate_szamlazz::gateway::{
    CreateOutcome, CreatePermission, CreateStepRequest, CreateStepRequestError, DocumentRefs,
    recovery::WriteOperation,
};
use restate_szamlazz::{ExternalId, OrderKey};
use szamlazz_agent::InvoiceNumber;
use szamlazz_agent::ops::invoice::InvoiceKind;
use wiremock::matchers::body_string_contains;

/// Exercise the public path when construction succeeds, so accidental acceptance
/// is observable both as the wrong result and as unexpected network traffic.
async fn rejected_before_io(
    h: &Harness,
    request: Result<CreateStepRequest<'_>, CreateStepRequestError>,
    expected: CreateStepRequestError,
) {
    match request {
        Err(error) => assert_eq!(error, expected),
        Ok(request) => {
            let result = h
                .gateway
                .create_once(request, CreatePermission::grant())
                .await;
            panic!("inconsistent request reached Gateway: {result:?}");
        }
    }
    assert!(h.bodies().await.is_empty(), "no query or send");
}

#[tokio::test]
async fn built_create_cannot_be_paired_with_another_identity() {
    let h = Harness::start().await;
    let id = external_id();
    let order = order();
    let outbound = h
        .gateway
        .build_create(
            IssuedKind::Invoice,
            &document(),
            &order,
            &id,
            DocumentRefs::default(),
        )
        .expect("build");
    let other_id = ExternalId::new("acct:ORD-2:invoice");
    let other_order = OrderKey::parse("ORD-2").expect("order");
    for (intended_id, intended_order, error) in [
        (
            &other_id,
            &order,
            CreateStepRequestError::ExternalIdMismatch,
        ),
        (&id, &other_order, CreateStepRequestError::OrderMismatch),
    ] {
        rejected_before_io(
            &h,
            CreateStepRequest::new(
                intended_id,
                IssuedKind::Invoice,
                intended_order,
                &outbound,
                None,
                None,
            ),
            error,
        )
        .await;
    }

    // Omission, blank text, case changes and surrounding whitespace are not
    // repaired. This includes mutations of an otherwise valid build result.
    for value in [
        None,
        Some(""),
        Some(" "),
        Some("acct:ord-1:invoice"),
        Some("acct:ORD-1:invoice "),
    ] {
        let mut changed = outbound.clone();
        changed.external_id = value.map(str::to_owned);
        rejected_before_io(
            &h,
            CreateStepRequest::new(&id, IssuedKind::Invoice, &order, &changed, None, None),
            CreateStepRequestError::ExternalIdMismatch,
        )
        .await;
    }
    for value in [None, Some(""), Some(" "), Some("ord-1"), Some(" ORD-1 ")] {
        let mut changed = outbound.clone();
        changed.header.order_number = value.map(str::to_owned);
        rejected_before_io(
            &h,
            CreateStepRequest::new(&id, IssuedKind::Invoice, &order, &changed, None, None),
            CreateStepRequestError::OrderMismatch,
        )
        .await;
    }
}

#[tokio::test]
async fn every_supported_kind_requires_the_same_outbound_kind() {
    let h = Harness::start().await;
    let id = external_id();
    let order = order();
    for outbound_kind in IssuedKind::ALL {
        let corrected = (outbound_kind == IssuedKind::Corrective).then_some("SZ-BASE");
        let outbound = h
            .gateway
            .build_create(
                outbound_kind,
                &document(),
                &order,
                &id,
                DocumentRefs {
                    proforma: None,
                    prepayment: Some("ES-1"),
                    corrected,
                },
            )
            .expect("build supported kind");
        for intended_kind in IssuedKind::ALL {
            let request = CreateStepRequest::new(
                &id,
                intended_kind,
                &order,
                &outbound,
                Some("OLD-1"),
                corrected,
            );
            if intended_kind == outbound_kind {
                assert_eq!(
                    request.expect("matching kind").operation(),
                    WriteOperation::Create {
                        kind: intended_kind,
                        expected_number: Some("OLD-1".to_owned()),
                        corrected_number: corrected.map(str::to_owned),
                    }
                );
            } else {
                rejected_before_io(&h, request, CreateStepRequestError::KindMismatch).await;
            }
        }
        if corrected.is_none() {
            rejected_before_io(
                &h,
                CreateStepRequest::new(
                    &id,
                    outbound_kind,
                    &order,
                    &outbound,
                    None,
                    Some("SZ-BASE"),
                ),
                CreateStepRequestError::CorrectiveIntentMismatch,
            )
            .await;
        }
        let unsupported = szamlazz_agent::ops::invoice::CreateInvoice {
            kind: InvoiceKind::DeliveryNote,
            ..outbound
        };
        rejected_before_io(
            &h,
            CreateStepRequest::new(&id, outbound_kind, &order, &unsupported, None, corrected),
            CreateStepRequestError::UnsupportedKind,
        )
        .await;
    }
}

#[tokio::test]
async fn corrective_base_must_be_present_nonblank_and_exactly_the_intended_base() {
    let h = Harness::start().await;
    let id = ExternalId::new("acct:ORD-1:corrective:c1");
    let order = order();
    let mut outbound = h
        .gateway
        .build_create(
            IssuedKind::Corrective,
            &document(),
            &order,
            &id,
            DocumentRefs {
                corrected: Some("SZ-BASE"),
                ..DocumentRefs::default()
            },
        )
        .expect("build corrective");
    for intended in [
        None,
        Some(""),
        Some("SZ-OTHER"),
        Some("sz-base"),
        Some(" SZ-BASE "),
    ] {
        rejected_before_io(
            &h,
            CreateStepRequest::new(
                &id,
                IssuedKind::Corrective,
                &order,
                &outbound,
                None,
                intended,
            ),
            CreateStepRequestError::CorrectiveIntentMismatch,
        )
        .await;
    }
    for blank in ["", " ", "\t", "\u{2003}"] {
        outbound.kind = InvoiceKind::Corrective {
            corrected_number: InvoiceNumber::new(blank),
        };
        rejected_before_io(
            &h,
            CreateStepRequest::new(
                &id,
                IssuedKind::Corrective,
                &order,
                &outbound,
                None,
                Some(blank),
            ),
            CreateStepRequestError::CorrectiveIntentMismatch,
        )
        .await;
    }
}

#[tokio::test]
async fn validated_supported_kinds_send_the_identity_retained_for_recovery() {
    for kind in IssuedKind::ALL {
        let h = Harness::start().await;
        let id = ExternalId::new(format!("acct:ORD-1:{kind}"));
        let order = order();
        let corrected = (kind == IssuedKind::Corrective).then_some("SZ-BASE");
        let outbound = h
            .gateway
            .build_create(
                kind,
                &document(),
                &order,
                &id,
                DocumentRefs {
                    proforma: Some("D-1"),
                    prepayment: Some("ES-1"),
                    corrected,
                },
            )
            .expect("build");
        let request = CreateStepRequest::new(&id, kind, &order, &outbound, None, corrected)
            .expect("consistent identity");
        assert_eq!(
            request.operation(),
            WriteOperation::Create {
                kind,
                expected_number: None,
                corrected_number: corrected.map(str::to_owned),
            }
        );
        external_id_query(id.as_str())
            .respond_with(not_found())
            .expect(1)
            .mount(&h.server)
            .await;
        create()
            .and(body_string_contains(format!(
                "<szamlaKulsoAzon>{id}</szamlaKulsoAzon>"
            )))
            .and(body_string_contains("<rendelesSzam>ORD-1</rendelesSzam>"))
            .and(body_string_contains(match kind {
                IssuedKind::Proforma => "<dijbekero>true</dijbekero>",
                IssuedKind::Invoice => "<dijbekeroSzamlaszam>D-1</dijbekeroSzamlaszam>",
                IssuedKind::Prepayment => "<elolegszamla>true</elolegszamla>",
                IssuedKind::Final => "<vegszamla>true</vegszamla>",
                IssuedKind::Corrective => {
                    "<helyesbitettSzamlaszam>SZ-BASE</helyesbitettSzamlaszam>"
                }
            }))
            .respond_with(created("NEW-1", "1000", "1270"))
            .expect(1)
            .mount(&h.server)
            .await;
        assert!(matches!(
            h.gateway
                .create_once(request, CreatePermission::grant())
                .await,
            Ok(CreateOutcome::Issued(_))
        ));
        assert_eq!(h.bodies().await.len(), 2, "one query and one create");
        h.server.verify().await;
    }
}
