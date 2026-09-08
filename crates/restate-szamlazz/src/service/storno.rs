//! The storno protocol, proforma deletion and the `get` live view.

use std::ops::ControlFlow;
use std::sync::Arc;

use restate_sdk::errors::HandlerError;
use restate_sdk::prelude::{ObjectContext, SharedObjectContext};

use super::prologue::Execution;
use super::support::{
    Fault, Lookup, StornoIntent, StornoVerdict, after_storno_lookup, reversed_response,
    storno_response, verified_document,
};
use super::support::{object, shared};
use crate::contract::{
    ConflictReason, DeleteProformaRequest, DeleteProformaResponse, DocumentKind, DocumentState,
    DocumentStatus, IssuedKind, OrderStatus, StornoOutcome, StornoRequest, StornoResponse,
};
use crate::gateway::{DeleteOutcome, FoundDocument, issued_kind_of};
use crate::identity::Namespace;
use crate::identity::{ExternalId, OrderKey};

impl Execution {
    // ----- storno_invoice ---------------------------------------------

    /// `storno_invoice`, on the `order` the handler parsed from its key.
    pub(super) async fn storno(
        &self,
        ctx: &ObjectContext<'_>,
        order: OrderKey,
        request: StornoRequest,
    ) -> Result<StornoResponse, HandlerError> {
        let StornoRequest {
            invoice_number: number,
            comment,
        } = request;
        let number = String::from(number);
        let gateway = &self.gateway;
        let namespace = &self.config.namespace;
        let storno_id = ExternalId::for_storno(namespace, &order, &number);

        // Step 1: verify the document.
        let found = match self
            .verify_for_storno(ctx, &order, &number, &storno_id)
            .await?
        {
            ControlFlow::Continue(found) => found,
            ControlFlow::Break(response) => return Ok(response),
        };
        let kind = issued_kind_of(&found.document_type);
        // Every fault from here on is about this storno.
        let about = |fault: Fault| fault.about(&order, kind, storno_id.as_str());
        // The intent is a pure function of the verified document: a `telj`
        // it does not carry is a fault after every answer that needs no
        // send.
        let intent = StornoIntent::from_verified(
            &found,
            gateway.account(),
            number.clone(),
            storno_id.clone(),
            comment,
        )
        .map_err(about)?;

        // Step 2: lookup: a storno of ours already under the id.
        let looked_up = object::lookup_storno(ctx, self, &intent)
            .await
            .map_err(about)?;
        if let ControlFlow::Break(response) =
            after_storno_lookup(looked_up, &number, namespace).map_err(about)?
        {
            return Ok(response);
        }

        // Step 3: the storno step, under the issue policy. Any `Err` from the
        // run (exhaustion (500) or cancellation (409)) is `outcome_unknown`
        // about this storno: nothing is recorded, the next invocation's
        // verify and lookup find whatever landed.
        let outcome = object::storno_step(ctx, self, &intent)
            .await
            .map_err(|error| {
                about(Fault::outcome_unknown(format!(
                    "the storno step ended without a confirmed outcome ({}): {}; retry with a new Idempotency-Key",
                    error.code(),
                    error.message()
                )))
            })?;

        // Step 4: branch on data.
        storno_response(outcome, number, namespace).map_err(|fault| about(fault).into())
    }

    /// Step 1 of the storno protocol: the document must be known, carry this
    /// order's number and be a live invoice kind. One read (the verify),
    /// then [`storno_verdict`] on what it found; `Break(response)` is the
    /// answer for anything that stops the storno before it is sent (not
    /// managed, already reversed, not stornoable): a domain outcome, not a
    /// fault.
    async fn verify_for_storno(
        &self,
        ctx: &ObjectContext<'_>,
        order: &OrderKey,
        number: &str,
        storno_id: &ExternalId,
    ) -> Result<ControlFlow<StornoResponse, Box<FoundDocument>>, HandlerError> {
        let namespace = &self.config.namespace;
        let about = |fault: Fault| fault.about(order, None, storno_id.as_str());
        let found = object::verify(ctx, self, format!("verify-storno-{number}"), number)
            .await
            .map_err(about)?;
        let found = verified_document(found, number, namespace).map_err(about)?;
        match storno_verdict(&found, order, number) {
            StornoVerdict::Proceed => Ok(ControlFlow::Continue(found)),
            StornoVerdict::Answered(response) => Ok(ControlFlow::Break(response)),
            StornoVerdict::AlreadyReversed => {
                // Idempotent: already reversed by anyone. The storno number is
                // best effort; a cancelled invocation propagates as such.
                let storno_number =
                    object::storno_number_of(ctx, self, order, number, storno_id).await?;
                Ok(ControlFlow::Break(reversed_response(number, storno_number)))
            }
        }
    }

    // ----- delete_proforma ---------------------------------------

