//! `Szamlazz.Order.delete_proforma`: one read (the proforma under its
//! external id), a guard on what it found, the delete step (a one-shot write
//! without a retry of its own, refreshing the pinned number inside the run),
//! and the answer from data.

use std::ops::ControlFlow;

use restate_sdk::errors::HandlerError;
use restate_sdk::prelude::ObjectContext;

use super::prologue::Execution;
use super::support::{AnsweredCode, Fault, lookup};
use crate::contract::{
    DeleteProformaRequest, DeleteProformaResponse, DeleteReason, DocumentKind, IssuedKind,
};
use crate::gateway::{DeleteOutcome, FoundDocument, OwnershipOutcome};
use crate::identity::{ExternalId, Namespace, OrderKey};

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
        // Every fault is about this proforma.
        let about = |fault: Fault| fault.about(&order, Some(IssuedKind::Proforma), &proforma_id);
        let found = lookup(
            ctx,
            self,
            "lookup-proforma",
            &proforma_id,
            &order,
            kind.into(),
        )
        .await?;
        if let OwnershipOutcome::Live(doc) | OwnershipOutcome::Reversed(doc) = &found
            && doc.number != request.expected_number.as_str()
        {
            return Ok(DeleteProformaResponse::not_deleted(
                DeleteReason::TargetChanged,
            ));
        }
        let found =
            match delete_guard(found, request.force, &self.config.namespace).map_err(about)? {
                ControlFlow::Break(response) => return Ok(response),
                ControlFlow::Continue(found) => found,
            };

        let number = found.number.clone();
        let target_fault = |mut fault: Fault| {
            fault.message = format!("proforma {number}: {}", fault.message);
            about(fault)
        };
        let outcome = {
            let target_order = order.clone();
            let result = self
                .protected_write(
                    ctx,
                    &order,
                    &proforma_id,
                    crate::contract::recovery::WriteOperation::Delete {
                        number: number.clone(),
                    },
                    format!("delete-proforma-{number}"),
                    move |gateway, _marker| async move {
                        crate::gateway::recovery::WriteResult::delete(
                            gateway
                                .delete_proforma(&found, &target_order, request.force)
                                .await,
                        )
                    },
                )
                .await?;
            let crate::gateway::recovery::WriteResult::Delete(outcome) = result else {
                return Err(Fault::outcome_unknown("unexpected recovery operation").into());
            };
            outcome
        };
        delete_response(outcome, &self.config.namespace).map_err(|fault| target_fault(fault).into())
    }
}

/// The guard before the delete step, on what the lookup of the proforma's
/// external id found: `Break(response)` for a proforma nothing will be sent
/// for (nothing under the id: deleted earlier or consumed, `get` tells which;
/// another document under it, never touched; one with a credit entry without
/// `force`: szamlazz.hu has no guard against deleting a paid proforma, so
/// the delete step repeats this check on a fresh query), `Continue(found)`
/// for the pinned target: a proforma of ours,
/// live or, were szamlazz.hu ever to report one so, reversed (a proforma
/// cannot be stornoed; the delete is what removes it).
///
/// # Errors
///
/// The faults an answered read can be: another szamlazz.hu code
/// (`unavailable`, nothing may be concluded) or a credential code
/// (`credentials_rejected`). The caller attaches the proforma's identity.
fn delete_guard(
    found: OwnershipOutcome,
    force: bool,
    namespace: &Namespace,
) -> Result<ControlFlow<DeleteProformaResponse, Box<FoundDocument>>, Fault> {
    let found = match found {
        OwnershipOutcome::Absent => {
            return Ok(ControlFlow::Break(DeleteProformaResponse::absent()));
        }
        OwnershipOutcome::Collision(_) => {
            return Ok(ControlFlow::Break(DeleteProformaResponse::not_deleted(
                DeleteReason::ExternalIdCollision,
            )));
        }
        OwnershipOutcome::Live(found) | OwnershipOutcome::Reversed(found) => found,
        OwnershipOutcome::Api(answer) => {
            return Err(AnsweredCode::Inconclusive(answer).into_fault(namespace));
        }
        OwnershipOutcome::CredentialsRejected(answer) => {
            return Err(AnsweredCode::CredentialsRejected(answer).into_fault(namespace));
        }
    };
    if !found.credit_entries.is_empty() && !force {
        return Ok(ControlFlow::Break(DeleteProformaResponse::not_deleted(
            DeleteReason::ProformaPaid,
        )));
    }
    Ok(ControlFlow::Continue(found))
}

/// A lost/inconclusive answer or cancelled one-shot run: reconcile first.
const DELETE_RECOVERY: &str = "read get and query the expected number; reconcile the earlier send, then, if deletion is still intended, retry with a new Idempotency-Key and the same expected_number; never substitute a replacement automatically";

fn delete_unknown(lost: &impl std::fmt::Display) -> Fault {
    Fault::outcome_unknown(format!(
        "proforma deletion outcome unknown: {lost}; deletion may have landed: {DELETE_RECOVERY}"
    ))
}

