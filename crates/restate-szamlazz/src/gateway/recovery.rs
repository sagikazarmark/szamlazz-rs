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
    Unresolved(WriteDiagnostic),
}

/// Minimal, safe evidence retained with uncertainty. No upstream response body
/// or vendor free-text message is copied into recovery diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WriteDiagnostic {
    pub(crate) reason: String,
    pub(crate) candidate_number: Option<String>,
}

impl WriteDiagnostic {
    pub(crate) fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            candidate_number: None,
        }
    }

    fn unconfirmed(cause: super::Unconfirmed) -> Self {
        match cause {
            super::Unconfirmed::Open { code, .. } => Self::new(match code {
                Some(code) => format!("open vendor code {code}"),
                None => "success without a document number".into(),
            }),
            super::Unconfirmed::StornoVerification { number, .. } => Self {
                reason: "numbered storno reply needs positive reversal evidence".into(),
                candidate_number: Some(number),
            },
            super::Unconfirmed::Transport(message) | super::Unconfirmed::Unavailable(message) => {
                Self::new(message)
            }
            super::Unconfirmed::ReQueryFailed { .. } => Self::new("post-send query failed"),
        }
    }
}

impl WriteResult {
    pub(crate) fn unresolved(reason: impl Into<String>) -> Self {
        Self::Unresolved(WriteDiagnostic::new(reason))
    }

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
            DeleteOutcome::Lost(cause) => Self::unresolved(cause.to_string()),
            DeleteOutcome::Inconclusive(answer) => {
                Self::unresolved(format!("inconclusive deletion code {}", answer.code))
            }
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
            Err(super::QueryError::Transport(message)) => return WriteResult::unresolved(message),
        }
        match self.create_send(&request, true).await {
            Ok(outcome) => WriteResult::Create(outcome),
            // Uncertain sends go through the marker's stronger identity checks.
            Err(cause) => WriteResult::Unresolved(WriteDiagnostic::unconfirmed(cause)),
        }
    }

    pub(crate) async fn protected_storno(
        &self,
        request: super::StornoStepRequest<'_>,
        marker: &UnresolvedWrite,
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
            Err(super::QueryError::Transport(message)) => return WriteResult::unresolved(message),
        }
        match self.storno_send(request, Some(marker)).await {
            Ok(
                outcome @ (StornoOutcome::Reversed(_)
                | StornoOutcome::Rejected(_)
                | StornoOutcome::NotStornoable
                | StornoOutcome::CredentialsRejected(_)),
            ) => WriteResult::Storno(outcome),
            Err(cause) => WriteResult::Unresolved(WriteDiagnostic::unconfirmed(cause)),
            Ok(_) => WriteResult::unresolved("storno requires positive reversal evidence"),
        }
    }

    /// Query positive evidence for the exact marker, never sending a mutation.
    /// Absence and every inconclusive answer preserve uncertainty.
    pub(crate) async fn reconcile_write(
        &self,
        marker: &UnresolvedWrite,
        candidate: Option<&str>,
    ) -> WriteResult {
        let mut checked = self.reconcile_write_checked(marker, candidate).await;
        // A reply's candidate is a hint, not a restriction on automatic recovery.
        // Operator document evidence calls the checked method directly and remains
        // strict about the number the operator submitted.
        if candidate.is_some()
            && matches!(marker.operation, WriteOperation::Storno { .. })
            && !matches!(&checked, Ok(WriteResult::Storno(_)))
        {
            checked = self.reconcile_write_checked(marker, None).await;
        }
        match checked {
            Ok(WriteResult::Answered {
                credentials,
                answer,
            }) => WriteResult::unresolved(format!(
                "{} code {}",
                if credentials {
                    "credentials rejected"
                } else {
                    "inconclusive vendor"
                },
                answer.code
            )),
            Err(cause) => WriteResult::unresolved(cause.to_string()),
            Ok(result) => result,
        }
    }

    pub(crate) async fn reconcile_write_checked(
        &self,
        marker: &UnresolvedWrite,
        candidate: Option<&str>,
    ) -> Result<WriteResult, super::Unanswered> {
        // Storno is idempotent by original number; a repeat does not attach our
        // external id. A supplied candidate is therefore verified by number.
        // Without a candidate, the order hint can name a matching reversal.
        let queried = if matches!(marker.operation, WriteOperation::Storno { .. }) {
            if let Some(number) = candidate {
                self.verify(number).await?
            } else {
                match self
                    .query(&Selector::ExternalId(marker.external_id.clone()))
                    .await?
                {
                    QueryOutcome::NotFound => self.hint(&marker.order).await?,
                    outcome => outcome,
                }
            }
        } else {
            self.query(&Selector::ExternalId(marker.external_id.clone()))
                .await?
        };
        let found = match queried {
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
            QueryOutcome::NotFound => {
                return Ok(WriteResult::unresolved(
                    "document absent; absence does not settle the write",
                ));
            }
        };
        if candidate.is_some_and(|number| found.number != number) {
            return Ok(WriteResult::unresolved(
                "candidate number does not match the queried document",
            ));
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
                    return Ok(WriteResult::unresolved(
                        "document does not match order, kind or expected issuance intent",
                    ));
                }
                WriteResult::Create(if found.is_live() {
                    CreateOutcome::Reconciled(found)
                } else {
                    CreateOutcome::Reversed(found)
                })
            }
            WriteOperation::Storno { number } => {
                if !found.is_storno_of(number) || !found.carries_order(&marker.order) {
                    return Ok(WriteResult::unresolved(
                        "candidate is not the original's storno of this order",
                    ));
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
                    _ => WriteResult::unresolved(
                        "original reversal and order identity are not established",
                    ),
                }
            }
            // A query cannot distinguish a deletion from consumption or hiding.
            WriteOperation::Delete { .. } => WriteResult::unresolved(
                "deletion requires independent settlement; document queries cannot establish completion",
            ),
        })
    }
}
