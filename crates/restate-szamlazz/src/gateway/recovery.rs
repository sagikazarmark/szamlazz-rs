//! Intent-aware, read-only evidence for caller-retained writes.
//!
//! The same evidence rules serve expert Gateway orchestrators and protected Order
//! recovery. These reads do not admit, arm, retry or settle a write durably.

use serde::{Deserialize, Serialize};

use super::{
    CreateOutcome, DeleteOutcome, FoundDocument, Gateway, QueryError, QueryOutcome,
    StornoLookupOutcome, StornoOutcome, Unanswered,
};
use crate::contract::recovery::UnresolvedWrite;
pub use crate::contract::recovery::WriteOperation;
use crate::identity::{ExternalId, OrderKey};

/// The exact retained intent to reconcile on this Gateway's account.
///
/// The caller must retain these values before sending and keep the account mapping
/// stable. An external id is not unique at szamlazz.hu: ownership and operation
/// intent are checked together. No Restate invocation or marker is required.
#[derive(Debug, Clone, Copy)]
pub struct ReconciliationRequest<'a> {
    /// External id of the potentially effective write.
    pub external_id: &'a str,
    /// Order whose document was to be issued or reversed.
    pub order: &'a OrderKey,
    /// Create kind, expected old holder and corrective base, or exact mutation target.
    /// A corrective must carry its base; omitting it cannot establish completion.
    pub operation: &'a WriteOperation,
    /// Optional exact candidate number. For creates it must match the external-id
    /// holder; for storno it is queried by number. A mismatch is inconclusive,
    /// with no fallback to a different number. Vendor numbers are not mutation
    /// inputs: their original spelling and length are preserved.
    pub candidate: Option<&'a str>,
}

/// What a read establishes about the retained intent. Only `Created` and
/// `Reversed` are positive completion evidence; every other variant retains
/// uncertainty, including an answered credential failure. None grants permission
/// for a new write. A newer, reversed create can still establish that issuance
/// occurred; inspect its `reversed` field before making a new business decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ReconciliationOutcome {
    /// Matching issuance, possibly reversed since it was issued.
    Created(Box<FoundDocument>),
    /// Matching storno and a freshly queried, reversed original of this order.
    Reversed {
        /// The verified reversal document number.
        storno_number: String,
        /// Provider record ID of the verified reversal, absent in older evidence.
        #[serde(default)]
        storno_document_id: Option<i64>,
    },
    /// No sufficient evidence. Absence cannot prove non-execution; queries also
    /// cannot establish deletion rather than consumption or a hidden holder.
    Inconclusive {
        /// Safe explanation, without upstream body or vendor free text.
        reason: String,
    },
    /// Credentials prevented verification; the earlier write remains unresolved.
    CredentialsRejected(super::SzamlazzAnswer),
    /// Another vendor answer prevented verification.
    Api(super::SzamlazzAnswer),
}

impl ReconciliationOutcome {
    fn inconclusive(reason: impl Into<String>) -> Self {
        Self::Inconclusive {
            reason: reason.into(),
        }
    }
}

enum StornoEvidence {
    Verified {
        storno_number: String,
        storno_document_id: Option<i64>,
    },
    Inconclusive(&'static str),
}

/// The corrective base is part of issuance intent, beyond external-id ownership.
pub(super) fn matches_corrective_base(found: &FoundDocument, operation: &WriteOperation) -> bool {
    match operation {
        WriteOperation::Create {
            kind,
            corrected_number,
            ..
        } => matches_create_base(found, *kind, corrected_number.as_deref()),
        _ => true,
    }
}

/// Issuance-base identity, shared by the pre-send lookup and positive evidence.
/// Callers classify a mismatch according to whether a send can have happened.
pub(crate) fn matches_create_base(
    found: &FoundDocument,
    kind: crate::identity::IssuedKind,
    corrected_number: Option<&str>,
) -> bool {
    match (kind, corrected_number) {
        (crate::identity::IssuedKind::Corrective, Some(base)) => found.is_corrective_of(base),
        (crate::identity::IssuedKind::Corrective, None) | (_, Some(_)) => false,
        (_, None) => true,
    }
}

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
    /// A reported warning belongs only to `candidate_number`.
    #[serde(default)]
    pub(crate) notification_delivery_failed: bool,
}

