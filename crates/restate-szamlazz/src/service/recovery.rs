//! Cross-invocation marker and execution-local send permission.

use std::future::Future;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tracing::Instrument as _;

use restate_sdk::context::{
    ContextReadState, ContextSideEffects, ContextWriteState, RunFuture as _, RunRetryPolicy,
};
use restate_sdk::errors::{HandlerError, TerminalError};
use restate_sdk::prelude::{ObjectContext, SharedObjectContext};
use restate_sdk::serde::Json;

use super::{Order, prologue::Execution, support::Fault};
use crate::account::Account;
use crate::contract::recovery::{
    AttestedCompletion, MarkerVersion, RecoveryEvidence, RecoveryRequest, RecoveryResponse,
    UnresolvedObservation, UnresolvedWrite, WriteOperation,
};
use crate::gateway::{Gateway, recovery::WriteResult};
use crate::identity::{ExternalId, OrderKey};

const STATE: &str = "unresolved-write";

#[derive(Clone, Copy)]
enum ReplayWrite {
    Create,
    Delete,
    Storno,
}

impl ReplayWrite {
    const fn step(self) -> &'static str {
        match self {
            Self::Create => "ordinary",
            Self::Delete => "proforma-delete",
            Self::Storno => "order-storno",
        }
    }
    const fn write_name(self) -> &'static str {
        match self {
            Self::Create => "create-ordinary-request-response",
            Self::Delete => "delete-proforma-request-response",
            Self::Storno => "storno-request-response",
        }
    }
}

/// A distinct serialized run result as well as state discriminator. Run names
/// alone are not an exceptional-replay fence: an old prepare-write result must
/// fail decoding rather than become ordinary resend permission.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(transparent)]
pub(super) struct OrdinaryIntent {
    marker: UnresolvedWrite,
}

impl<'de> serde::Deserialize<'de> for OrdinaryIntent {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let marker = UnresolvedWrite::deserialize(de)?;
        if !valid_execution_contract(&marker) || marker.execution_contract.is_none() {
            return Err(serde::de::Error::custom(
                "ordinary intent requires execution contract",
            ));
        }
        Ok(Self { marker })
    }
}

impl OrdinaryIntent {
    pub(super) fn new(mut marker: UnresolvedWrite) -> Self {
        if marker.execution_contract.is_none() {
            marker.execution_contract = Some(
                crate::contract::recovery::OrdinaryExecutionContract::RequestResponseOrdinaryV1,
            );
        }
        Self { marker }
    }
}

/// Interruption boundaries exposed only for real-runtime protocol tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteCheckpoint {
    /// Before the marker command.
    BeforeMarker,
    /// After the marker command, before acknowledged arming.
    AfterMarker,
    /// After acknowledged arming, before consuming permission.
    #[cfg(feature = "test-util")]
    Armed,
    /// Experimental ordinary write: fresh target and guards passed, before send.
    OrdinaryGuardsPassed,
    /// After the write returns, before recording its result.
    Sent,
    /// After a positive reconciliation query, before recording it.
    Reconciled,
    /// After recorded settlement, before clearing the marker.
    Settled,
    /// After recorded document verification, before recording recovery.
    #[cfg(feature = "test-util")]
    RecoveryVerified,
    /// After recorded recovery evidence, before clearing its marker.
    #[cfg(feature = "test-util")]
    RecoveryRecorded,
}

/// Test-only interruption control; a deployment must not enable `test-util`.
#[cfg(feature = "test-util")]
pub trait WriteObserver: Send + Sync {
    /// Wait at an execution boundary. Never called by a completed run replay.
    fn reached(&self, order: &str, point: WriteCheckpoint) -> crate::account::BoxFuture<'_, ()>;
}

// Require the same sealed privacy registry at the direct SDK boundary used for
// arming and invocation-policy reconciliation (neither takes a run policy).
fn journal<T: super::support::Journaled>(value: T) -> Json<T> {
    Json(value)
}