    /// `delete_proforma`, on the `order` the handler parsed from its key:
    /// one read (the proforma under its external id), [`delete_guard`] on
    /// what it found, the delete step, [`delete_response`] on what it
    /// settled.
    pub(super) async fn delete(
        &self,
        ctx: &ObjectContext<'_>,
        order: OrderKey,
        request: DeleteProformaRequest,
    ) -> Result<DeleteProformaResponse, HandlerError> {
        let kind = DocumentKind::Proforma;
        let proforma_id = ExternalId::for_kind(&self.config.namespace, &order, kind);
        let found = object::lookup(
            ctx,
            self,
            "proforma-for-delete",
            &proforma_id,
            &order,
            kind.into(),
        )
        .await?;
        let found = match delete_guard(found, request.force) {
            ControlFlow::Break(response) => return Ok(response),
            ControlFlow::Continue(found) => found,
        };

        let outcome = {
            let gateway = Arc::clone(&self.gateway);
            let number = found.number;
            object::run_once(
                ctx,
                format!("delete-proforma-{number}"),
                move || async move { gateway.delete_proforma(&number).await },
            )
            .await?
        };
        // Every fault of the settled step is about this proforma.
        let about =
            |fault: Fault| fault.about(&order, Some(IssuedKind::Proforma), proforma_id.as_str());
        delete_response(outcome, &self.config.namespace).map_err(|fault| about(fault).into())
    }

