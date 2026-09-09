//! `Szamlazz.Order.get`: the live view of what szamlazz.hu holds under the
//! order's four external ids right now, four read-only steps under the read
//! policy and a fold; no state.

use restate_sdk::errors::HandlerError;
use restate_sdk::prelude::SharedObjectContext;

use super::prologue::Execution;
use super::support::{Lookup, lookup};
use crate::contract::{DocumentKind, DocumentState, DocumentStatus, OrderStatus};
use crate::gateway::FoundDocument;
use crate::identity::{ExternalId, OrderKey};

impl Execution {
    /// The live view: what szamlazz.hu holds under the order's four external
    /// ids right now: four read-only steps under the read policy, on the
    /// `order` the handler parsed from its key, then [`order_status`] on
    /// what they found.
    pub(super) async fn status(
        &self,
        ctx: &SharedObjectContext<'_>,
        order: OrderKey,
    ) -> Result<OrderStatus, HandlerError> {
        let mut found = Vec::new();
        for kind in DocumentKind::ALL {
            let external_id = ExternalId::for_kind(&self.config.namespace, &order, kind);
            let looked_up = lookup(
                ctx,
                self,
                format!("get-{kind}"),
                &external_id,
                &order,
                kind.into(),
            )
            .await?;
            found.push((kind, looked_up));
        }
        Ok(order_status(found))
    }
}

/// The `get` status folded from the four reads: a document of ours fills the
/// slot of its kind ([`document_status`]); nothing and a collision leave it
/// `None` (a read must not fail on an answer, and the issuing handlers are
/// the ones that refuse a collision). A proforma szamlazz.hu no longer
/// returns while the invoice or the prepayment carries `hivdijbekszam` was
/// consumed by that document: `{state: consumed, by}`, the invoice's
/// reference before the prepayment's.
fn order_status(found: impl IntoIterator<Item = (DocumentKind, Lookup)>) -> OrderStatus {
    let mut status = OrderStatus::default();
    for (kind, looked_up) in found {
        match looked_up {
            Lookup::Ours(found) => status.set(kind, Some(document_status(&found))),
            Lookup::Absent | Lookup::Collision(_) => {}
        }
    }
    if status.proforma.is_none()
        && let Some(consumer) = [&status.invoice, &status.prepayment]
            .into_iter()
            .flatten()
            .find(|document| document.referenced_proforma.is_some())
        && let Some(proforma) = &consumer.referenced_proforma
    {
        status.proforma = Some(DocumentStatus::new(
            proforma,
            DocumentState::Consumed {
                by: consumer.number.clone(),
            },
        ));
    }
    status
}