impl Order {
    /// Install test-only interruption control for this endpoint.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub fn with_write_observer(mut self, observer: Arc<dyn WriteObserver>) -> Self {
        self.write_observer = Some(observer);
        self
    }
    pub(super) async fn observe_marker(
        &self,
        ctx: &SharedObjectContext<'_>,
    ) -> Result<UnresolvedObservation, HandlerError> {
        let state = ctx.get::<bytes::Bytes>(STATE).await.map_err(read_fault)?;
        Ok(match state {
            None => UnresolvedObservation::Absent,
            Some(raw) => {
                if let Some(marker) = decode_marker(&raw, ctx.scope(), ctx.key()) {
                    let value: serde_json::Value = serde_json::from_slice(&raw)
                        .map_err(|_| Fault::unavailable("could not read marker JSON"))?;
                    if serde_json::to_value(&marker).ok().as_ref() != Some(&value) {
                        return Ok(UnresolvedObservation::Other {
                            state: "unresolved".into(),
                            fields: serde_json::Map::from_iter([("marker".into(), value)]),
                        });
                    }
                    UnresolvedObservation::Unresolved {
                        marker: Box::new(marker),
                    }
                } else {
                    if self.parts.config.order_execution.permits_replay()
                        && let Ok(marker) = serde_json::from_slice::<serde_json::Value>(&raw)
                    {
                        return Ok(UnresolvedObservation::Other {
                            state: "unresolved".into(),
                            fields: serde_json::Map::from_iter([("marker".into(), marker)]),
                        });
                    }
                    UnresolvedObservation::Unreadable
                }
            }
        })
    }

    pub(super) async fn recover_marker(
        &self,
        ctx: &ObjectContext<'_>,
        request: RecoveryRequest,
        exact_marker: serde_json::Value,
    ) -> Result<RecoveryResponse, HandlerError> {
        let sdk_span = tracing::Span::current();
        let span =
            super::prologue::execution_span(ctx.scope(), Some(ctx.key()), ctx.invocation_id());
        super::prologue::SDK_SPAN
            .scope(
                sdk_span,
                self.recover_marker_inner(ctx, request, exact_marker)
                    .instrument(span),
            )
            .await
    }

    #[allow(
        clippy::too_many_lines,
        reason = "keep recovery evidence recording and state clearance in command order"
    )]
    async fn recover_marker_inner(
        &self,
        ctx: &ObjectContext<'_>,
        request: RecoveryRequest,
        exact_marker: serde_json::Value,
    ) -> Result<RecoveryResponse, HandlerError> {
        let raw = ctx
            .get::<bytes::Bytes>(STATE)
            .await
            .map_err(read_fault)?
            .ok_or_else(|| Fault::invalid_input("no unresolved marker matches this recovery"))?;
        let marker = decode_marker(&raw, ctx.scope(), ctx.key()).ok_or_else(|| {
            Fault::outcome_unknown("unreadable unresolved marker; use a compatible deployment")
        })?;
        let stored: serde_json::Value = serde_json::from_slice(&raw)
            .map_err(|_| Fault::outcome_unknown("unreadable marker JSON"))?;
        if marker != request.marker || stored != exact_marker {
            return Err(Fault::invalid_input(
                "recovery does not match the exact unresolved marker",
            )
            .into());
        }
        tracing::Span::current().record("account.id", tracing::field::display(&marker.account_id));
        let account = pinned_account(&marker)?;
        let config = crate::config::WorkerConfig {
            namespace: marker.namespace.clone(),
            ..self.parts.config.clone().into_inner()
        };
        let execution = Execution::new(account, self.parts.accounts.clone(), config);
        let evidence = request.evidence;
        match &evidence {
            RecoveryEvidence::NotExecuted {
                audit_reference, ..
            }
            | RecoveryEvidence::Completed {
                audit_reference, ..
            } if audit_reference.trim().is_empty() => {
                return Err(Fault::invalid_input(
                    "an operator attestation requires an audit reference",
                )
                .into());
            }
            RecoveryEvidence::NotExecuted { .. } => {}
            RecoveryEvidence::Completed { completion, .. } => {
                validate_completion(&marker.operation, completion)?;
            }
            RecoveryEvidence::Document { number } => {
                let result = ctx
                    .run(|| async {
                        super::prologue::mark_fresh_work();
                        let gateway = execution.gateway().await.map_err(HandlerError::from)?;
                        Ok(journal(
                            gateway
                                .reconcile_write_checked(&marker, Some(number.as_str()))
                                .await?,
                        ))
                    })
                    .name("verify-recovery")
                    .retry_policy(self.parts.config.read.run_retry_policy())
                    .await
                    .map_err(read_fault)?;
                #[cfg(feature = "test-util")]
                if let Some(observer) = &self.write_observer {
                    observer
                        .reached(ctx.key(), WriteCheckpoint::RecoveryVerified)
                        .await;
                }
                if let WriteResult::Answered {
                    credentials,
                    answer,
                } = result.0
                {
                    let code = if credentials {
                        super::support::AnsweredCode::CredentialsRejected(answer)
                    } else {
                        super::support::AnsweredCode::Inconclusive(answer)
                    };
                    return Err(code.into_fault(&marker.namespace).into());
                }
                if let WriteResult::Unresolved(diagnostic) = result.0 {
                    return Err(Fault::outcome_unknown(format!("document evidence does not settle the exact unresolved write; marker retained: {}", diagnostic.reason)).into());
                }
            }
        }
        let receipt = RecoveryResponse {
            token: marker.token,
            operator: request.operator,
            evidence: serde_json::to_value(evidence)
                .map_err(|_| Fault::unavailable("could not encode recovery evidence"))?,
        };
        let receipt = ctx
            .run(|| async { Ok(journal(receipt)) })
            .name("record-recovery")
            .await
            .map_err(read_fault)?;
        #[cfg(feature = "test-util")]
        if let Some(observer) = &self.write_observer {
            observer
                .reached(ctx.key(), WriteCheckpoint::RecoveryRecorded)
                .await;
        }
        ctx.clear(STATE);
        Ok(receipt.0)
    }
}