/// The settled delete step as the response: deleted, and gone since the
/// lookup (335), are both `deleted`; szamlazz.hu refusing is `not_deleted`
/// with its code as the reason. Fresh guards can stop a paid/changed target.
///
/// # Errors
///
/// Failed fresh reads (`unavailable`), rejected credentials
/// (`credentials_rejected`), and a lost or inconclusive answer
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
            Ok(DeleteProformaResponse::not_deleted(rejection.code.into()))
        }
        DeleteOutcome::TargetChanged => Ok(DeleteProformaResponse::not_deleted(
            DeleteReason::TargetChanged,
        )),
        DeleteOutcome::Paid => Ok(DeleteProformaResponse::not_deleted(
            DeleteReason::ProformaPaid,
        )),
        DeleteOutcome::Api(answer) => Err(AnsweredCode::Inconclusive(answer).into_fault(namespace)),
        DeleteOutcome::GuardFailed(cause) => Err(Fault::unavailable(format!(
            "could not refresh the pinned proforma before deletion: {cause}; the outcome is not known; {DELETE_RECOVERY}"
        ))),
        DeleteOutcome::CredentialsRejected(answer) => {
            Err(AnsweredCode::CredentialsRejected(answer).into_fault(namespace))
        }
        DeleteOutcome::Lost(lost) => Err(delete_unknown(&lost)),
        DeleteOutcome::Inconclusive(answer) => {
            Err(delete_unknown(&answer).with_szamlazz_code(answer.code))
        }
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use restate_sdk::errors::TerminalError;

    use super::*;
    use crate::gateway::{Rejection, SzamlazzAnswer, Unanswered};
    use crate::test_support::{CreditRecord, Doc};

    fn namespace() -> Namespace {
        "acct".parse().expect("namespace")
    }

    fn fault_body(fault: Fault) -> (u16, serde_json::Value) {
        let error = TerminalError::try_from(fault).expect("known fault");
        let body = serde_json::from_str(error.message()).expect("json body");
        (error.code(), body)
    }

    /// The guard before the delete step: nothing under the proforma's
    /// external id is `deleted{reason: absent}` (deleted earlier or consumed;
    /// `get` tells which); another document under it is
    /// `not_deleted{external_id_collision}`, never touched, `force` or not; a
    /// proforma with a credit entry is `not_deleted{proforma_paid}` without
    /// `force` (szamlazz.hu has no such guard) and the one to delete with it;
    /// an unpaid one is the one to delete, whether szamlazz.hu reports it
    /// live or reversed (a proforma of ours is a proforma of ours). An
    /// answered code is a fault: another code `unavailable` carrying it, a
    /// credential code `credentials_rejected`.
    #[test]
    fn the_delete_guard_refuses_a_paid_proforma_unless_forced() {
        let namespace = namespace();
        let guard = |found, force| delete_guard(found, force, &namespace);
        let paid = Doc {
            credit_entries: &[CreditRecord::new(date(2026, 9, 4), "átutalás", "1270")],
            ..Doc::new("D-1", "D")
        };
        let unpaid = Doc::new("D-1", "D");
        let reversed = Doc {
            reversed: true,
            ..Doc::new("D-1", "D")
        };
        let other = Doc {
            order: Some("ORD-2"),
            ..Doc::new("D-OTHER", "D")
        };

        for force in [false, true] {
            assert_eq!(
                guard(OwnershipOutcome::Absent, force).expect("data"),
                ControlFlow::Break(DeleteProformaResponse::absent()),
                "force {force}"
            );
            assert_eq!(
                guard(OwnershipOutcome::Reversed(reversed.boxed()), force).expect("data"),
                ControlFlow::Continue(reversed.boxed()),
                "force {force}: a proforma of ours, however reported, is the one to delete"
            );
            assert_eq!(
                guard(OwnershipOutcome::Collision(other.boxed()), force).expect("data"),
                ControlFlow::Break(DeleteProformaResponse::not_deleted(
                    DeleteReason::ExternalIdCollision
                )),
                "force {force}"
            );
            assert_eq!(
                guard(OwnershipOutcome::Live(unpaid.boxed()), force).expect("data"),
                ControlFlow::Continue(unpaid.boxed()),
                "force {force}"
            );
        }
        assert_eq!(
            guard(OwnershipOutcome::Live(paid.boxed()), false).expect("data"),
            ControlFlow::Break(DeleteProformaResponse::not_deleted(
                DeleteReason::ProformaPaid
            ))
        );
        assert_eq!(
            guard(OwnershipOutcome::Live(paid.boxed()), true).expect("data"),
            ControlFlow::Continue(paid.boxed())
        );

        let (status, body) = fault_body(
            guard(
                OwnershipOutcome::Api(SzamlazzAnswer::new("57", "Hibás XML.")),
                true,
            )
            .expect_err("a fault"),
        );
        assert_eq!(status, 503, "{body}");
        assert_eq!(body["code"], "unavailable", "{body}");
        assert_eq!(body["szamlazz_code"], "57", "{body}");
        let (status, body) = fault_body(
            guard(
                OwnershipOutcome::CredentialsRejected(SzamlazzAnswer::new("3", "login")),
                true,
            )
            .expect_err("a fault"),
        );
        assert_eq!(status, 503, "{body}");
        assert_eq!(body["code"], "credentials_rejected", "{body}");
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
            DeleteProformaResponse::not_deleted(DeleteReason::Szamlazz("57".to_owned()))
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
                DeleteOutcome::Lost(Unanswered::Transport("connection reset".to_owned())),
                &namespace,
            )
            .expect_err("a fault"),
        );
        assert_eq!(status, 500, "{body}");
        assert_eq!(body["code"], "outcome_unknown", "{body}");
        assert_eq!(body["szamlazz_code"], serde_json::Value::Null, "{body}");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains("connection reset"), "{message}");
        assert!(
            message.contains("retry with a new Idempotency-Key"),
            "{message}"
        );
    }
}
