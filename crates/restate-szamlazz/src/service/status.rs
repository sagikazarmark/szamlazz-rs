//! `Szamlazz.Order.get`: a non-atomic observation of the order's four external
//! ids, four separately journaled reads under the read policy and a fold.
//! Reads run alongside exclusive writes and can mix observation times and
//! replay ages; this is neither a snapshot nor an invocation-completion barrier.

use restate_sdk::errors::HandlerError;
use restate_sdk::prelude::SharedObjectContext;

use super::prologue::Execution;
use super::support::{AnsweredCode, Fault, lookup};
use crate::contract::{DocumentKind, DocumentState, DocumentStatus, OrderStatus};
use crate::gateway::{FoundDocument, OwnershipOutcome};
use crate::identity::{ExternalId, Namespace, OrderKey};

impl Execution {
    /// Observe the order's four external ids with separately journaled,
    /// sequential read-only steps under the read policy, on the
    /// `order` the handler parsed from its key. Answered faults stop the reads
    /// immediately; successful observations determine proforma consumption.
    pub(super) async fn status(
        &self,
        ctx: &SharedObjectContext<'_>,
        order: OrderKey,
    ) -> Result<OrderStatus, HandlerError> {
        let mut status = OrderStatus::default();
        for kind in DocumentKind::ALL {
            let external_id = ExternalId::for_kind(&self.config.namespace, &order, kind);
            let looked_up = lookup(
                ctx,
                self,
                format!("lookup-{kind}"),
                &external_id,
                &order,
                kind.into(),
            )
            .await?;
            record_observation(&mut status, kind, looked_up, &self.config.namespace)?;
        }
        Ok(with_consumed_proforma(status))
    }
}

/// The `get` status folded from the four reads: a document of ours, live or
/// reversed, fills the slot of its kind ([`document_status`]); nothing and a
/// collision leave it `None` (a read must not fail on an answer, and the
/// issuing handlers are the ones that refuse a collision). A proforma
/// szamlazz.hu no longer returns while the invoice or the prepayment carries
/// `hivdijbekszam` was consumed by that document: `{state: consumed, by}`,
/// the invoice's reference before the prepayment's.
///
/// # Errors
///
/// The faults an answered read can be: another szamlazz.hu code
/// (`unavailable`, nothing may be concluded) or a credential code
/// (`credentials_rejected`). `get` names no document in them: which of the
/// four reads drew the code is in the message.
#[cfg(test)]
fn order_status(
    found: impl IntoIterator<Item = (DocumentKind, OwnershipOutcome)>,
    namespace: &Namespace,
) -> Result<OrderStatus, Fault> {
    let mut status = OrderStatus::default();
    for (kind, looked_up) in found {
        record_observation(&mut status, kind, looked_up, namespace)?;
    }
    Ok(with_consumed_proforma(status))
}

/// Classify a read before starting the next one, preserving its fault and warning.
fn record_observation(
    status: &mut OrderStatus,
    kind: DocumentKind,
    looked_up: OwnershipOutcome,
    namespace: &Namespace,
) -> Result<(), Fault> {
    match looked_up {
        OwnershipOutcome::Live(found) | OwnershipOutcome::Reversed(found) => {
            status.set(kind, Some(document_status(&found)));
        }
        OwnershipOutcome::Absent | OwnershipOutcome::Collision(_) => {}
        OwnershipOutcome::Api(answer) => {
            return Err(AnsweredCode::Inconclusive(answer).into_fault(namespace));
        }
        OwnershipOutcome::CredentialsRejected(answer) => {
            return Err(AnsweredCode::CredentialsRejected(answer).into_fault(namespace));
        }
    }
    Ok(())
}