    // ----- get -------------------------------------------------------------

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
            let looked_up = shared::lookup(
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

/// Step 1's decision on the verified document, for `storno_invoice` on
/// `order`: `number` is the invoice as the caller named it, which every
/// answer echoes. In the order of the answers: a document that does not
/// carry this order's number is `conflict{not_managed}` (the order's handler
/// acts on nothing but its own, and reads nothing further); one already
/// reversed, by anyone, is [`StornoVerdict::AlreadyReversed`] (the storno
/// number is the hint's, which the handler reads best effort); a `tipus`
/// szamlazz.hu cannot reverse (a proforma, a delivery note, a storno) is
/// `rejected{not_stornoable}`; a live `SZ`, `ES`, `VS` or `HS` proceeds.
fn storno_verdict(found: &FoundDocument, order: &OrderKey, number: &str) -> StornoVerdict {
    if !found.carries_order(order) {
        return StornoVerdict::Answered(
            StornoResponse::new(StornoOutcome::Conflict, number)
                .with_conflict_reason(ConflictReason::NotManaged),
        );
    }
    if !found.is_live() {
        return StornoVerdict::AlreadyReversed;
    }
    if !matches!(found.document_type.as_str(), "SZ" | "ES" | "VS" | "HS") {
        return StornoVerdict::Answered(not_stornoable(number.to_owned()));
    }
    StornoVerdict::Proceed
}

/// The answer for a document the verify step sees szamlazz.hu cannot reverse
/// (proformas, delivery notes, stornos), before anything is sent.
fn not_stornoable(number: String) -> StornoResponse {
    StornoResponse::new(StornoOutcome::Rejected, number)
        .with_code("not_stornoable")
        .with_message("the document cannot be reversed: only invoices can be stornoed")
}

/// The guard before the delete step, on what the lookup of the proforma's
/// external id found: `Break(response)` for a proforma nothing will be sent
/// for (nothing under the id: deleted earlier or consumed, `get` tells which;
/// another document under it, never touched; one with a credit entry
/// without `force`: szamlazz.hu has no guard against deleting a paid
/// proforma, so this is it), `Continue(found)` for the one to delete.
fn delete_guard(
    found: Lookup,
    force: bool,
) -> ControlFlow<DeleteProformaResponse, Box<FoundDocument>> {
    let found = match found {
        Lookup::Absent => return ControlFlow::Break(DeleteProformaResponse::absent()),
        Lookup::Collision(_) => {
            return ControlFlow::Break(DeleteProformaResponse::not_deleted(
                "external_id_collision",
            ));
        }
        Lookup::Ours(found) => found,
    };
    if !found.payments.is_empty() && !force {
        return ControlFlow::Break(DeleteProformaResponse::not_deleted("proforma_paid"));
    }
    ControlFlow::Continue(found)
}

/// The settled delete step as the response: deleted, and gone since the
/// lookup (335), are both `deleted`; szamlazz.hu refusing is `not_deleted`
/// with its code as the reason.
///
/// # Errors
///
/// Rejected credentials (`credentials_rejected`), and a lost reply
/// (`outcome_unknown`: the step has no retry of its own, and the next call's
/// lookup tells). The caller attaches the proforma's identity.
fn delete_response(
    outcome: DeleteOutcome,
    namespace: &Namespace,
) -> Result<DeleteProformaResponse, Fault> {
    match outcome {
        DeleteOutcome::Deleted | DeleteOutcome::AlreadyGone => {
            Ok(DeleteProformaResponse::deleted())
        }
        DeleteOutcome::Rejected { code, .. } => Ok(DeleteProformaResponse::not_deleted(code)),
        DeleteOutcome::CredentialsRejected { code, message } => {
            Err(Fault::credentials_rejected(namespace, code, message))
        }
        DeleteOutcome::Transport(message) => Err(Fault::outcome_unknown(format!(
            "proforma deletion outcome unknown: {message}; retry with a new Idempotency-Key"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use restate_sdk::errors::TerminalError;
    use rust_decimal::dec;

    use super::*;
    use crate::test_support::{CreditRecord, Doc};

    fn ord_1() -> OrderKey {
        OrderKey::parse("ORD-1").expect("order")
    }

    fn namespace() -> Namespace {
        "acct".parse().expect("namespace")
    }

    fn fault_body(fault: Fault) -> (u16, serde_json::Value) {
        let error = TerminalError::from(fault);
        let body = serde_json::from_str(error.message()).expect("json body");
        (error.code(), body)
    }

    /// The answer for a document szamlazz.hu cannot reverse, as the verdict
    /// wraps it: `rejected` under the worker's own code `not_stornoable`
    /// (no szamlazz.hu code: nothing was sent), echoing the number as the
    /// caller named it, with no storno number and no conflict reason.
    #[test]
    fn not_stornoable_is_a_rejection_under_the_workers_own_code() {
        let response = not_stornoable("D-1".to_owned());
        assert_eq!(response.outcome, StornoOutcome::Rejected);
        assert_eq!(response.invoice_number, "D-1");
        assert_eq!(response.code.as_deref(), Some("not_stornoable"));
        assert_eq!(
            response.message.as_deref(),
            Some("the document cannot be reversed: only invoices can be stornoed")
        );
        assert_eq!(response.storno_number, None);
        assert_eq!(response.conflict_reason, None);
    }

    /// Step 1 of the storno protocol on the verified document: this order's
    /// live invoice kind (`SZ`, `ES`, `VS`, `HS`) proceeds; a document that
    /// does not carry this order's number (another order's, none, an empty
    /// or whitespace `rendelesszam`, as rendered and as a parsed value the
    /// worker reads on its own) is `conflict{not_managed}` before
    /// anything else is read; one already reversed, by anyone, is
    /// `AlreadyReversed` (the answer is known, the storno number is the
    /// hint's); a `tipus` szamlazz.hu cannot reverse (a proforma, a delivery
    /// note, a storno) is `rejected{not_stornoable}`. The order of the checks
    /// is the order of the answers: another order's reversed proforma is
    /// `not_managed`, this order's reversed proforma is `AlreadyReversed`.
    #[test]
    fn the_storno_verdict_refuses_before_it_reverses() {
        let order = ord_1();
        let verdict = |doc: &Doc| storno_verdict(&doc.parse(), &order, doc.number);

        for tipus in ["SZ", "ES", "VS", "HS"] {
            assert_eq!(
                verdict(&Doc::new("X-1", tipus)),
                StornoVerdict::Proceed,
                "{tipus}"
            );
        }

        for not_ours in [Some("ORD-2"), None, Some(""), Some("  ")] {
            let StornoVerdict::Answered(response) = verdict(&Doc {
                order: not_ours,
                reversed: true,
                ..Doc::new("D-1", "D")
            }) else {
                panic!("rendelesszam {not_ours:?} is answered");
            };
            assert_eq!(response.outcome, StornoOutcome::Conflict, "{not_ours:?}");
            assert_eq!(
                response.conflict_reason,
                Some(ConflictReason::NotManaged),
                "{not_ours:?}"
            );
            assert_eq!(response.invoice_number, "D-1", "{not_ours:?}");
            assert_eq!(response.storno_number, None, "{not_ours:?}");
        }
        // The projection's own reading of the parsed value, with the parser's
        // normalisation out of the way.
        for raw in ["", "   ", "ORD-2"] {
            let StornoVerdict::Answered(response) =
                storno_verdict(&Doc::default().assigned_order(Some(raw)), &order, "SZ-1")
            else {
                panic!("order_number {raw:?} is answered");
            };
            assert_eq!(
                response.conflict_reason,
                Some(ConflictReason::NotManaged),
                "order_number {raw:?}: the worker's own reading"
            );
        }
        assert_eq!(
            storno_verdict(
                &Doc::default().assigned_order(Some(" ORD-1 ")),
                &order,
                "SZ-1"
            ),
            StornoVerdict::Proceed,
            "a padded order number carries the order"
        );

        for tipus in ["SZ", "D"] {
            assert_eq!(
                verdict(&Doc {
                    reversed: true,
                    ..Doc::new("X-1", tipus)
                }),
                StornoVerdict::AlreadyReversed,
                "{tipus}: reversed is answered before the kind is checked"
            );
        }

        for (number, tipus) in [("D-1", "D"), ("SL-1", "SL"), ("SS-1", "SS")] {
            let StornoVerdict::Answered(response) = verdict(&Doc {
                referenced_invoice: (tipus == "SS").then_some("SZ-1"),
                ..Doc::new(number, tipus)
            }) else {
                panic!("{tipus} is answered");
            };
            assert_eq!(response.outcome, StornoOutcome::Rejected, "{tipus}");
            assert_eq!(response.code.as_deref(), Some("not_stornoable"), "{tipus}");
            assert_eq!(response.invoice_number, number, "{tipus}");
            assert_eq!(response.conflict_reason, None, "{tipus}");
            assert!(
                response
                    .message
                    .as_deref()
                    .is_some_and(|message| message.contains("only invoices can be stornoed")),
                "{tipus}: {response:?}"
            );
        }
    }

    /// The guard before the delete step: nothing under the proforma's
    /// external id is `deleted{reason: absent}` (deleted earlier or consumed;
    /// `get` tells which); another document under it is
    /// `not_deleted{external_id_collision}`, never touched, `force` or not; a
    /// proforma with a credit entry is `not_deleted{proforma_paid}` without
    /// `force` (szamlazz.hu has no such guard) and the one to delete with it;
    /// an unpaid one is the one to delete.
    #[test]
    fn the_delete_guard_refuses_a_paid_proforma_unless_forced() {
        let paid = Doc {
            payments: &[CreditRecord::new(date(2026, 9, 4), "átutalás", "1270")],
            ..Doc::new("D-1", "D")
        };
        let unpaid = Doc::new("D-1", "D");
        let other = Doc {
            order: Some("ORD-2"),
            ..Doc::new("D-OTHER", "D")
        };

        for force in [false, true] {
            assert_eq!(
                delete_guard(Lookup::Absent, force),
                ControlFlow::Break(DeleteProformaResponse::absent()),
                "force {force}"
            );
            assert_eq!(
                delete_guard(Lookup::Collision(other.boxed()), force),
                ControlFlow::Break(DeleteProformaResponse::not_deleted("external_id_collision")),
                "force {force}"
            );
            assert_eq!(
                delete_guard(Lookup::Ours(unpaid.boxed()), force),
                ControlFlow::Continue(unpaid.boxed()),
                "force {force}"
            );
        }
        assert_eq!(
            delete_guard(Lookup::Ours(paid.boxed()), false),
            ControlFlow::Break(DeleteProformaResponse::not_deleted("proforma_paid"))
        );
        assert_eq!(
            delete_guard(Lookup::Ours(paid.boxed()), true),
            ControlFlow::Continue(paid.boxed())
        );
    }

    /// The settled delete step as the response: deleted, and gone since the
    /// lookup (335), are both `deleted`; szamlazz.hu refusing is
    /// `not_deleted` with its code as the reason; rejected credentials are
    /// the `credentials_rejected` fault, a lost reply the `outcome_unknown`
    /// one naming the failure and the next step.
    #[test]
    fn the_delete_response_settles_every_answer() {
        let namespace = namespace();

        assert_eq!(
            delete_response(DeleteOutcome::Deleted, &namespace).expect("data"),
            DeleteProformaResponse::deleted()
        );
        assert_eq!(
            delete_response(DeleteOutcome::AlreadyGone, &namespace).expect("data"),
            DeleteProformaResponse::deleted()
        );
        assert_eq!(
            delete_response(
                DeleteOutcome::Rejected {
                    code: "57".to_owned(),
                    message: "Hibás XML.".to_owned(),
                },
                &namespace,
            )
            .expect("data"),
            DeleteProformaResponse::not_deleted("57")
        );

        let (status, body) = fault_body(
            delete_response(
                DeleteOutcome::CredentialsRejected {
                    code: "3".to_owned(),
                    message: "Sikertelen bejelentkezés.".to_owned(),
                },
                &namespace,
            )
            .expect_err("a fault"),
        );
        assert_eq!(status, 503, "{body}");
        assert_eq!(body["code"], "credentials_rejected", "{body}");
        assert_eq!(body["szamlazz_code"], "3", "{body}");

        let (status, body) = fault_body(
            delete_response(
                DeleteOutcome::Transport("connection reset".to_owned()),
                &namespace,
            )
            .expect_err("a fault"),
        );
        assert_eq!(status, 500, "{body}");
        assert_eq!(body["code"], "outcome_unknown", "{body}");
        assert_eq!(body.get("szamlazz_code"), None, "{body}");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains("connection reset"), "{message}");
        assert!(
            message.contains("retry with a new Idempotency-Key"),
            "{message}"
        );
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
