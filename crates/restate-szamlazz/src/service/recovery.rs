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
    MarkerVersion, RecoveryEvidence, RecoveryRequest, RecoveryResponse, UnresolvedObservation,
    UnresolvedWrite, WriteOperation,
};
use crate::gateway::{Gateway, recovery::WriteResult};
use crate::identity::{ExternalId, OrderKey};

const STATE: &str = "unresolved-write";

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
    /// After the write returns, before recording its result.
    Sent,
    /// After a positive reconciliation query, before recording it.
    Reconciled,
    /// After recorded settlement, before clearing the marker.
    Settled,
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

/// Host authorization for recovery, evaluated before marker observation or mutation.
///
/// The host must authenticate the caller at its ingress boundary and strip any
/// caller-supplied identity assertions before passing trusted metadata. Restate
/// request identity authenticates the runtime, not an operator. Internal SDK
/// callers must meet the same policy. No authorizer is installed by default.
pub trait RecoveryAuthorizer: Send + Sync {
    /// Return the authenticated operator's audit identity, or deny access.
    fn authorize(
        &self,
        scope: Option<&str>,
        order: &str,
        headers: &restate_sdk::context::HeaderMap,
    ) -> Option<String>;
}

impl Order {
    /// Install test-only interruption control for this endpoint.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub fn with_write_observer(mut self, observer: Arc<dyn WriteObserver>) -> Self {
        self.write_observer = Some(observer);
        self
    }
    /// Enable operator recovery through a host-provided authorization boundary.
    #[must_use]
    pub fn with_recovery_authorizer(mut self, authorizer: Arc<dyn RecoveryAuthorizer>) -> Self {
        self.recovery_authorizer = Some(authorizer);
        self
    }

    pub(super) fn authorize_recovery(
        &self,
        scope: Option<&str>,
        key: &str,
        headers: &restate_sdk::context::HeaderMap,
    ) -> Option<String> {
        self.recovery_authorizer
            .as_ref()
            .and_then(|auth| auth.authorize(scope, key, headers))
            .filter(|operator| !operator.trim().is_empty())
    }

    pub(super) async fn observe_marker(
        &self,
        ctx: &SharedObjectContext<'_>,
    ) -> Result<UnresolvedObservation, HandlerError> {
        let _operator = ctx
            .run(|| async {
                Ok(journal(self.authorize_recovery(
                    ctx.scope(),
                    ctx.key(),
                    ctx.headers(),
                )))
            })
            .name("authorize-recovery")
            .await
            .map_err(read_fault)?
            .0
            .ok_or_else(forbidden)?;
        let state = ctx.get::<bytes::Bytes>(STATE).await.map_err(read_fault)?;
        Ok(match state {
            None => UnresolvedObservation::Absent,
            Some(raw) => match decode_marker(&raw, ctx.scope(), ctx.key()) {
                Some(marker) => UnresolvedObservation::Unresolved {
                    marker: Box::new(marker),
                },
                None => UnresolvedObservation::Unreadable,
            },
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

    async fn recover_marker_inner(
        &self,
        ctx: &ObjectContext<'_>,
        request: RecoveryRequest,
    ) -> Result<RecoveryResponse, HandlerError> {
        let operator = ctx
            .run(|| async {
                Ok(journal(self.authorize_recovery(
                    ctx.scope(),
                    ctx.key(),
                    ctx.headers(),
                )))
            })
            .name("authorize-recovery")
            .await
            .map_err(read_fault)?
            .0
            .ok_or_else(forbidden)?;
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
        let mut account = Account::new(marker.account_id.clone(), marker.credential_ref.clone());
        account.endpoint = marker
            .endpoint
            .parse()
            .map_err(|_| Fault::invalid_input("invalid pinned endpoint"))?;
        let config = crate::config::WorkerConfig {
            namespace: marker.namespace.clone(),
            ..self.parts.config.clone().into_inner()
        };
        let execution = Execution::new(account, self.parts.accounts.clone(), config);
        let evidence = request.evidence;
        match &evidence {
            RecoveryEvidence::NotExecuted {
                audit_reference, ..
            } if audit_reference.trim().is_empty() => {
                return Err(Fault::invalid_input(
                    "an audited non-execution attestation requires an audit reference",
                )
                .into());
            }
            RecoveryEvidence::NotExecuted { .. } => {}
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
                if matches!(result.0, WriteResult::Unresolved) {
                    return Err(Fault::outcome_unknown("document evidence does not settle the exact unresolved write; marker retained").into());
                }
            }
        }
        let receipt = RecoveryResponse {
            token: marker.token,
            operator,
            evidence: serde_json::to_value(evidence)
                .map_err(|_| Fault::unavailable("could not encode recovery evidence"))?,
        };
        let receipt = ctx
            .run(|| async { Ok(journal(receipt)) })
            .name("record-recovery")
            .await
            .map_err(read_fault)?;
        ctx.clear(STATE);
        Ok(receipt.0)
    }
}

fn decode_marker(raw: &[u8], scope: Option<&str>, key: &str) -> Option<UnresolvedWrite> {
    let marker: UnresolvedWrite = serde_json::from_slice(raw).ok()?;
    if marker.scope.as_deref() != scope
        || marker.order.as_str() != key
        || marker.token.is_empty()
        || marker.token != marker.owner_invocation
        || marker.account_id.is_empty()
        || marker.credential_ref.is_empty()
        || marker.endpoint.parse::<crate::account::Endpoint>().is_err()
    {
        return None;
    }
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
            number.parse::<crate::identity::InvoiceNumber>().ok()?;
            ExternalId::for_storno(&marker.namespace, &marker.order, number)
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
                        Err(_) => WriteResult::Unresolved,
                    }
                } else {
                    WriteResult::Unresolved
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
        let result = if matches!(result, WriteResult::Unresolved) {
            ctx.run(|| async {
                super::prologue::mark_fresh_work();
                let gateway = self.gateway().await.map_err(|_| std::io::Error::other("pinned recovery account unavailable; write remains unresolved"))?;
                let result = gateway.reconcile_write(&marker, None).await;
                if matches!(result, WriteResult::Unresolved) {
                    return Err(std::io::Error::other("Order write unresolved; reconciliation is read-only; absence does not authorize another send").into());
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

fn forbidden() -> Fault {
    Fault::new(
        crate::contract::TerminalCode::Forbidden,
        "operator recovery access denied",
    )
}
