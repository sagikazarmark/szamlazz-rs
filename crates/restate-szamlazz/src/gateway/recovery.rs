//! Read-only evidence for retained Order writes.

use serde::{Deserialize, Serialize};

use super::{CreateOutcome, DeleteOutcome, Gateway, QueryOutcome, StornoOutcome};
use crate::contract::Selector;
use crate::contract::recovery::{UnresolvedWrite, WriteOperation};

/// A protected write result; uncertainty is journaled data, never a send retry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum WriteResult {
    Create(CreateOutcome),
    Storno(StornoOutcome),
    Delete(DeleteOutcome),
    /// A recovery read answered a code rather than evidence.
    Answered {
        credentials: bool,
        answer: super::SzamlazzAnswer,
    },
    Unresolved,
}

impl WriteResult {
    pub(crate) fn delete(outcome: DeleteOutcome) -> Self {
        match outcome {
            outcome @ (DeleteOutcome::Deleted
            | DeleteOutcome::AlreadyGone
            | DeleteOutcome::Rejected(_)
            | DeleteOutcome::Paid
            | DeleteOutcome::TargetChanged
            | DeleteOutcome::GuardFailed(_)
            | DeleteOutcome::Api(_)
            | DeleteOutcome::CredentialsRejected(_)) => Self::Delete(outcome),
            _ => Self::Unresolved,
        }
    }
}

impl Gateway {
    pub(crate) async fn protected_create(
        &self,
        request: super::CreateStepRequest<'_>,
        _marker: &UnresolvedWrite,
    ) -> WriteResult {
        match self.settled_by_query(&request, true).await {
            Ok(Some(outcome)) => return WriteResult::Create(outcome),
            Ok(None) | Err(super::QueryError::NotFound) => {}
            Err(super::QueryError::Api(answer)) => {
                return WriteResult::Create(CreateOutcome::Api(answer));
            }
            Err(super::QueryError::CredentialsRejected(answer)) => {
                return WriteResult::Create(CreateOutcome::CredentialsRejected(answer));
            }
            Err(super::QueryError::Unavailable(message)) => {
                return WriteResult::Create(CreateOutcome::Unavailable { message });
            }
            Err(_) => return WriteResult::Unresolved,
        }
        match self.create_send(&request, true).await {
            Ok(
                outcome @ (CreateOutcome::Issued(_)
                | CreateOutcome::Rejected(_)
                | CreateOutcome::DuplicateOrderNumber { .. }
                | CreateOutcome::CredentialsRejected(_)),
            ) => WriteResult::Create(outcome),
            // Every post-send query goes through the marker's stronger identity checks.
            _ => WriteResult::Unresolved,
        }
    }

    pub(crate) async fn protected_storno(
        &self,
        request: super::StornoStepRequest<'_>,
        _marker: &UnresolvedWrite,
    ) -> WriteResult {
        match self.storno_settled_by_query(&request).await {
            Ok(Some(outcome)) => return WriteResult::Storno(outcome),
            Ok(None) | Err(super::QueryError::NotFound) => {}
            Err(super::QueryError::Api(answer)) => {
                return WriteResult::Storno(StornoOutcome::Api(answer));
            }
            Err(super::QueryError::CredentialsRejected(answer)) => {
                return WriteResult::Storno(StornoOutcome::CredentialsRejected(answer));
            }
            Err(super::QueryError::Unavailable(message)) => {
                return WriteResult::Storno(StornoOutcome::Unavailable { message });
            }
            Err(_) => return WriteResult::Unresolved,
        }
        match self.storno_send(request, true).await {
            Ok(
                outcome @ (StornoOutcome::Reversed(_)
                | StornoOutcome::Rejected(_)
                | StornoOutcome::NotStornoable
                | StornoOutcome::CredentialsRejected(_)),
            ) => WriteResult::Storno(outcome),
            _ => WriteResult::Unresolved,
        }
    }

    /// Query positive evidence for the exact marker, never sending a mutation.
    /// Absence and every inconclusive answer preserve uncertainty.
    pub(crate) async fn reconcile_write(
        &self,
        marker: &UnresolvedWrite,
        candidate: Option<&str>,
    ) -> WriteResult {
        match self.reconcile_write_checked(marker, candidate).await {
            Ok(WriteResult::Answered { .. }) | Err(_) => WriteResult::Unresolved,
            Ok(result) => result,
        }
    }

    pub(crate) async fn reconcile_write_checked(
        &self,
        marker: &UnresolvedWrite,
        candidate: Option<&str>,
    ) -> Result<WriteResult, super::Unanswered> {
        let selector = Selector::ExternalId(marker.external_id.clone());
        let found = match self.query(&selector).await? {
            QueryOutcome::Found(found) => found,
            QueryOutcome::CredentialsRejected(answer) => {
                return Ok(WriteResult::Answered {
                    credentials: true,
                    answer,
                });
            }
            QueryOutcome::Api(answer) => {
                return Ok(WriteResult::Answered {
                    credentials: false,
                    answer,
                });
            }
            QueryOutcome::NotFound => return Ok(WriteResult::Unresolved),
        };
        if candidate.is_some_and(|number| found.number != number) {
            return Ok(WriteResult::Unresolved);
        }
        Ok(match &marker.operation {
            WriteOperation::Create {
                kind,
                expected_number,
                corrected_number,
            } => {
                if !found.is_ours(&marker.order, *kind)
                    || expected_number.as_deref() == Some(found.number.as_str())
                    || corrected_number
                        .as_ref()
                        .is_some_and(|base| found.referenced_invoice_number.as_ref() != Some(base))
                {
                    return Ok(WriteResult::Unresolved);
                }
                WriteResult::Create(if found.is_live() {
                    CreateOutcome::Reconciled(found)
                } else {
                    CreateOutcome::Reversed(found)
                })
            }
            WriteOperation::Storno { number } => {
                if !found.is_storno_of(number) || !found.carries_order(&marker.order) {
                    return Ok(WriteResult::Unresolved);
                }
                match self.verify(number).await? {
                    QueryOutcome::Found(original)
                        if original.number == *number
                            && original.carries_order(&marker.order)
                            && original.reversed == Some(true) =>
                    {
                        WriteResult::Storno(StornoOutcome::AlreadyReversed {
                            storno_number: found.number,
                        })
                    }
                    QueryOutcome::CredentialsRejected(answer) => WriteResult::Answered {
                        credentials: true,
                        answer,
                    },
                    QueryOutcome::Api(answer) => WriteResult::Answered {
                        credentials: false,
                        answer,
                    },
                    _ => WriteResult::Unresolved,
                }
            }
            // A query cannot distinguish a deletion from consumption or hiding.
            WriteOperation::Delete { .. } => WriteResult::Unresolved,
        })
    }
}