impl WriteDiagnostic {
    pub(crate) fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            candidate_number: None,
            notification_delivery_failed: false,
        }
    }

    fn unconfirmed(cause: super::Unconfirmed) -> Self {
        match cause {
            super::Unconfirmed::ReissueEcho => {
                Self::new(super::Unconfirmed::ReissueEcho.to_string())
            }
            super::Unconfirmed::Open { code, .. } => Self::new(match code {
                Some(code) => format!("open vendor code {code}"),
                None => "success without a document number".into(),
            }),
            super::Unconfirmed::StornoVerification {
                number,
                notification_delivery_failed,
                ..
            } => Self {
                reason: "numbered storno reply needs positive reversal evidence".into(),
                candidate_number: Some(number),
                notification_delivery_failed,
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
    /// Replay-enabled deletion refreshes the pinned target before resubmission.
    /// Only acknowledged deletion settles; an absent target is still uncertainty.
    pub(crate) async fn delete_replay_enabled(
        &self,
        external_id: &ExternalId,
        marker: &UnresolvedWrite,
        found: &FoundDocument,
        request: &crate::contract::DeleteProformaRequest,
    ) -> WriteResult {
        use super::OwnershipOutcome;
        use crate::contract::DeleteMode;
        use crate::contract::recovery::ReplayExecutionContract as Contract;

        let contract = Contract::for_delete(request, found.document_id);
        if marker.execution_contract != Some(contract)
            || marker.operation
                != (WriteOperation::Delete {
                    number: found.number.clone(),
                })
        {
            return WriteResult::unresolved("deletion request differs from retained intent");
        }
        let order = &marker.order;
        if request.mode == DeleteMode::NamespaceOwned {
            match self
                .lookup_ours(external_id, order, crate::identity::IssuedKind::Proforma)
                .await
            {
                Ok(OwnershipOutcome::Live(current) | OwnershipOutcome::Reversed(current))
                    if current.number == found.number
                        && current.document_id == found.document_id => {}
                Ok(OwnershipOutcome::CredentialsRejected(answer)) => {
                    answer.warn_credentials_rejected(external_id.namespace());
                    return WriteResult::unresolved(format!(
                        "deletion namespace credentials rejected: {}",
                        answer.code
                    ));
                }
                Ok(OwnershipOutcome::Absent) => {
                    return WriteResult::unresolved(
                        "deletion namespace holder absent; absence does not settle deletion",
                    );
                }
                Ok(OwnershipOutcome::Api(answer)) => {
                    return WriteResult::unresolved(format!(
                        "deletion namespace vendor code {}",
                        answer.code
                    ));
                }
                Err(cause) => {
                    return WriteResult::unresolved(format!(
                        "deletion namespace query unanswered: {cause}"
                    ));
                }
                _ => {
                    return WriteResult::unresolved(
                        "deletion namespace target changed or collided",
                    );
                }
            }
        }
        match self.delete_proforma(found, order, request.force).await {
            DeleteOutcome::Deleted => WriteResult::Delete(DeleteOutcome::Deleted),
            DeleteOutcome::CredentialsRejected(answer) => {
                answer.warn_credentials_rejected(external_id.namespace());
                WriteResult::unresolved(format!("deletion credentials rejected: {}", answer.code))
            }
            DeleteOutcome::AlreadyGone => WriteResult::unresolved(
                "proforma reported absent; absence does not settle earlier deletion",
            ),
            DeleteOutcome::Paid => WriteResult::unresolved(
                "fresh deletion paid guard refused; earlier deletion remains unresolved",
            ),
            DeleteOutcome::TargetChanged => WriteResult::unresolved(
                "fresh deletion target changed; earlier deletion remains unresolved",
            ),
            DeleteOutcome::Api(answer) => {
                WriteResult::unresolved(format!("deletion guard vendor code {}", answer.code))
            }
            DeleteOutcome::Rejected(rejection) => WriteResult::unresolved(format!(
                "deletion refused: {}; earlier execution remains unresolved",
                rejection.code
            )),
            DeleteOutcome::Inconclusive(answer) => {
                WriteResult::unresolved(format!("inconclusive deletion code {}", answer.code))
            }
            DeleteOutcome::GuardFailed(cause) => {
                WriteResult::unresolved(format!("deletion guard unanswered: {cause}"))
            }
            DeleteOutcome::Lost(cause) => {
                WriteResult::unresolved(format!("deletion answer lost: {cause}"))
            }
        }
    }

    /// Replay-enabled Order storno: settle existing evidence before inspecting
    /// fresh send guards; only the pinned live original can authorize replay.
    pub(crate) async fn storno_replay_enabled(
        &self,
        request: super::StornoStepRequest<'_>,
        marker: &UnresolvedWrite,
        pinned: &FoundDocument,
    ) -> WriteResult {
        use crate::contract::recovery::ReplayExecutionContract as Contract;
        let expected = Contract::RequestResponseStornoV1 {
            document_id: pinned.document_id,
            fulfillment_date: request.fulfillment_date,
            e_invoice: request.e_invoice,
            appearance: pinned.appearance,
        };
        if marker.execution_contract != Some(expected)
            || marker.operation
                != (WriteOperation::Storno {
                    number: request.invoice_number.to_owned(),
                })
            || marker.external_id != request.external_id.as_str()
        {
            return WriteResult::unresolved("storno request differs from retained original intent");
        }
        match self
            .lookup_order_storno_checked(request.external_id, &marker.order, request.invoice_number)
            .await
        {
            Ok(StornoLookupOutcome::AlreadyReversed { .. }) => {
                return self.reconcile_write(marker, None).await;
            }
            Ok(StornoLookupOutcome::Absent) => {}
            Err(QueryError::CredentialsRejected(answer))
            | Ok(StornoLookupOutcome::CredentialsRejected(answer)) => {
                answer.warn_credentials_rejected(request.external_id.namespace());
                return WriteResult::unresolved(format!(
                    "storno lookup credentials rejected: {}",
                    answer.code
                ));
            }
            Err(QueryError::Api(answer) | QueryError::Transient(answer))
            | Ok(StornoLookupOutcome::Api(answer)) => {
                return WriteResult::unresolved(format!(
                    "storno lookup vendor code {}",
                    answer.code
                ));
            }
            Err(cause) => {
                return WriteResult::unresolved(format!("storno lookup inconclusive: {cause}"));
            }
        }
        let fresh = match self.verify(request.invoice_number).await {
            Ok(QueryOutcome::Found(found)) => found,
            Ok(QueryOutcome::CredentialsRejected(answer)) => {
                answer.warn_credentials_rejected(request.external_id.namespace());
                return WriteResult::unresolved(format!(
                    "original credentials rejected: {}",
                    answer.code
                ));
            }
            Ok(QueryOutcome::Api(answer)) => {
                return WriteResult::unresolved(format!(
                    "original query vendor code {}",
                    answer.code
                ));
            }
            Ok(QueryOutcome::NotFound) => {
                return WriteResult::unresolved("original absent; reversal not established");
            }
            Err(cause) => {
                return WriteResult::unresolved(format!("original query unanswered: {cause}"));
            }
        };
        if fresh.document_id != pinned.document_id
            || !fresh.carries_order(&marker.order)
            || !fresh.is_stornoable()
            || fresh.fulfillment_date != Some(request.fulfillment_date)
            || fresh.appearance != pinned.appearance
        {
            return WriteResult::unresolved("fresh original differs from pinned storno intent");
        }
        if !fresh.is_live() {
            return self.reconcile_write(marker, None).await;
        }
        match self.storno_send(request, Some(marker)).await {
            Ok(outcome @ (StornoOutcome::Reversed(_) | StornoOutcome::AlreadyReversed { .. })) => {
                WriteResult::Storno(outcome)
            }
            Ok(StornoOutcome::CredentialsRejected(answer)) => {
                answer.warn_credentials_rejected(request.external_id.namespace());
                WriteResult::unresolved(format!("storno credentials rejected: {}", answer.code))
            }
            Ok(StornoOutcome::Rejected(rejection)) => WriteResult::unresolved(format!(
                "storno refused: {}; earlier execution remains unresolved",
                rejection.code
            )),
            Ok(_) => WriteResult::unresolved("storno answer did not establish reversal"),
            Err(cause) => WriteResult::Unresolved(WriteDiagnostic::unconfirmed(cause)),
        }
    }
    /// Replay-enabled creation: open-run replay may submit again after fresh absence and
    /// guards. Kept distinct from the public consumed-permission contract.
    pub(crate) async fn create_replay_enabled<F, Fut>(
        &self,
        request: super::CreateStepRequest<'_>,
        before_send: F,
    ) -> WriteResult
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = ()> + Send,
    {
        // Defence in depth: this seam is never a blanket permission for kinds.
        if request.replay_contract().is_none() || request.corrected_number.is_some() {
            return WriteResult::unresolved("unsupported replay-enabled issuance intent");
        }
        // After admission a different matching holder is replacement evidence,
        // not a new target to send past. The expected old holder alone can permit
        // a resend, and only while it remains reversed.
        match self
            .seen(request.external_id, request.order, request.kind)
            .await
        {
            Ok(super::Seen::Absent) if request.reversed.is_none() => {}
            Ok(super::Seen::Reversed(found)) if request.reversed == Some(found.number.as_str()) => {
            }
            Ok(super::Seen::Live(found)) if request.reversed != Some(found.number.as_str()) => {
                return WriteResult::Create(CreateOutcome::Found(found));
            }
            Ok(super::Seen::Reversed(found)) => {
                return WriteResult::Create(CreateOutcome::Reversed(found));
            }
            Ok(_) => {
                return WriteResult::unresolved(
                    "holder does not permit the retained issuance intent",
                );
            }
            Err(QueryError::CredentialsRejected(answer)) => {
                answer.warn_credentials_rejected(request.external_id.namespace());
                return WriteResult::unresolved(format!(
                    "holder credentials rejected: {}",
                    answer.code
                ));
            }
            Err(_) => return WriteResult::unresolved("holder query failed"),
        }
        // Target evidence settles first, even if a prerequisite has since changed.
        // Keep these reads sequential: namespace guards, pinned references, then hint.
        if let Err(unresolved) = self.check_replay_create_kinds(&request).await {
            return WriteResult::Unresolved(unresolved);
        }
        if let Err(unresolved) = self.check_replay_create_references(&request).await {
            return WriteResult::Unresolved(unresolved);
        }
        if let Err(unresolved) = self.check_replay_create_hint(&request).await {
            return WriteResult::Unresolved(unresolved);
        }
        before_send().await;
        replay_create_result(
            self.create_send(
                &request,
                super::CreateSettlement::RetainedWithDuplicateEvidence,
            )
            .await,
            request.external_id.namespace(),
        )
    }

    async fn check_replay_create_kinds(
        &self,
        request: &super::CreateStepRequest<'_>,
    ) -> Result<(), WriteDiagnostic> {
        use crate::identity::{DocumentKind, IssuedKind};
        let namespace = request
            .external_id
            .namespace()
            .parse()
            .map_err(|_| WriteDiagnostic::new("invalid issuance namespace"))?;
        let proforma = request.proforma_number();
        let prepayment = request.prepayment_number();
        if request.kind == IssuedKind::Final && prepayment.is_none() {
            return Err(WriteDiagnostic::new(
                "final issuance requires pinned prepayment",
            ));
        }
        let guarded = if request.kind == IssuedKind::Proforma {
            [
                DocumentKind::Invoice,
                DocumentKind::Prepayment,
                DocumentKind::Final,
            ]
        } else if request.kind == IssuedKind::Final {
            [
                DocumentKind::Invoice,
                DocumentKind::Prepayment,
                DocumentKind::Proforma,
            ]
        } else if request.kind == IssuedKind::Prepayment {
            [
                DocumentKind::Invoice,
                DocumentKind::Final,
                DocumentKind::Proforma,
            ]
        } else {
            [
                DocumentKind::Prepayment,
                DocumentKind::Final,
                DocumentKind::Proforma,
            ]
        };
        for kind in guarded {
            let id = ExternalId::for_kind(&namespace, request.order, kind);
            match self.lookup_ours(&id, request.order, kind.into()).await {
                Ok(super::OwnershipOutcome::Live(found))
                    if kind == DocumentKind::Prepayment
                        && request.kind == IssuedKind::Final
                        && prepayment == Some(found.number.as_str()) => {}
                Ok(super::OwnershipOutcome::Absent | super::OwnershipOutcome::Reversed(_))
                    if kind == DocumentKind::Prepayment && request.kind == IssuedKind::Final =>
                {
                    return Err(WriteDiagnostic::new("pinned prepayment absent or reversed"));
                }
                Ok(super::OwnershipOutcome::Absent | super::OwnershipOutcome::Reversed(_)) => {}
                Ok(super::OwnershipOutcome::Live(found))
                    if kind == DocumentKind::Proforma
                        && proforma == Some(found.number.as_str()) => {}
                Ok(super::OwnershipOutcome::CredentialsRejected(answer)) => {
                    answer.warn_credentials_rejected(request.external_id.namespace());
                    return Err(WriteDiagnostic::new(format!(
                        "fresh {kind} credentials rejected: {}",
                        answer.code
                    )));
                }
                _ => {
                    return Err(WriteDiagnostic::new(format!(
                        "fresh {kind} guard did not permit issuance"
                    )));
                }
            }
        }
        Ok(())
    }

    async fn check_replay_create_references(
        &self,
        request: &super::CreateStepRequest<'_>,
    ) -> Result<(), WriteDiagnostic> {
        use crate::identity::IssuedKind;
        if let Some(number) = request.prepayment_number() {
            match self.verify(number).await {
                Ok(super::QueryOutcome::Found(found))
                    if found.is_ours(request.order, IssuedKind::Prepayment) && found.is_live() => {}
                Ok(super::QueryOutcome::CredentialsRejected(answer)) => {
                    answer.warn_credentials_rejected(request.external_id.namespace());
                    return Err(WriteDiagnostic::new(format!(
                        "prepayment credentials rejected: {}",
                        answer.code
                    )));
                }
                _ => {
                    return Err(WriteDiagnostic::new(
                        "pinned prepayment no longer live on this Order",
                    ));
                }
            }
        }
        if let Some(number) = request.proforma_number() {
            match self.verify(number).await {
                Ok(super::QueryOutcome::Found(found))
                    if found.carries_order(request.order)
                        && found.document_type == szamlazz_agent::DocumentType::Proforma
                        && found.is_live() => {}
                Ok(super::QueryOutcome::CredentialsRejected(answer)) => {
                    answer.warn_credentials_rejected(request.external_id.namespace());
                    return Err(WriteDiagnostic::new(format!(
                        "pinned proforma credentials rejected: {}",
                        answer.code
                    )));
                }
                _ => {
                    return Err(WriteDiagnostic::new(
                        "pinned proforma no longer live on this Order",
                    ));
                }
            }
        }
        Ok(())
    }

    async fn check_replay_create_hint(
        &self,
        request: &super::CreateStepRequest<'_>,
    ) -> Result<(), WriteDiagnostic> {
        use crate::identity::IssuedKind;
        let prepayment = request.prepayment_number();
        let proforma = request.proforma_number();
        // Unlike the ordinary best-effort lookup hint, a failed fresh hint must
        // not authorize an unfinished-run resend. Proformas can be implicitly
        // consumed even when they are not under our namespace's external id.
        match self.hint_raw(request.order).await {
            Err(QueryError::NotFound) => {}
            Err(QueryError::CredentialsRejected(answer)) => {
                answer.warn_credentials_rejected(request.external_id.namespace());
                return Err(WriteDiagnostic::new(format!(
                    "fresh order hint credentials rejected: {}",
                    answer.code
                )));
            }
            Ok(found)
                if (prepayment == Some(found.number.as_str())
                    && found.is_ours(request.order, IssuedKind::Prepayment))
                    || (found.document_type == szamlazz_agent::DocumentType::Proforma
                        && proforma == Some(found.number.as_str())
                        && found.carries_order(request.order))
                    || !found.is_live()
                    || (!found.is_invoice_family()
                        && found.document_type != szamlazz_agent::DocumentType::Proforma) => {}
            _ => {
                return Err(WriteDiagnostic::new(
                    "fresh order hint did not permit issuance",
                ));
            }
        }
        Ok(())
    }

    /// The protected Order lookup: only absence permits a send. A holder that
    /// cannot establish this order's reversal is an unanswered evidence read.
    /// The fresh original check stays inside the same durable lookup operation.
    pub(crate) async fn lookup_order_storno(
        &self,
        external_id: &ExternalId,
        order: &OrderKey,
        number: &str,
    ) -> Result<StornoLookupOutcome, Unanswered> {
        match self
            .lookup_order_storno_checked(external_id, order, number)
            .await
        {
            Ok(outcome) => Ok(outcome),
            Err(error) => match error.answered()? {
                super::Answer::CredentialsRejected(answer) => {
                    Ok(StornoLookupOutcome::CredentialsRejected(answer))
                }
                super::Answer::Api(answer) => Ok(StornoLookupOutcome::Api(answer)),
                super::Answer::NotFound => Err(Unanswered::Transport(
                    "reversal evidence absent; reversal is not established".into(),
                )),
            },
        }
    }

    // Keep vendor-code provenance until the caller selects read retry or
    // leading-write settlement. In particular, a transient code is no header.
    async fn lookup_order_storno_checked(
        &self,
        external_id: &ExternalId,
        order: &OrderKey,
        number: &str,
    ) -> Result<StornoLookupOutcome, QueryError> {
        let result = match self
            .query_raw(szamlazz_agent::InvoiceSelector::ExternalId(
                external_id.as_str().to_owned(),
            ))
            .await
        {
            Ok(found) => self.verify_order_storno(&found, order, number).await,
            Err(QueryError::NotFound) => return Ok(StornoLookupOutcome::Absent),
            Err(error) => Err(error),
        };
        match result {
            Ok(StornoEvidence::Verified {
                storno_number,
                storno_document_id,
            }) => Ok(StornoLookupOutcome::AlreadyReversed {
                storno_number,
                storno_document_id,
            }),
            Ok(StornoEvidence::Inconclusive(reason)) => Err(QueryError::Transport(reason.into())),
            Err(QueryError::NotFound) => Err(QueryError::Transport(
                "original absent; reversal is not established".into(),
            )),
            Err(error) => Err(error),
        }
    }

    /// One evidence rule for protected lookup, leading query and recovery.
    /// A candidate never establishes reversal without a fresh, matching original.
    async fn verify_order_storno(
        &self,
        found: &FoundDocument,
        order: &OrderKey,
        number: &str,
    ) -> Result<StornoEvidence, QueryError> {
        if !found.is_storno_of(number) || !found.carries_order(order) {
            return Ok(StornoEvidence::Inconclusive(
                "candidate is not the original's storno of this order",
            ));
        }
        let original = self
            .query_raw(szamlazz_agent::InvoiceSelector::InvoiceNumber(
                szamlazz_agent::InvoiceNumber::new(number),
            ))
            .await?;
        if !original.is_stornoable()
            || !original.carries_order(order)
            || original.reversed != Some(true)
        {
            return Ok(StornoEvidence::Inconclusive(
                "original reversal and order identity are not established",
            ));
        }
        Ok(StornoEvidence::Verified {
            storno_number: found.number.clone(),
            storno_document_id: Some(found.document_id),
        })
    }

    pub(crate) async fn protected_create(
        &self,
        request: super::CreateStepRequest<'_>,
    ) -> WriteResult {
        match self.settled_by_query(&request, true).await {
            Ok(Some(outcome)) => return WriteResult::Create(outcome),
            Ok(None) | Err(super::QueryError::NotFound) => {}
            Err(super::QueryError::Api(answer) | super::QueryError::Transient(answer)) => {
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
        match self
            .create_send(&request, super::CreateSettlement::Retained)
            .await
        {
            Ok(outcome) => WriteResult::Create(outcome),
            // Uncertain sends continue through the shared read-only evidence seam.
            Err(cause) => WriteResult::Unresolved(WriteDiagnostic::unconfirmed(cause)),
        }
    }

    pub(crate) async fn protected_storno(
        &self,
        request: super::StornoStepRequest<'_>,
        marker: &UnresolvedWrite,
    ) -> WriteResult {
        match self
            .lookup_order_storno_checked(request.external_id, &marker.order, request.invoice_number)
            .await
        {
            Ok(StornoLookupOutcome::AlreadyReversed {
                storno_number,
                storno_document_id,
            }) => {
                return WriteResult::Storno(StornoOutcome::AlreadyReversed {
                    storno_number,
                    storno_document_id,
                });
            }
            Ok(StornoLookupOutcome::Absent) => {}
            Ok(StornoLookupOutcome::Api(answer))
            | Err(QueryError::Api(answer) | QueryError::Transient(answer)) => {
                return WriteResult::Storno(StornoOutcome::Api(answer));
            }
            Ok(StornoLookupOutcome::CredentialsRejected(answer))
            | Err(QueryError::CredentialsRejected(answer)) => {
                return WriteResult::Storno(StornoOutcome::CredentialsRejected(answer));
            }
            Err(QueryError::Unavailable(message)) => {
                return WriteResult::Storno(StornoOutcome::Unavailable { message });
            }
            Err(cause) => return WriteResult::unresolved(cause.to_string()),
        }
        match self.storno_send(request, Some(marker)).await {
            Ok(
                outcome @ (StornoOutcome::Reversed(_)
                | StornoOutcome::Rejected(_)
                | StornoOutcome::CredentialsRejected(_)),
            ) => WriteResult::Storno(outcome),
            Err(cause) => WriteResult::Unresolved(WriteDiagnostic::unconfirmed(cause)),
            Ok(_) => WriteResult::unresolved("storno requires positive reversal evidence"),
        }
    }

    /// Reconcile a journaled uncertain write, retaining an acknowledged warning
    /// only when the evidence establishes that exact reversal candidate.
    pub(crate) async fn reconcile_write_diagnostic(
        &self,
        marker: &UnresolvedWrite,
        diagnostic: &WriteDiagnostic,
    ) -> WriteResult {
        let result = self
            .reconcile_write(marker, diagnostic.candidate_number.as_deref())
            .await;
        if diagnostic.notification_delivery_failed
            && let WriteResult::Storno(StornoOutcome::AlreadyReversed {
                storno_number,
                storno_document_id,
            }) = &result
            && diagnostic.candidate_number.as_deref() == Some(storno_number.as_str())
        {
            return WriteResult::Storno(StornoOutcome::Reversed(super::IssuedDocument {
                number: storno_number.clone(),
                document_id: *storno_document_id,
                net_total: None,
                gross_total: None,
                outstanding: None,
                customer_account_url: None,
                notification_delivery_failed: true,
            }));
        }
        result
    }

    /// Query positive evidence for the exact marker, never sending a mutation.
    /// Absence and every inconclusive answer preserve uncertainty.
    pub(crate) async fn reconcile_write(
        &self,
        marker: &UnresolvedWrite,
        candidate: Option<&str>,
    ) -> WriteResult {
        let mut checked = self.reconcile_write_checked(marker, candidate).await;
        warn_reconciliation_credentials(&checked, marker);
        // A reply's candidate is a hint, not a restriction on automatic recovery.
        // Operator document evidence calls the checked method directly and remains
        // strict about the number the operator submitted.
        if candidate.is_some()
            && matches!(marker.operation, WriteOperation::Storno { .. })
            && !matches!(&checked, Ok(WriteResult::Storno(_)))
        {
            checked = self.reconcile_write_checked(marker, None).await;
            warn_reconciliation_credentials(&checked, marker);
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
        Ok(
            match self
                .reconcile(ReconciliationRequest {
                    external_id: &marker.external_id,
                    order: &marker.order,
                    operation: &marker.operation,
                    candidate,
                })
                .await?
            {
                ReconciliationOutcome::Created(found) => WriteResult::Create(if found.is_live() {
                    CreateOutcome::Reconciled(found)
                } else {
                    CreateOutcome::Reversed(found)
                }),
                ReconciliationOutcome::Reversed {
                    storno_number,
                    storno_document_id,
                } => {
                    if let Some(crate::contract::recovery::ReplayExecutionContract::RequestResponseStornoV1 {document_id,fulfillment_date,appearance,..})=marker.execution_contract
                        && let WriteOperation::Storno {number}=&marker.operation {
                        match self.verify(number).await? {
                            QueryOutcome::Found(original) if original.document_id==document_id && original.fulfillment_date==Some(fulfillment_date) && original.appearance==appearance && original.carries_order(&marker.order) && original.is_stornoable() && original.reversed==Some(true)=>{},
                            QueryOutcome::CredentialsRejected(answer)=>return Ok(WriteResult::Answered {credentials:true,answer}),
                            QueryOutcome::Api(answer)=>return Ok(WriteResult::Answered {credentials:false,answer}),
                            _=>return Ok(WriteResult::unresolved("reversal evidence does not match pinned original")),
                        }
                    }
                    WriteResult::Storno(StornoOutcome::AlreadyReversed {
                        storno_number,
                        storno_document_id,
                    })
                }
                ReconciliationOutcome::Inconclusive { reason } => WriteResult::unresolved(reason),
                ReconciliationOutcome::CredentialsRejected(answer) => WriteResult::Answered {
                    credentials: true,
                    answer,
                },
                ReconciliationOutcome::Api(answer) => WriteResult::Answered {
                    credentials: false,
                    answer,
                },
            },
        )
    }

    /// Query evidence for retained issuance or reversal intent, without sending
    /// any mutation. This is the evidence boundary used by protected Order too.
    ///
    /// Creates must match order, kind and corrective base and differ from the
    /// expected old reissue target. Storno requires both a matching reversal and
    /// a fresh, matching reversed original. Without a candidate, storno queries
    /// the external id then takes the order hint only if that id is absent.
    /// Deletion cannot be settled by these queries.
    ///
    /// The caller owns durable exclusion, account selection and recording the
    /// settlement. Keep uncertainty on every result except positive evidence;
    /// empty reads, elapsed time and interruptions never authorize another send.
    ///
    /// # Errors
    ///
    /// [`Unanswered`] if a query produced no answer. Repeating this read is safe;
    /// its failure says nothing about the earlier write.
    pub async fn reconcile(
        &self,
        request: ReconciliationRequest<'_>,
    ) -> Result<ReconciliationOutcome, Unanswered> {
        let ReconciliationRequest {
            external_id,
            order,
            operation,
            candidate,
        } = request;
        if matches!(operation, WriteOperation::Delete { .. }) {
            return Ok(ReconciliationOutcome::inconclusive(
                "deletion requires independent settlement; document queries cannot establish completion",
            ));
        }
        // Storno is idempotent by original number; a repeat does not attach our
        // external id. A supplied candidate is therefore verified by number.
        // Without a candidate, the order hint can name a matching reversal.
        let queried = if matches!(operation, WriteOperation::Storno { .. }) {
            if let Some(number) = candidate {
                self.verify(number).await?
            } else {
                match super::outcome(
                    self.query_raw(szamlazz_agent::InvoiceSelector::ExternalId(
                        external_id.to_owned(),
                    ))
                    .await,
                )? {
                    QueryOutcome::NotFound => self.hint(order).await?,
                    outcome => outcome,
                }
            }
        } else {
            super::outcome(
                self.query_raw(szamlazz_agent::InvoiceSelector::ExternalId(
                    external_id.to_owned(),
                ))
                .await,
            )?
        };
        let found = match queried {
            QueryOutcome::Found(found) => found,
            QueryOutcome::CredentialsRejected(answer) => {
                return Ok(ReconciliationOutcome::CredentialsRejected(answer));
            }
            QueryOutcome::Api(answer) => {
                return Ok(ReconciliationOutcome::Api(answer));
            }
            QueryOutcome::NotFound => {
                return Ok(ReconciliationOutcome::inconclusive(
                    "document absent; absence does not settle the write",
                ));
            }
        };
        if candidate.is_some_and(|number| found.number != number) {
            return Ok(ReconciliationOutcome::inconclusive(
                "candidate number does not match the queried document",
            ));
        }
        Ok(match operation {
            WriteOperation::Create {
                kind,
                expected_number,
                ..
            } => {
                if !found.is_ours(order, *kind)
                    || expected_number.as_deref() == Some(found.number.as_str())
                    || !matches_corrective_base(&found, operation)
                {
                    return Ok(ReconciliationOutcome::inconclusive(
                        "document does not match order, kind or expected issuance intent",
                    ));
                }
                ReconciliationOutcome::Created(found)
            }
            WriteOperation::Storno { number } => {
                match self.verify_order_storno(&found, order, number).await {
                    Ok(StornoEvidence::Verified {
                        storno_number,
                        storno_document_id,
                    }) => ReconciliationOutcome::Reversed {
                        storno_number,
                        storno_document_id,
                    },
                    Ok(StornoEvidence::Inconclusive(reason)) => {
                        ReconciliationOutcome::inconclusive(reason)
                    }
                    Err(error) => match error.answered()? {
                        super::Answer::CredentialsRejected(answer) => {
                            ReconciliationOutcome::CredentialsRejected(answer)
                        }
                        super::Answer::Api(answer) => ReconciliationOutcome::Api(answer),
                        super::Answer::NotFound => ReconciliationOutcome::inconclusive(
                            "original absent; reversal is not established",
                        ),
                    },
                }
            }
            // A query cannot distinguish a deletion from consumption or hiding.
            WriteOperation::Delete { .. } => ReconciliationOutcome::inconclusive(
                "deletion requires independent settlement; document queries cannot establish completion",
            ),
        })
    }
}

/// Positive issuance alone clears under accepted create replay risk. A later
/// refusal/collision says nothing about an interrupted earlier execution.
fn replay_create_result(
    result: Result<CreateOutcome, super::Unconfirmed>,
    namespace: &str,
) -> WriteResult {
    match result {
        Ok(
            outcome @ (CreateOutcome::Issued(_)
            | CreateOutcome::Found(_)
            | CreateOutcome::Reconciled(_)
            | CreateOutcome::Reversed(_)),
        ) => WriteResult::Create(outcome),
        Ok(CreateOutcome::CredentialsRejected(answer)) => {
            answer.warn_credentials_rejected(namespace);
            WriteResult::unresolved(format!("issuance credentials rejected: {}", answer.code))
        }
        Ok(CreateOutcome::Rejected(rejection)) => {
            WriteResult::unresolved(format!("issuance refusal: {}", rejection.code))
        }
        Ok(CreateOutcome::DuplicateOrderNumber { answer, .. }) => {
            WriteResult::unresolved(format!("issuance duplicate refusal: {}", answer.code))
        }
        Ok(_) => WriteResult::unresolved("guard or query did not establish issuance"),
        Err(cause) => WriteResult::Unresolved(WriteDiagnostic::unconfirmed(cause)),
    }
}

/// Alert before a fallback can replace the answer. This does not settle the
/// earlier write or change the retryable reconciliation result.
pub(super) fn warn_reconciliation_credentials(
    checked: &Result<WriteResult, Unanswered>,
    marker: &UnresolvedWrite,
) {
    if let Ok(WriteResult::Answered {
        credentials: true,
        answer,
    }) = checked
    {
        answer.warn_credentials_rejected(&marker.namespace);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn contradictory_reissue_keeps_its_cause_in_the_journal() {
        let result = super::WriteResult::Unresolved(super::WriteDiagnostic::unconfirmed(
            super::super::Unconfirmed::ReissueEcho,
        ));
        let journal = serde_json::to_string(&result).expect("journal");
        assert!(journal.contains("reissue acknowledgement names the old document"));
        assert!(!journal.contains("without a document number"));
    }

    use super::*;
    use crate::account::{Account, Endpoint};
    use crate::contract::recovery::MarkerVersion;
    use crate::identity::IssuedKind;
    use crate::test_support::{LogCapture, api_error, open_gateway};
    use szamlazz_agent::Credentials;
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::body_string_contains};

    fn query(element: &str, value: &str) -> wiremock::MockBuilder {
        Mock::given(body_string_contains("action-szamla_agent_xml")).and(body_string_contains(
            format!("<{element}>{value}</{element}>"),
        ))
    }

    #[tokio::test]
    async fn protected_storno_leading_check_preserves_transient_code_provenance() {
        for code in [Some("1"), Some("55"), None] {
            let server = MockServer::start().await;
            let mut account = Account::new("account", "reference");
            account.endpoint = Endpoint::parse(&server.uri()).expect("endpoint");
            let gateway = open_gateway(account, Credentials::agent_key("key"));
            let id = ExternalId::new("acct:ORD-1:storno:SZ-1");
            let marker = UnresolvedWrite {
                prepayment_number: None,
                execution_contract: None,
                proforma_number: None,
                version: MarkerVersion,
                token: "owner".into(),
                owner_invocation: "owner".into(),
                created_at: "2026-09-14T12:00:00Z".into(),
                scope: None,
                order: OrderKey::parse("ORD-1").expect("order"),
                namespace: "acct".parse().expect("namespace"),
                external_id: id.to_string(),
                account_id: "account".into(),
                endpoint: server.uri(),
                credential_ref: "reference".into(),
                operation: WriteOperation::Storno {
                    number: "SZ-1".into(),
                },
            };
            query("szamlaKulsoAzon", id.as_str())
                .respond_with(code.map_or_else(
                    || ResponseTemplate::new(503).insert_header("szlahu_down", "maintenance"),
                    |code| api_error(code, "vendor answer"),
                ))
                .expect(2)
                .mount(&server)
                .await;
            Mock::given(body_string_contains("action-szamla_agent_st"))
                .respond_with(ResponseTemplate::new(500))
                .expect(0)
                .mount(&server)
                .await;
            // The ordinary protected lookup still feeds its read retry policy.
            assert!(matches!(
                gateway
                    .lookup_order_storno(&id, &marker.order, "SZ-1")
                    .await,
                Err(Unanswered::Unavailable(_))
            ));
            // Once armed, the leading check records the vendor's actual answer
            // without sending; only the header can become Unavailable outcome.
            let result = gateway
                .protected_storno(
                    crate::gateway::StornoStepRequest {
                        invoice_number: "SZ-1",
                        external_id: &id,
                        comment: None,
                        buyer_email: None,
                        e_invoice: false,
                        fulfillment_date: jiff::civil::date(2026, 9, 14),
                    },
                    &marker,
                )
                .await;
            match (code, result) {
                (Some(code), WriteResult::Storno(StornoOutcome::Api(answer))) => {
                    assert_eq!(answer.code, code);
                    assert_eq!(answer.message, "vendor answer");
                }
                (None, WriteResult::Storno(StornoOutcome::Unavailable { message })) => {
                    assert_eq!(message, "query: szlahu_down");
                }
                (_, result) => panic!("unexpected leading check: {result:?}"),
            }
            assert_eq!(server.received_requests().await.expect("requests").len(), 2);
            server.verify().await;
        }
    }

    #[tokio::test]
    async fn protected_reissue_refuses_changed_live_reversed_and_absent_holders_before_send() {
        use crate::contract::{BuyerInput, DocumentInput, LineItemInput, PaymentMethod};
        use crate::gateway::{CreateStepRequest, DocumentRefs};
        use crate::test_support::Doc;
        for response in [
            Doc::new("SZ-NEW", "SZ").response(),
            Doc::reversed("SZ-NEW", "SZ").response(),
            api_error("7", "absent"),
        ] {
            let server = MockServer::start().await;
            let mut account = Account::new("account", "reference");
            account.endpoint = Endpoint::parse(&server.uri()).expect("endpoint");
            let gateway = open_gateway(account, Credentials::agent_key("key"));
            let external_id = ExternalId::new("acct:ORD-1:invoice");
            let order = OrderKey::parse("ORD-1").expect("order");
            query("szamlaKulsoAzon", external_id.as_str())
                .respond_with(response)
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(body_string_contains("action-xmlagentxmlfile"))
                .respond_with(ResponseTemplate::new(500))
                .expect(0)
                .mount(&server)
                .await;
            let input = DocumentInput::new(
                BuyerInput::new("Buyer", "1000", "City", "Street"),
                vec![LineItemInput::new(
                    "item",
                    rust_decimal::dec!(1),
                    "db",
                    rust_decimal::dec!(1000),
                    "27",
                )],
                jiff::civil::date(2026, 9, 14),
                jiff::civil::date(2026, 9, 14),
                PaymentMethod::Transfer,
            );
            let create = gateway
                .build_create(
                    IssuedKind::Invoice,
                    &input,
                    &order,
                    &external_id,
                    DocumentRefs::default(),
                )
                .expect("create");
            let request = CreateStepRequest::new(
                &external_id,
                IssuedKind::Invoice,
                &order,
                &create,
                Some("SZ-OLD"),
                None,
            )
            .expect("intent");
            let result = gateway.protected_create(request).await;
            assert!(
                matches!(result, WriteResult::Create(CreateOutcome::TargetChanged)),
                "{result:?}"
            );
            assert_eq!(server.received_requests().await.expect("requests").len(), 1);
            server.verify().await;
        }
    }

    #[tokio::test]
    #[allow(
        clippy::too_many_lines,
        reason = "one deferred evidence sequence, with exact-candidate and fallback controls"
    )]
    async fn deferred_storno_warning_survives_journal_only_for_its_exact_candidate() {
        use crate::test_support::Doc;
        for verified_number in ["SS-CANDIDATE", "SS-OTHER"] {
            let server = MockServer::start().await;
            let mut account = Account::new("account", "reference");
            account.endpoint = Endpoint::parse(&server.uri()).expect("endpoint");
            let gateway = open_gateway(account, Credentials::agent_key("PRIVATE-KEY"));
            let external_id = ExternalId::new("acct:ORD-1:storno:SZ-1");
            let marker = UnresolvedWrite {
                prepayment_number: None,
                execution_contract: None,
                proforma_number: None,
                version: MarkerVersion,
                token: "owner".into(),
                owner_invocation: "owner".into(),
                created_at: "2026-09-14T12:00:00Z".into(),
                scope: None,
                order: OrderKey::parse("ORD-1").expect("order"),
                namespace: "acct".parse().expect("namespace"),
                external_id: external_id.to_string(),
                account_id: "account".into(),
                endpoint: server.uri(),
                credential_ref: "reference".into(),
                operation: WriteOperation::Storno {
                    number: "SZ-1".into(),
                },
            };
            query("szamlaKulsoAzon", external_id.as_str())
                .respond_with(api_error("7", "absent"))
                .up_to_n_times(1)
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(body_string_contains("action-szamla_agent_st"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("szlahu_error_code", "56")
                        .insert_header("szlahu_error", "PRIVATE-DIAGNOSTIC")
                        .insert_header("szlahu_szamlaszam", "SS-CANDIDATE")
                        .set_body_string(crate::test_support::numbered_reply_body(
                            "SS-CANDIDATE",
                            None,
                        )),
                )
                .expect(1)
                .mount(&server)
                .await;
            query("szamlaszam", "SS-CANDIDATE")
                .respond_with(api_error("1", "PRIVATE-MAINTENANCE"))
                .up_to_n_times(2)
                .expect(2)
                .mount(&server)
                .await;
            // The first deferred read fails too. Its fallback cannot settle.
            query("szamlaKulsoAzon", external_id.as_str())
                .respond_with(api_error("7", "absent"))
                .up_to_n_times(1)
                .expect(1)
                .mount(&server)
                .await;
            query("rendelesSzam", "ORD-1")
                .respond_with(api_error("7", "absent"))
                .expect(1)
                .mount(&server)
                .await;
            let result = gateway
                .protected_storno(
                    crate::gateway::StornoStepRequest {
                        invoice_number: "SZ-1",
                        external_id: &external_id,
                        comment: None,
                        buyer_email: Some("notification@example.test"),
                        e_invoice: false,
                        fulfillment_date: jiff::civil::date(2026, 9, 11),
                    },
                    &marker,
                )
                .await;
            let journal = serde_json::to_string(&result).expect("journal");
            assert!(!journal.contains("PRIVATE-"));
            let WriteResult::Unresolved(diagnostic) =
                serde_json::from_str(&journal).expect("replay")
            else {
                panic!("expected retained uncertainty: {journal}");
            };
            assert_eq!(diagnostic.candidate_number.as_deref(), Some("SS-CANDIDATE"));
            assert!(diagnostic.notification_delivery_failed);
            assert!(matches!(
                gateway
                    .reconcile_write_diagnostic(&marker, &diagnostic)
                    .await,
                WriteResult::Unresolved(_)
            ));

            let reversal = Doc {
                referenced_invoice: Some("SZ-1"),
                ..Doc::new(verified_number, "SS")
            };
            query("szamlaszam", "SS-CANDIDATE")
                .respond_with(if verified_number == "SS-CANDIDATE" {
                    reversal.response()
                } else {
                    api_error("7", "absent")
                })
                .expect(1)
                .mount(&server)
                .await;
            if verified_number != "SS-CANDIDATE" {
                query("szamlaKulsoAzon", external_id.as_str())
                    .respond_with(reversal.response())
                    .expect(1)
                    .mount(&server)
                    .await;
            }
            query("szamlaszam", "SZ-1")
                .respond_with(Doc::reversed("SZ-1", "SZ").response())
                .expect(1)
                .mount(&server)
                .await;
            let result = gateway
                .reconcile_write_diagnostic(&marker, &diagnostic)
                .await;
            match result {
                WriteResult::Storno(StornoOutcome::Reversed(issued)) => {
                    assert_eq!(verified_number, "SS-CANDIDATE");
                    assert_eq!(issued.number, verified_number);
                    assert!(issued.notification_delivery_failed);
                    assert!(issued.document_id.is_some());
                }
                WriteResult::Storno(StornoOutcome::AlreadyReversed { storno_number, .. }) => {
                    assert_eq!(verified_number, "SS-OTHER");
                    assert_eq!(storno_number, verified_number);
                }
                other => panic!("expected verified reversal: {other:?}"),
            }
            server.verify().await;
        }
    }

    #[tokio::test]
    async fn protected_duplicate_diagnostics_alert_without_changing_refusal() {
        use crate::gateway::CreateStepRequest;
        use szamlazz_agent::ops::invoice::{Buyer, CreateInvoice, InvoiceHeader, InvoiceKind};
        let capture = LogCapture::default();
        let _guard = capture.subscribe();
        LogCapture::rebuild_interest();
        for hint in [false, true] {
            let server = MockServer::start().await;
            let mut account = Account::new("account", "reference");
            account.endpoint = Endpoint::parse(&server.uri()).expect("endpoint");
            let gateway = open_gateway(account, Credentials::agent_key("PRIVATE-KEY"));
            let external_id = ExternalId::new("acct:ORD-1:invoice");
            let order = OrderKey::parse("ORD-1").expect("order");
            let header = InvoiceHeader::new(
                jiff::civil::date(2026, 9, 11),
                jiff::civil::date(2026, 9, 11),
                szamlazz_agent::PaymentMethod::Transfer,
                szamlazz_agent::Currency::HUF,
                szamlazz_agent::Language::Hungarian,
            );
            let mut create = CreateInvoice::new(
                InvoiceKind::invoice(),
                header,
                Buyer::new("Buyer", "1000", "City", "Street"),
                vec![
                    szamlazz_agent::LineItem::try_calculated(
                        "item",
                        rust_decimal::dec!(1),
                        "db",
                        rust_decimal::dec!(1),
                        szamlazz_agent::VatRate::Aam,
                        szamlazz_agent::Rounding::Exact,
                    )
                    .expect("item"),
                ],
            );
            create.header.order_number = Some("ORD-1".into());
            create.external_id = Some(external_id.as_str().to_owned());
            Mock::given(body_string_contains("action-xmlagentxmlfile"))
                .respond_with(api_error("152", "duplicate"))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(body_string_contains("action-szamla_agent_xml"))
                .and(body_string_contains(
                    "<szamlaKulsoAzon>acct:ORD-1:invoice</szamlaKulsoAzon>",
                ))
                .respond_with(if hint {
                    api_error("7", "absent")
                } else {
                    api_error("135", "PRIVATE-DIAGNOSTIC")
                })
                .expect(1)
                .mount(&server)
                .await;
            if hint {
                Mock::given(body_string_contains("<rendelesSzam>ORD-1</rendelesSzam>"))
                    .respond_with(api_error("135", "PRIVATE-DIAGNOSTIC"))
                    .expect(1)
                    .mount(&server)
                    .await;
            }
            let before = capture
                .logs()
                .matches("fix the account's agent key")
                .count();
            let result = gateway
                .create_send(
                    &CreateStepRequest::new(
                        &external_id,
                        IssuedKind::Invoice,
                        &order,
                        &create,
                        None,
                        None,
                    )
                    .expect("consistent create intent"),
                    crate::gateway::CreateSettlement::Retained,
                )
                .await
                .expect("settled refusal");
            assert!(
                matches!(result, CreateOutcome::DuplicateOrderNumber { .. }),
                "{result:?}"
            );
            let logs = capture.logs();
            assert_eq!(
                logs.matches("fix the account's agent key").count(),
                before + 1,
                "{logs}"
            );
            assert!(!logs.contains("PRIVATE-"), "{logs}");
            server.verify().await;
        }
    }

    #[tokio::test]
    async fn reconciliation_alerts_before_fallback_and_keeps_uncertainty() {
        let capture = LogCapture::default();
        let _guard = capture.subscribe();
        LogCapture::rebuild_interest();
        for candidate in [None, Some("SS-CANDIDATE")] {
            let server = MockServer::start().await;
            let mut account = Account::new("account", "reference");
            account.endpoint = Endpoint::parse(&server.uri()).expect("endpoint");
            let gateway = open_gateway(account, Credentials::agent_key("PRIVATE-KEY"));
            let marker = UnresolvedWrite {
                prepayment_number: None,
                execution_contract: None,
                proforma_number: None,
                version: MarkerVersion,
                token: "owner".into(),
                owner_invocation: "owner".into(),
                created_at: "2026-09-11T12:00:00Z".into(),
                scope: None,
                order: OrderKey::parse("ORD-1").expect("order"),
                namespace: "acct".parse().expect("namespace"),
                external_id: if candidate.is_some() {
                    "acct:ORD-1:storno:SZ-1"
                } else {
                    "acct:ORD-1:invoice"
                }
                .into(),
                account_id: "account".into(),
                endpoint: server.uri(),
                credential_ref: "reference".into(),
                operation: if candidate.is_some() {
                    WriteOperation::Storno {
                        number: "SZ-1".into(),
                    }
                } else {
                    WriteOperation::Create {
                        kind: IssuedKind::Invoice,
                        expected_number: None,
                        corrected_number: None,
                    }
                },
            };
            Mock::given(body_string_contains(
                candidate.unwrap_or(&marker.external_id),
            ))
            .respond_with(api_error("135", "PRIVATE-VENDOR-CREDENTIALS"))
            .expect(1)
            .mount(&server)
            .await;
            if candidate.is_some() {
                Mock::given(body_string_contains(&marker.external_id))
                    .respond_with(ResponseTemplate::new(503))
                    .expect(1)
                    .mount(&server)
                    .await;
            }
            let before = capture
                .logs()
                .matches("fix the account's agent key")
                .count();
            let result = gateway.reconcile_write(&marker, candidate).await;
            assert!(matches!(result, WriteResult::Unresolved(_)), "{result:?}");
            let logs = capture.logs();
            assert_eq!(
                logs.matches("fix the account's agent key").count(),
                before + 1,
                "{logs}"
            );
            assert!(
                logs.contains("code=135") && logs.contains("namespace=acct"),
                "{logs}"
            );
            assert!(!logs.contains("PRIVATE-"), "{logs}");
            server.verify().await;
        }
    }

    #[tokio::test]
    async fn numberless_protected_storno_stays_unresolved_and_reconciliation_never_resends() {
        let server = MockServer::start().await;
        let mut account = Account::new("account", "reference");
        account.endpoint = Endpoint::parse(&server.uri()).expect("endpoint");
        let gateway = open_gateway(account, Credentials::agent_key("PRIVATE-KEY"));
        let external_id = ExternalId::new("acct:ORD-1:storno:SZ-1");
        let marker = UnresolvedWrite {
            prepayment_number: None,
            execution_contract: None,
            proforma_number: None,
            version: MarkerVersion,
            token: "owner".into(),
            owner_invocation: "owner".into(),
            created_at: "2026-09-11T12:00:00Z".into(),
            scope: None,
            order: OrderKey::parse("ORD-1").expect("order"),
            namespace: "acct".parse().expect("namespace"),
            external_id: external_id.to_string(),
            account_id: "account".into(),
            endpoint: server.uri(),
            credential_ref: "reference".into(),
            operation: WriteOperation::Storno {
                number: "SZ-1".into(),
            },
        };
        Mock::given(body_string_contains("action-szamla_agent_xml"))
            .respond_with(api_error("7", "not found"))
            .expect(5)
            .mount(&server)
            .await;
        Mock::given(body_string_contains("action-szamla_agent_st"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><szamlabrutto>-1270</szamlabrutto><vevoifiokurl>PRIVATE-URL</vevoifiokurl><pdf>JVBERi0=</pdf></xmlszamlavalasz>"#,
            ))
            .expect(1)
            .mount(&server)
            .await;
        let result = gateway
            .protected_storno(
                crate::gateway::StornoStepRequest {
                    invoice_number: "SZ-1",
                    external_id: &external_id,
                    comment: None,
                    buyer_email: None,
                    e_invoice: false,
                    fulfillment_date: jiff::civil::date(2026, 9, 11),
                },
                &marker,
            )
            .await;
        assert!(matches!(&result, WriteResult::Unresolved(_)), "{result:?}");
        let journal = serde_json::to_string(&result).expect("serialize");
        assert!(journal.contains("without a document number"));
        assert!(!journal.contains("PRIVATE-") && !journal.contains("JVBERi0="));
        // This is the continuation used after an uncertain protected send:
        // repeated absence retains the marker, never authorizes another send.
        for _ in 0..2 {
            let result = gateway.reconcile_write(&marker, None).await;
            assert!(matches!(result, WriteResult::Unresolved(_)), "{result:?}");
        }
        server.verify().await;
    }

    #[tokio::test]
    async fn immediate_storno_verification_alerts_and_retains_candidate() {
        use crate::gateway::StornoStepRequest;
        let capture = LogCapture::default();
        let _guard = capture.subscribe();
        LogCapture::rebuild_interest();
        let server = MockServer::start().await;
        let mut account = Account::new("account", "reference");
        account.endpoint = Endpoint::parse(&server.uri()).expect("endpoint");
        let gateway = open_gateway(account, Credentials::agent_key("PRIVATE-KEY"));
        let external_id = ExternalId::new("acct:ORD-1:storno:SZ-1");
        let marker = UnresolvedWrite {
            prepayment_number: None,
            execution_contract: None,
            proforma_number: None,
            version: MarkerVersion,
            token: "owner".into(),
            owner_invocation: "owner".into(),
            created_at: "2026-09-11T12:00:00Z".into(),
            scope: None,
            order: OrderKey::parse("ORD-1").expect("order"),
            namespace: "acct".parse().expect("namespace"),
            external_id: external_id.to_string(),
            account_id: "account".into(),
            endpoint: server.uri(),
            credential_ref: "reference".into(),
            operation: WriteOperation::Storno {
                number: "SZ-1".into(),
            },
        };
        Mock::given(body_string_contains("action-szamla_agent_st"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                crate::test_support::numbered_reply_body("SS-CANDIDATE", None),
            ))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(body_string_contains(
            "<szamlaszam>SS-CANDIDATE</szamlaszam>",
        ))
        .respond_with(api_error("135", "PRIVATE-DIAGNOSTIC"))
        .expect(1)
        .mount(&server)
        .await;
        let result = gateway
            .storno_send(
                StornoStepRequest {
                    invoice_number: "SZ-1",
                    external_id: &external_id,
                    comment: None,
                    buyer_email: None,
                    e_invoice: false,
                    fulfillment_date: jiff::civil::date(2026, 9, 11),
                },
                Some(&marker),
            )
            .await;
        assert!(
            matches!(result, Err(super::super::Unconfirmed::StornoVerification { ref number, .. }) if number == "SS-CANDIDATE"),
            "{result:?}"
        );
        let logs = capture.logs();
        assert_eq!(
            logs.matches("fix the account's agent key").count(),
            1,
            "{logs}"
        );
        assert!(
            logs.contains("code=135") && logs.contains("namespace=acct"),
            "{logs}"
        );
        assert!(!logs.contains("PRIVATE-"), "{logs}");
        server.verify().await;
    }
}