fn pinned_account(marker: &UnresolvedWrite) -> Result<Account, Fault> {
    let mut account = Account::new(marker.account_id.clone(), marker.credential_ref.clone());
    account.endpoint = marker
        .endpoint
        .parse()
        .map_err(|_| Fault::invalid_input("invalid pinned endpoint"))?;
    Ok(account)
}

fn validate_completion(
    operation: &WriteOperation,
    completion: &AttestedCompletion,
) -> Result<(), Fault> {
    let matches = match (operation, completion) {
        (
            WriteOperation::Create {
                expected_number, ..
            },
            AttestedCompletion::Issued { number },
        ) => expected_number.as_deref() != Some(number.as_str()),
        (WriteOperation::Storno { number: original }, AttestedCompletion::Reversed { number }) => {
            original != number.as_str()
        }
        (WriteOperation::Delete { number: target }, AttestedCompletion::Deleted { number }) => {
            target == number.as_str()
        }
        _ => false,
    };
    if matches {
        Ok(())
    } else {
        Err(Fault::invalid_input(
            "attested completion does not match the unresolved operation and expected document",
        ))
    }
}

fn decode_marker(raw: &[u8], scope: Option<&str>, key: &str) -> Option<UnresolvedWrite> {
    let marker: UnresolvedWrite = serde_json::from_slice(raw).ok()?;
    if !valid_execution_contract(&marker) {
        return None;
    }
    if marker.execution_contract.is_some() {
        if marker.proforma_number.as_ref().is_some_and(|n| {
            n.parse::<crate::contract::ProviderDocumentNumber>()
                .is_err()
        }) {
            return None;
        }
    } else if marker.proforma_number.is_some() {
        return None;
    }
    if marker.scope.as_deref() != scope
        || marker.order.as_str() != key
        || marker.token.is_empty()
        || marker.token != marker.owner_invocation
        || marker.endpoint.parse::<crate::account::Endpoint>().is_err()
    {
        return None;
    }
    // AccountId and CredentialRef are resolver-owned opaque strings. Empty
    // values are legal on Account and must remain usable in its recovery marker.
    if marker.created_at.parse::<jiff::Timestamp>().is_err() {
        return None;
    }
    let expected = match &marker.operation {
        WriteOperation::Create {
            kind,
            expected_number,
            corrected_number,
        } => {
            if expected_number
                .as_ref()
                .is_some_and(|n| n.parse::<crate::identity::InvoiceNumber>().is_err())
            {
                return None;
            }
            if *kind == crate::identity::IssuedKind::Corrective {
                let base = corrected_number.as_ref()?;
                base.parse::<crate::identity::InvoiceNumber>().ok()?;
                if expected_number.is_some() {
                    return None;
                }
                let prefix = format!("{}:{}:corrective:", marker.namespace, marker.order);
                let correction = marker
                    .external_id
                    .strip_prefix(&prefix)?
                    .parse::<crate::identity::CorrectionId>()
                    .ok()?;
                ExternalId::for_corrective(&marker.namespace, &marker.order, &correction)
            } else {
                if corrected_number.is_some() {
                    return None;
                }
                let kind = match kind {
                    crate::identity::IssuedKind::Proforma => {
                        crate::identity::DocumentKind::Proforma
                    }
                    crate::identity::IssuedKind::Invoice => crate::identity::DocumentKind::Invoice,
                    crate::identity::IssuedKind::Prepayment => {
                        crate::identity::DocumentKind::Prepayment
                    }
                    crate::identity::IssuedKind::Final => crate::identity::DocumentKind::Final,
                    crate::identity::IssuedKind::Corrective => return None,
                };
                ExternalId::for_kind(&marker.namespace, &marker.order, kind)
            }
        }
        WriteOperation::Storno { number } => {
            let number = number.parse::<crate::identity::InvoiceNumber>().ok()?;
            ExternalId::for_storno(&marker.namespace, &marker.order, &number)
        }
        WriteOperation::Delete { number } => {
            number.parse::<crate::identity::InvoiceNumber>().ok()?;
            ExternalId::for_kind(
                &marker.namespace,
                &marker.order,
                crate::identity::DocumentKind::Proforma,
            )
        }
    };
    if marker.external_id != expected.as_str() {
        return None;
    }
    Some(marker)
}