fn with_consumed_proforma(mut status: OrderStatus) -> OrderStatus {
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
    status.credit_entries = found.credit_entry_amounts();
    status
        .referenced_proforma
        .clone_from(&found.referenced_proforma_number);
    status.e_invoice = found.e_invoice();

    status
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use restate_sdk::errors::TerminalError;
    use rust_decimal::dec;

    use super::*;
    use crate::gateway::SzamlazzAnswer;
    use crate::test_support::{CreditRecord, Doc};

    fn namespace() -> Namespace {
        "acct".parse().expect("namespace")
    }

    /// The `get` projection of a document of ours: its number, `live` or
    /// `reversed` (the storno number is not looked up here), the totals,
    /// the credit entry amounts in szamlazz.hu's order, the proforma it
    /// references, and its appearance as a storno would lift it (`None` on a
    /// proforma).
    #[test]
    fn the_document_status_projects_what_szamlazz_reports() {
        let live = document_status(
            &Doc {
                credit_entries: &[
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
        assert_eq!(live.credit_entries, [dec!(500), dec!(770)]);
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
        assert!(reversed_status.credit_entries.is_empty());
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

    /// `get` folds the four reads into the status: a document of ours, live
    /// or reversed ([`a_reversed_document_of_ours_fills_its_slot_as_reversed`]),
    /// fills its slot, nothing and a collision leave it empty (a read must not
    /// fail on an answer), and a proforma szamlazz.hu no longer
    /// returns while the invoice or the prepayment references it is
    /// `consumed` by that document, on every combination of the two
    /// consumers: the invoice's reference, the prepayment's, the invoice's
    /// when both reference one (the invoice is read first), and no reference
    /// leaves the slot empty. A proforma still returned is reported as found,
    /// whatever references it. An answered code on any of the four reads is a
    /// fault: another code `unavailable` carrying it, a credential code
    /// `credentials_rejected`.
    #[test]
    fn the_order_status_derives_a_consumed_proforma_from_its_consumer() {
        use OwnershipOutcome::{Absent, Collision, Live};

        let namespace = namespace();
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
        let fold = |proforma: OwnershipOutcome,
                    invoice: OwnershipOutcome,
                    prepayment: OwnershipOutcome| {
            order_status(
                [
                    (DocumentKind::Proforma, proforma),
                    (DocumentKind::Invoice, invoice),
                    (DocumentKind::Prepayment, prepayment),
                    (DocumentKind::Final, Absent),
                ],
                &namespace,
            )
            .expect("data")
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
            Live(proforma().boxed()),
            Live(invoice(None).boxed()),
            Collision(other().boxed()),
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
        let status = fold(Absent, Live(invoice(Some("D-1")).boxed()), Absent);
        assert_eq!(status.proforma, consumed_by("SZ-1"));
        let status = fold(Absent, Absent, Live(prepayment(Some("D-1")).boxed()));
        assert_eq!(status.proforma, consumed_by("ES-1"));
        let status = fold(
            Absent,
            Live(invoice(Some("D-1")).boxed()),
            Live(prepayment(Some("D-1")).boxed()),
        );
        assert_eq!(status.proforma, consumed_by("SZ-1"), "the invoice first");
        let status = fold(
            Absent,
            Live(invoice(None).boxed()),
            Live(prepayment(Some("D-1")).boxed()),
        );
        assert_eq!(
            status.proforma,
            consumed_by("ES-1"),
            "the invoice references none: the prepayment's"
        );

        // The proforma absent and referenced by nothing: deleted, or never
        // issued.
        let status = fold(
            Absent,
            Live(invoice(None).boxed()),
            Live(prepayment(None).boxed()),
        );
        assert_eq!(status.proforma, None);
        let status = fold(Absent, Absent, Absent);
        assert_eq!(status, OrderStatus::default());

        // The proforma still returned: found, whatever references it.
        let status = fold(
            Live(proforma().boxed()),
            Live(invoice(Some("D-1")).boxed()),
            Absent,
        );
        let found_proforma = status.proforma.as_ref().expect("the proforma slot");
        assert_eq!(found_proforma.number, "D-1");
        assert_eq!(found_proforma.state, DocumentState::Live, "not derived");

        // A collision under the proforma's id leaves the slot empty, and the
        // derivation fills it when a consumer references one.
        let status = fold(
            Collision(other().boxed()),
            Live(invoice(Some("D-1")).boxed()),
            Absent,
        );
        assert_eq!(status.proforma, consumed_by("SZ-1"));
    }

    /// A reversed document of ours fills its slot as `reversed`: `get` reports
    /// what szamlazz.hu holds, live or not (the storno number is the
    /// lookup step's to name, not this read's).
    #[test]
    fn a_reversed_document_of_ours_fills_its_slot_as_reversed() {
        let status = order_status(
            [(
                DocumentKind::Invoice,
                OwnershipOutcome::Reversed(
                    Doc {
                        reversed: true,
                        ..Doc::new("SZ-1", "SZ")
                    }
                    .boxed(),
                ),
            )],
            &namespace(),
        )
        .expect("data");
        let found_invoice = status.invoice.as_ref().expect("the invoice slot");
        assert_eq!(found_invoice.number, "SZ-1");
        assert_eq!(
            found_invoice.state,
            DocumentState::Reversed {
                storno_number: None
            }
        );
        assert_eq!(status.proforma, None);
    }

    /// An answered code on any of the four reads is a fault, with the
    /// szamlazz.hu code beside the token: another code is `unavailable`
    /// (nothing may be concluded), a credential code `credentials_rejected`.
    #[test]
    fn an_answered_code_on_a_get_read_is_a_fault() {
        let namespace = namespace();
        let fault_body = |fault: Fault| {
            let error = TerminalError::try_from(fault).expect("known fault");
            let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
            (error.code(), body)
        };

        let (status, body) = fault_body(
            order_status(
                [
                    (DocumentKind::Proforma, OwnershipOutcome::Absent),
                    (
                        DocumentKind::Invoice,
                        OwnershipOutcome::Api(SzamlazzAnswer::new("57", "Hibás XML.")),
                    ),
                ],
                &namespace,
            )
            .expect_err("a fault"),
        );
        assert_eq!(status, 503, "{body}");
        assert_eq!(body["code"], "unavailable", "{body}");
        assert_eq!(body["szamlazz_code"], "57", "{body}");

        let (status, body) = fault_body(
            order_status(
                [(
                    DocumentKind::Proforma,
                    OwnershipOutcome::CredentialsRejected(SzamlazzAnswer::new("3", "login")),
                )],
                &namespace,
            )
            .expect_err("a fault"),
        );
        assert_eq!(status, 503, "{body}");
        assert_eq!(body["code"], "credentials_rejected", "{body}");
        assert_eq!(body["szamlazz_code"], "3", "{body}");
    }
}