/// The `get` projection of a document of ours.
fn document_status(found: &FoundDocument) -> DocumentStatus {
    let state = if found.is_live() {
        DocumentState::Live
    } else {
        DocumentState::Reversed {
            storno_number: None,
        }
    };
    let mut status = DocumentStatus::new(&found.number, state);
    status.gross = Some(found.gross_total);
    status.net = Some(found.net_total);
    status.payments = found.payment_amounts();
    status
        .referenced_proforma
        .clone_from(&found.referenced_proforma_number);
    status.e_invoice = found.e_invoice();

    status
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use rust_decimal::dec;

    use super::*;
    use crate::test_support::{CreditRecord, Doc};

    /// The `get` projection of a document of ours: its number, `live` or
    /// `reversed` (the storno number is not looked up here), the totals,
    /// the credit entry amounts in szamlazz.hu's order, the proforma it
    /// references, and its appearance as a storno would lift it (`None` on a
    /// proforma).
    #[test]
    fn the_document_status_projects_what_szamlazz_reports() {
        let live = document_status(
            &Doc {
                payments: &[
                    CreditRecord::new(date(2026, 9, 4), "átutalás", "500"),
                    CreditRecord::new(date(2026, 9, 5), "átutalás", "770"),
                ],
                referenced_proforma: Some("D-1"),
                eszamla: Some(1),
                ..Doc::default()
            }
            .parse(),
        );
        assert_eq!(live.number, "SZ-1");
        assert_eq!(live.state, DocumentState::Live);
        assert_eq!(live.gross, Some(dec!(1270)));
        assert_eq!(live.net, Some(dec!(1000)));
        assert_eq!(live.payments, [dec!(500), dec!(770)]);
        assert_eq!(live.referenced_proforma.as_deref(), Some("D-1"));
        assert_eq!(live.e_invoice, Some(false), "eszamla 1 is paper");

        let reversed_status = document_status(
            &Doc {
                reversed: true,
                eszamla: Some(3),
                ..Doc::default()
            }
            .parse(),
        );
        assert_eq!(
            reversed_status.state,
            DocumentState::Reversed {
                storno_number: None
            }
        );
        assert!(reversed_status.payments.is_empty());
        assert_eq!(reversed_status.referenced_proforma, None);
        assert_eq!(
            reversed_status.e_invoice,
            Some(true),
            "eszamla 3 is an e-invoice"
        );

        let proforma = document_status(&Doc::new("D-1", "D").parse());
        assert_eq!(proforma.number, "D-1");
        assert_eq!(proforma.state, DocumentState::Live);
        assert_eq!(proforma.e_invoice, None, "a proforma has no appearance");
    }

    /// `get` folds the four reads into the status: a document of ours fills
    /// its slot, nothing and a collision leave it empty (a read must not fail
    /// on an answer), and a proforma szamlazz.hu no longer returns while the
    /// invoice or the prepayment references it is `consumed` by that
    /// document, on every combination of the two consumers: the invoice's
    /// reference, the prepayment's, the invoice's when both reference one
    /// (the invoice is read first), and no reference leaves the slot empty. A
    /// proforma still returned is reported as found, whatever references it.
    #[test]
    fn the_order_status_derives_a_consumed_proforma_from_its_consumer() {
        let proforma = || Doc::new("D-1", "D");
        let invoice = |referenced: Option<&'static str>| Doc {
            referenced_proforma: referenced,
            ..Doc::new("SZ-1", "SZ")
        };
        let prepayment = |referenced: Option<&'static str>| Doc {
            referenced_proforma: referenced,
            ..Doc::new("ES-1", "ES")
        };
        let other = || Doc {
            order: Some("ORD-2"),
            ..Doc::new("SZ-OTHER", "SZ")
        };
        let fold = |proforma: Lookup, invoice: Lookup, prepayment: Lookup| {
            order_status([
                (DocumentKind::Proforma, proforma),
                (DocumentKind::Invoice, invoice),
                (DocumentKind::Prepayment, prepayment),
                (DocumentKind::Final, Lookup::Absent),
            ])
        };
        let consumed_by = |by: &str| {
            Some(DocumentStatus::new(
                "D-1",
                DocumentState::Consumed { by: by.to_owned() },
            ))
        };

        // Every slot from its read: ours fills it, absent and a collision
        // leave it empty.
        let status = fold(
            Lookup::Ours(proforma().boxed()),
            Lookup::Ours(invoice(None).boxed()),
            Lookup::Collision(other().boxed()),
        );
        let found_proforma = status.proforma.as_ref().expect("the proforma slot");
        assert_eq!(found_proforma.number, "D-1");
        assert_eq!(found_proforma.state, DocumentState::Live);
        let found_invoice = status.invoice.as_ref().expect("the invoice slot");
        assert_eq!(found_invoice.number, "SZ-1");
        assert_eq!(found_invoice.state, DocumentState::Live);
        assert_eq!(found_invoice.gross, Some(dec!(1270)));
        assert_eq!(status.prepayment, None, "a collision is an empty slot");
        assert_eq!(status.r#final, None);

        // The proforma absent and referenced: consumed by the referencing
        // document.
        let status = fold(
            Lookup::Absent,
            Lookup::Ours(invoice(Some("D-1")).boxed()),
            Lookup::Absent,
        );
        assert_eq!(status.proforma, consumed_by("SZ-1"));
        let status = fold(
            Lookup::Absent,
            Lookup::Absent,
            Lookup::Ours(prepayment(Some("D-1")).boxed()),
        );
        assert_eq!(status.proforma, consumed_by("ES-1"));
        let status = fold(
            Lookup::Absent,
            Lookup::Ours(invoice(Some("D-1")).boxed()),
            Lookup::Ours(prepayment(Some("D-1")).boxed()),
        );
        assert_eq!(status.proforma, consumed_by("SZ-1"), "the invoice first");
        let status = fold(
            Lookup::Absent,
            Lookup::Ours(invoice(None).boxed()),
            Lookup::Ours(prepayment(Some("D-1")).boxed()),
        );
        assert_eq!(
            status.proforma,
            consumed_by("ES-1"),
            "the invoice references none: the prepayment's"
        );

        // The proforma absent and referenced by nothing: deleted, or never
        // issued.
        let status = fold(
            Lookup::Absent,
            Lookup::Ours(invoice(None).boxed()),
            Lookup::Ours(prepayment(None).boxed()),
        );
        assert_eq!(status.proforma, None);
        let status = fold(Lookup::Absent, Lookup::Absent, Lookup::Absent);
        assert_eq!(status, OrderStatus::default());

        // The proforma still returned: found, whatever references it.
        let status = fold(
            Lookup::Ours(proforma().boxed()),
            Lookup::Ours(invoice(Some("D-1")).boxed()),
            Lookup::Absent,
        );
        let found_proforma = status.proforma.as_ref().expect("the proforma slot");
        assert_eq!(found_proforma.number, "D-1");
        assert_eq!(found_proforma.state, DocumentState::Live, "not derived");

        // A collision under the proforma's id leaves the slot empty, and the
        // derivation fills it when a consumer references one.
        let status = fold(
            Lookup::Collision(other().boxed()),
            Lookup::Ours(invoice(Some("D-1")).boxed()),
            Lookup::Absent,
        );
        assert_eq!(status.proforma, consumed_by("SZ-1"));
    }
}