fn valid_execution_contract(marker: &UnresolvedWrite) -> bool {
    use crate::contract::recovery::OrdinaryExecutionContract as Contract;
    use crate::identity::IssuedKind;
    if marker.prepayment_number.is_some()
        && marker.execution_contract != Some(Contract::RequestResponseFinalV1)
    {
        return false;
    }
    match (&marker.execution_contract, &marker.operation) {
        (
            Some(Contract::RequestResponseOrdinaryV1),
            WriteOperation::Create {
                kind: IssuedKind::Invoice,
                corrected_number: None,
                ..
            },
        )
        | (
            Some(Contract::RequestResponsePrepaymentV1),
            WriteOperation::Create {
                kind: IssuedKind::Prepayment,
                corrected_number: None,
                ..
            },
        ) => true,
        (
            Some(Contract::RequestResponseFinalV1),
            WriteOperation::Create {
                kind: IssuedKind::Final,
                corrected_number: None,
                ..
            },
        ) => {
            marker.proforma_number.is_none()
                && marker
                    .prepayment_number
                    .as_ref()
                    .is_some_and(|n| n.parse::<crate::contract::ProviderDocumentNumber>().is_ok())
        }
        (
            Some(Contract::RequestResponseProformaV1),
            WriteOperation::Create {
                kind: IssuedKind::Proforma,
                corrected_number: None,
                ..
            },
        )
        | (None, _)
        | (Some(Contract::RequestResponseDeleteV1 { .. }), WriteOperation::Delete { .. }) => {
            marker.proforma_number.is_none()
        }
        (
            Some(Contract::RequestResponseStornoV1 {
                fulfillment_date, ..
            }),
            WriteOperation::Storno { .. },
        ) => marker.proforma_number.is_none() && (1..=9999).contains(&fulfillment_date.year()),
        _ => false,
    }
}

pub(super) async fn guard(ctx: &ObjectContext<'_>) -> Result<(), HandlerError> {
    if ctx
        .get::<bytes::Bytes>(STATE)
        .await
        .map_err(read_fault)?
        .is_some()
    {
        return Err(Fault::outcome_unknown("an earlier Order write remains unresolved; retain its invocation identity and obtain operator reconciliation; no new mutation is authorized").into());
    }
    Ok(())
}

