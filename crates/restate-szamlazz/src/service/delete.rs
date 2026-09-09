//! `Szamlazz.Order.delete_proforma`: one read (the proforma under its
//! external id), a guard on what it found, the delete step (a one-shot write
//! without a retry of its own), and the answer from data.

use std::ops::ControlFlow;
use std::sync::Arc;

use restate_sdk::errors::HandlerError;
use restate_sdk::prelude::ObjectContext;

use super::prologue::Execution;
use super::support::{Fault, Lookup, lookup, run_once};
use crate::contract::{DeleteProformaRequest, DeleteProformaResponse, DocumentKind, IssuedKind};
use crate::gateway::{DeleteOutcome, FoundDocument};
use crate::identity::OrderKey;
use crate::identity::{ExternalId, Namespace};

impl Execution {
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
        let found = lookup(
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
            run_once(
                ctx,
                format!("delete-proforma-{number}"),
                move || async move { gateway.delete_proforma(&number).await },
            )
            .await?
        };
        // Every fault of the settled step is about this proforma.
        let about = |fault: Fault| fault.about(&order, Some(IssuedKind::Proforma), &proforma_id);
        delete_response(outcome, &self.config.namespace).map_err(|fault| about(fault).into())
    }
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
        DeleteOutcome::Rejected(rejection) => {
            Ok(DeleteProformaResponse::not_deleted(rejection.code))
        }
        DeleteOutcome::CredentialsRejected(answer) => {
            Err(Fault::credentials_rejected(namespace, answer))
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

    use super::*;
    use crate::gateway::{Rejection, SzamlazzAnswer};
    use crate::test_support::{CreditRecord, Doc};

    fn namespace() -> Namespace {
        "acct".parse().expect("namespace")
    }

    fn fault_body(fault: Fault) -> (u16, serde_json::Value) {
        let error = TerminalError::from(fault);
        let body = serde_json::from_str(error.message()).expect("json body");
        (error.code(), body)
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
                DeleteOutcome::Rejected(Rejection::from(SzamlazzAnswer::new("57", "Hibás XML."))),
                &namespace,
            )
            .expect("data"),
            DeleteProformaResponse::not_deleted("57")
        );

        let (status, body) = fault_body(
            delete_response(
                DeleteOutcome::CredentialsRejected(SzamlazzAnswer::new(
                    "3",
                    "Sikertelen bejelentkezés.",
                )),
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
}
