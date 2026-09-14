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

/// A distinct serialized run result as well as state discriminator. Run names
/// alone are not an exceptional-replay fence: an old prepare-write result must
/// fail decoding rather than become ordinary resend permission.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(
    not(feature = "test-util"),
    allow(
        dead_code,
        reason = "registered privacy-scanned experimental journal type"
    )
)]
pub(super) struct OrdinaryIntent {
    execution_contract: OrdinaryContract,
    #[serde(flatten)]
    marker: UnresolvedWrite,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
enum OrdinaryContract {
    #[serde(rename = "request_response_ordinary_v1")]
    RequestResponseOrdinaryV1,
}

impl OrdinaryIntent {
    #[cfg_attr(
        not(feature = "test-util"),
        allow(dead_code, reason = "experimental constructor")
    )]
    pub(super) fn new(marker: UnresolvedWrite) -> Self {
        Self {
            execution_contract: OrdinaryContract::RequestResponseOrdinaryV1,
            marker,
        }
    }
}

/// Interruption boundaries exposed only for real-runtime protocol tests.
#[cfg(feature = "test-util")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteCheckpoint {
    /// Before the marker command.
    BeforeMarker,
    /// After the marker command, before acknowledged arming.
    AfterMarker,
    /// After acknowledged arming, before consuming permission.
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
    RecoveryVerified,
    /// After recorded recovery evidence, before clearing its marker.
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
                    UnresolvedObservation::Unresolved {
                        marker: Box::new(marker),
                    }
                } else {
                    #[cfg(feature = "test-util")]
                    if self.experimental_request_response
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
    ) -> Result<RecoveryResponse, HandlerError> {
        let sdk_span = tracing::Span::current();
        let span =
            super::prologue::execution_span(ctx.scope(), Some(ctx.key()), ctx.invocation_id());
        super::prologue::SDK_SPAN
            .scope(
                sdk_span,
                self.recover_marker_inner(ctx, request).instrument(span),
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
    ) -> Result<RecoveryResponse, HandlerError> {
        let raw = ctx
            .get::<bytes::Bytes>(STATE)
            .await
            .map_err(read_fault)?
            .ok_or_else(|| Fault::invalid_input("no unresolved marker matches this recovery"))?;
        let marker = decode_marker(&raw, ctx.scope(), ctx.key()).ok_or_else(|| {
            Fault::outcome_unknown("unreadable unresolved marker; use a compatible deployment")
        })?;
        if marker != request.marker {
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
    /// Separate admission/run identity for the isolated ordinary experiment.
    /// Every non-positive result after the barrier retains earlier uncertainty.
    #[cfg(feature = "test-util")]
    pub(super) async fn ordinary_request_response(
        &self,
        ctx: &ObjectContext<'_>,
        order: &OrderKey,
        request: crate::gateway::CreateStepRequest<'_>,
        external_id: &ExternalId,
    ) -> Result<WriteResult, HandlerError> {
        let marker = UnresolvedWrite {
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
        let intent = ctx
            .run(|| async move {
                let mut marker = marker;
                marker.created_at = jiff::Timestamp::now().to_string();
                Ok(journal(OrdinaryIntent::new(marker)))
            })
            .name("prepare-ordinary-write")
            .await
            .map_err(read_fault)?
            .0;
        let marker = &intent.marker;
        let uncertain = |error: TerminalError| {
            Fault::outcome_unknown(
                "experimental ordinary write interrupted at durable await; marker retained",
            )
            .with_run_cause(&error)
            .about(
                order,
                Some(crate::identity::IssuedKind::Invoice),
                external_id,
            )
        };
        self.checkpoint(order, WriteCheckpoint::BeforeMarker).await;
        ctx.set(STATE, Json(intent.clone()));
        self.checkpoint(order, WriteCheckpoint::AfterMarker).await;
        // Awaited acknowledgement of commands preceding the barrier; replay
        // needs no execution-local arm permit to make progress.
        ctx.run(|| async { Ok(()) })
            .name("ordinary-marker-committed")
            .await
            .map_err(uncertain)?;
        let result = ctx
            .run(|| async {
                super::prologue::mark_fresh_work();
                let result = match self.gateway().await {
                    Ok(gateway) => {
                        gateway
                            .ordinary_request_response(request, || {
                                self.checkpoint(order, WriteCheckpoint::OrdinaryGuardsPassed)
                            })
                            .await
                    }
                    Err(_) => {
                        WriteResult::unresolved("pinned account unavailable inside ordinary write")
                    }
                };
                self.checkpoint(order, WriteCheckpoint::Sent).await;
                Ok(journal(result))
            })
            .name("create-ordinary-request-response")
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
            }).name("reconcile-ordinary-write").await.map_err(uncertain)?.0
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