impl Execution {
    pub(super) async fn request_response_storno(
        &self,
        ctx: &ObjectContext<'_>,
        order: &OrderKey,
        request: crate::gateway::StornoStepRequest<'_>,
        original: &crate::gateway::FoundDocument,
    ) -> Result<WriteResult, HandlerError> {
        let contract =
            crate::contract::recovery::OrdinaryExecutionContract::RequestResponseStornoV1 {
                document_id: original.document_id,
                fulfillment_date: request.fulfillment_date,
                e_invoice: request.e_invoice,
                appearance: original.appearance,
            };
        let marker = UnresolvedWrite {
            execution_contract: Some(contract),
            proforma_number: None,
            prepayment_number: None,
            version: MarkerVersion,
            token: ctx.invocation_id().to_owned(),
            owner_invocation: ctx.invocation_id().to_owned(),
            created_at: String::new(),
            scope: ctx.scope().map(str::to_owned),
            order: order.clone(),
            namespace: self.config.namespace.clone(),
            external_id: request.external_id.as_str().to_owned(),
            account_id: self.account.id.to_string(),
            endpoint: self.account.endpoint.as_str().to_owned(),
            credential_ref: self.account.credential_ref.to_string(),
            operation: WriteOperation::Storno {
                number: request.invoice_number.to_owned(),
            },
        };
        self.request_response_write(
            ctx,
            order,
            request.external_id,
            marker,
            ReplayWrite::Storno,
            move |gateway, marker| async move {
                gateway
                    .request_response_storno(request, &marker, original)
                    .await
            },
        )
        .await
    }
    /// Separate admission/run identity for the isolated ordinary experiment.
    /// Every non-positive result after the barrier retains earlier uncertainty.
    pub(super) async fn ordinary_request_response(
        &self,
        ctx: &ObjectContext<'_>,
        order: &OrderKey,
        request: crate::gateway::CreateStepRequest<'_>,
        external_id: &ExternalId,
    ) -> Result<WriteResult, HandlerError> {
        let marker = UnresolvedWrite {
            execution_contract: Some(match request.operation() {
                WriteOperation::Create {kind:crate::identity::IssuedKind::Prepayment,..} => crate::contract::recovery::OrdinaryExecutionContract::RequestResponsePrepaymentV1,
                WriteOperation::Create {kind:crate::identity::IssuedKind::Final,..} => crate::contract::recovery::OrdinaryExecutionContract::RequestResponseFinalV1,
                WriteOperation::Create {
                    kind: crate::identity::IssuedKind::Proforma,
                    ..
                } => {
                    crate::contract::recovery::OrdinaryExecutionContract::RequestResponseProformaV1
                }
                _ => {
                    crate::contract::recovery::OrdinaryExecutionContract::RequestResponseOrdinaryV1
                }
            }),
            proforma_number: request.proforma_number().map(str::to_owned),
            prepayment_number: request.prepayment_number().map(str::to_owned),
            version: MarkerVersion,
            token: ctx.invocation_id().to_owned(),
            owner_invocation: ctx.invocation_id().to_owned(),
            created_at: String::new(),
            scope: ctx.scope().map(str::to_owned),
            order: order.clone(),
            namespace: self.config.namespace.clone(),
            external_id: external_id.as_str().to_owned(),
            account_id: self.account.id.to_string(),
            endpoint: self.account.endpoint.as_str().to_owned(),
            credential_ref: self.account.credential_ref.to_string(),
            operation: request.operation(),
        };
        self.request_response_write(
            ctx,
            order,
            external_id,
            marker,
            ReplayWrite::Create,
            move |gateway, marker| async move {
                if marker.operation != request.operation()
                    || marker.proforma_number.as_deref() != request.proforma_number()
                    || marker.prepayment_number.as_deref() != request.prepayment_number()
                {
                    return WriteResult::unresolved(
                        "outbound request differs from retained issuance intent",
                    );
                }
                gateway
                    .ordinary_request_response(request, || {
                        self.checkpoint(order, WriteCheckpoint::OrdinaryGuardsPassed)
                    })
                    .await
            },
        )
        .await
    }

    pub(super) async fn request_response_delete(
        &self,
        ctx: &ObjectContext<'_>,
        order: &OrderKey,
        external_id: &ExternalId,
        found: Box<crate::gateway::FoundDocument>,
        request: crate::contract::DeleteProformaRequest,
    ) -> Result<WriteResult, HandlerError> {
        let contract =
            crate::contract::recovery::OrdinaryExecutionContract::RequestResponseDeleteV1 {
                mode: request.mode,
                force: request.force,
                document_id: found.document_id,
            };
        let marker = UnresolvedWrite {
            execution_contract: Some(contract),
            proforma_number: None,
            prepayment_number: None,
            version: MarkerVersion,
            token: ctx.invocation_id().to_owned(),
            owner_invocation: ctx.invocation_id().to_owned(),
            created_at: String::new(),
            scope: ctx.scope().map(str::to_owned),
            order: order.clone(),
            namespace: self.config.namespace.clone(),
            external_id: external_id.as_str().to_owned(),
            account_id: self.account.id.to_string(),
            endpoint: self.account.endpoint.as_str().to_owned(),
            credential_ref: self.account.credential_ref.to_string(),
            operation: WriteOperation::Delete {
                number: found.number.clone(),
            },
        };
        self.request_response_write(ctx,order,external_id,marker,ReplayWrite::Delete,move |gateway,marker| async move {
            if marker.execution_contract!=Some(contract) || marker.operation!=(WriteOperation::Delete {number:found.number.clone()}) {
                return WriteResult::unresolved("deletion request differs from retained intent");
            }
            if request.mode==crate::contract::DeleteMode::NamespaceOwned {
                match gateway.lookup_ours(external_id,order,crate::identity::IssuedKind::Proforma).await {
                    Ok(crate::gateway::OwnershipOutcome::Live(current)|crate::gateway::OwnershipOutcome::Reversed(current)) if current.number==found.number && current.document_id==found.document_id => {},
                    Ok(crate::gateway::OwnershipOutcome::CredentialsRejected(answer)) => { answer.warn_credentials_rejected(external_id.namespace());return WriteResult::unresolved(format!("deletion namespace credentials rejected: {}",answer.code)); },
                    Ok(crate::gateway::OwnershipOutcome::Absent)=>return WriteResult::unresolved("deletion namespace holder absent; absence does not settle deletion"),
                    Ok(crate::gateway::OwnershipOutcome::Api(answer))=>return WriteResult::unresolved(format!("deletion namespace vendor code {}",answer.code)),
                    Err(cause)=>return WriteResult::unresolved(format!("deletion namespace query unanswered: {cause}")),
                    _=>return WriteResult::unresolved("deletion namespace target changed or collided"),
                }
            }
            match gateway.delete_proforma(&found,order,request.force).await {
                crate::gateway::DeleteOutcome::Deleted=>WriteResult::Delete(crate::gateway::DeleteOutcome::Deleted),
                crate::gateway::DeleteOutcome::CredentialsRejected(answer)=>{answer.warn_credentials_rejected(external_id.namespace());WriteResult::unresolved(format!("deletion credentials rejected: {}",answer.code))},
                crate::gateway::DeleteOutcome::AlreadyGone=>WriteResult::unresolved("proforma reported absent; absence does not settle earlier deletion"),
                crate::gateway::DeleteOutcome::Paid=>WriteResult::unresolved("fresh deletion paid guard refused; earlier deletion remains unresolved"),
                crate::gateway::DeleteOutcome::TargetChanged=>WriteResult::unresolved("fresh deletion target changed; earlier deletion remains unresolved"),
                crate::gateway::DeleteOutcome::Api(answer)=>WriteResult::unresolved(format!("deletion guard vendor code {}",answer.code)),
                crate::gateway::DeleteOutcome::Rejected(rejection)=>WriteResult::unresolved(format!("deletion refused: {}; earlier execution remains unresolved",rejection.code)),
                crate::gateway::DeleteOutcome::Inconclusive(answer)=>WriteResult::unresolved(format!("inconclusive deletion code {}",answer.code)),
                crate::gateway::DeleteOutcome::GuardFailed(cause)=>WriteResult::unresolved(format!("deletion guard unanswered: {cause}")),
                crate::gateway::DeleteOutcome::Lost(cause)=>WriteResult::unresolved(format!("deletion answer lost: {cause}")),
            }
        }).await
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "one shared durable boundary for the two approved mutation contracts"
    )]
    async fn request_response_write<F, Fut>(
        &self,
        ctx: &ObjectContext<'_>,
        order: &OrderKey,
        external_id: &ExternalId,
        marker: UnresolvedWrite,
        operation: ReplayWrite,
        write: F,
    ) -> Result<WriteResult, HandlerError>
    where
        F: FnOnce(Arc<Gateway>, UnresolvedWrite) -> Fut + Send,
        Fut: Future<Output = WriteResult> + Send,
    {
        let step = operation.step();
        let expected_contract = marker.execution_contract;
        let intent = ctx
            .run(|| async move {
                let mut marker = marker;
                marker.created_at = jiff::Timestamp::now().to_string();
                Ok(journal(OrdinaryIntent::new(marker)))
            })
            .name(format!("prepare-{step}-write"))
            .await
            .map_err(read_fault)?
            .0;
        let marker = &intent.marker;
        let uncertain = |error: TerminalError| {
            Fault::outcome_unknown(
                "replay-enabled Order write interrupted at durable await; marker retained",
            )
            .with_run_cause(&error)
            .about(
                order,
                match marker.operation {
                    WriteOperation::Create { kind, .. } => Some(kind),
                    WriteOperation::Delete { .. } => Some(crate::identity::IssuedKind::Proforma),
                    WriteOperation::Storno { .. } => None,
                },
                external_id,
            )
        };
        self.checkpoint(order, WriteCheckpoint::BeforeMarker).await;
        ctx.set(STATE, Json(intent.clone()));
        self.checkpoint(order, WriteCheckpoint::AfterMarker).await;
        // Awaited acknowledgement of commands preceding the barrier; replay
        // needs no execution-local arm permit to make progress.
        ctx.run(|| async { Ok(()) })
            .name(format!("{step}-marker-committed"))
            .await
            .map_err(uncertain)?;
        let result = ctx
            .run(|| async {
                super::prologue::mark_fresh_work();
                let result = if marker.execution_contract == expected_contract {
                    match self.gateway().await {
                        Ok(gateway) => write(gateway, marker.clone()).await,
                        Err(_) => WriteResult::unresolved(
                            "pinned account unavailable inside ordinary write",
                        ),
                    }
                } else {
                    WriteResult::unresolved(
                        "outbound request differs from retained execution contract",
                    )
                };
                self.checkpoint(order, WriteCheckpoint::Sent).await;
                Ok(journal(result))
            })
            .name(operation.write_name())
            .retry_policy(RunRetryPolicy::new().max_attempts(1))
            .await
            .map_err(uncertain)?
            .0;
        let result = if let WriteResult::Unresolved(original) = result {
            ctx.run(|| async {
                super::prologue::mark_fresh_work();
                let gateway = self.gateway().await.map_err(|_| std::io::Error::other(format!("ordinary write unresolved; original: {}; latest reconciliation: pinned account unavailable", original.reason)))?;
                let result = gateway.reconcile_write_diagnostic(marker, &original).await;
                if let WriteResult::Unresolved(latest) = &result {
                    return Err(std::io::Error::other(format!("ordinary write unresolved; original: {}; latest: {}; reconciliation is read-only", original.reason, latest.reason)).into());
                }
                self.checkpoint(order, WriteCheckpoint::Reconciled).await;
                Ok(journal(result))
            }).name(format!("reconcile-{step}-write")).await.map_err(uncertain)?.0
        } else {
            result
        };
        self.checkpoint(order, WriteCheckpoint::Settled).await;
        ctx.clear(STATE);
        Ok(result)
    }

    pub(super) async fn protected_write<F, Fut>(
        &self,
        ctx: &ObjectContext<'_>,
        order: &OrderKey,
        external_id: &ExternalId,
        operation: WriteOperation,
        name: String,
        write: F,
    ) -> Result<WriteResult, HandlerError>
    where
        F: FnOnce(Arc<Gateway>, UnresolvedWrite) -> Fut + Send,
        Fut: Future<Output = WriteResult> + Send,
    {
        let marker = UnresolvedWrite {
            execution_contract: None,
            proforma_number: None,
            prepayment_number: None,
            version: MarkerVersion,
            token: ctx.invocation_id().to_owned(),
            owner_invocation: ctx.invocation_id().to_owned(),
            created_at: String::new(),
            scope: ctx.scope().map(str::to_owned),
            order: order.clone(),
            namespace: self.config.namespace.clone(),
            external_id: external_id.as_str().to_owned(),
            account_id: self.account.id.to_string(),
            endpoint: self.account.endpoint.as_str().to_owned(),
            credential_ref: self.account.credential_ref.to_string(),
            operation,
        };
        let marker = ctx
            .run(|| async move {
                let mut marker = marker;
                marker.created_at = jiff::Timestamp::now().to_string();
                Ok(journal(marker))
            })
            .name("prepare-write")
            .await
            .map_err(read_fault)?
            .0;
        let uncertain = |error: TerminalError| {
            let kind = match marker.operation {
                WriteOperation::Create { kind, .. } => Some(kind),
                _ => None,
            };
            Fault::outcome_unknown(format!("the Order write ended at a durable await ({}: {}); a send may have landed; the unresolved marker is retained; reconcile before deliberately authorizing another operation", error.code(), if super::support::is_cancelled(&error) { "cancelled" } else { "unconfirmed" }))
                .with_run_cause(&error).about(order, kind, external_id)
        };
        #[cfg(feature = "test-util")]
        self.checkpoint(order, WriteCheckpoint::BeforeMarker).await;
        ctx.set(STATE, Json(marker.clone()));
        #[cfg(feature = "test-util")]
        self.checkpoint(order, WriteCheckpoint::AfterMarker).await;
        let permit = AtomicBool::new(false);
        ctx.run(|| async {
            permit.store(true, Ordering::SeqCst);
            Ok(())
        })
        .name("arm-write")
        .await
        .map_err(uncertain)?;
        #[cfg(feature = "test-util")]
        self.checkpoint(order, WriteCheckpoint::Armed).await;
        let result = ctx
            .run(|| async {
                super::prologue::mark_fresh_work();
                let result = if permit.swap(false, Ordering::SeqCst) {
                    match self.gateway().await {
                        Ok(gateway) => write(gateway, marker.clone()).await,
                        Err(_) => WriteResult::unresolved(
                            "pinned recovery account unavailable before write execution",
                        ),
                    }
                } else {
                    WriteResult::unresolved(
                        "send permission unavailable on replay; reconcile the earlier execution",
                    )
                };
                #[cfg(feature = "test-util")]
                self.checkpoint(order, WriteCheckpoint::Sent).await;
                Ok(journal(result))
            })
            .name(name)
            .retry_policy(RunRetryPolicy::new().max_attempts(1))
            .await
            .map_err(uncertain)?
            .0;
        let result = if let WriteResult::Unresolved(original) = result {
            ctx.run(|| async {
                super::prologue::mark_fresh_work();
                let gateway = self.gateway().await.map_err(|_| std::io::Error::other(format!("Order write unresolved; original: {}; latest reconciliation: pinned recovery account unavailable", original.reason)))?;
                let result = gateway.reconcile_write_diagnostic(&marker, &original).await;
                if let WriteResult::Unresolved(latest) = &result {
                    return Err(std::io::Error::other(format!("Order write unresolved; original: {}; latest reconciliation: {}; reconciliation is read-only; absence does not authorize another send", original.reason, latest.reason)).into());
                }
                #[cfg(feature = "test-util")]
                self.checkpoint(order, WriteCheckpoint::Reconciled).await;
                Ok(journal(result))
            }).name("reconcile-write").await.map_err(uncertain)?.0
        } else {
            result
        };
        #[cfg(feature = "test-util")]
        self.checkpoint(order, WriteCheckpoint::Settled).await;
        ctx.clear(STATE);
        Ok(result)
    }

    #[cfg(not(feature = "test-util"))]
    #[allow(
        clippy::unused_self,
        reason = "same call sites as the feature-gated interruption observer"
    )]
    fn checkpoint(&self, _order: &OrderKey, _point: WriteCheckpoint) -> std::future::Ready<()> {
        std::future::ready(())
    }

    #[cfg(feature = "test-util")]
    async fn checkpoint(&self, order: &OrderKey, point: WriteCheckpoint) {
        if let Some(observer) = &self.write_observer {
            observer.reached(order.as_str(), point).await;
        }
    }
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "map_err consumes the SDK verdict"
)]
fn read_fault(error: TerminalError) -> Fault {
    if super::support::is_cancelled(&error) {
        Fault::cancelled("recovery observation cancelled; unresolved state is retained")
    } else {
        Fault::unavailable(
            "could not read or record recovery evidence; inspect the unresolved marker",
        )
    }
}
