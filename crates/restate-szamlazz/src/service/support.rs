//! Plumbing shared by the `Szamlazz.Order` and `Szamlazz.Agent` handlers: the
//! fault → `TerminalError` mapping, journaled runs and the validation of
//! documents found under our external ids.

use std::time::Duration;

use restate_sdk::errors::{HandlerError, TerminalError};
use serde::Serialize;
use szamlazz_agent::ops::query_xml::InvoiceDocument;

use crate::contract::{IssuedKind, TerminalCode};
use crate::identity::OrderKey;
use crate::steps::{InvoiceDocumentExt as _, QueryOutcome};

/// A fault raised as a `TerminalError` (design §7): never a domain outcome.
///
/// Serialised as the error message so that the ingress body carries the
/// [`TerminalCode`] token and the identity of the document it is about.
#[derive(Debug, Clone, Serialize)]
pub(super) struct Fault {
    code: TerminalCode,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    order: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<IssuedKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    external_id: Option<String>,
}

impl Fault {
    pub(super) fn new(code: TerminalCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            order: None,
            kind: None,
            external_id: None,
        }
    }

    pub(super) fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(TerminalCode::InvalidInput, message)
    }

    pub(super) fn unavailable(message: impl Into<String>) -> Self {
        Self::new(TerminalCode::Unavailable, message)
    }

    pub(super) fn outcome_unknown(message: impl Into<String>) -> Self {
        Self::new(TerminalCode::OutcomeUnknown, message)
    }

    pub(super) fn account_mismatch(message: impl Into<String>) -> Self {
        Self::new(TerminalCode::AccountMismatch, message)
    }

    /// Attaches the identity of the document the fault is about.
    pub(super) fn about(
        mut self,
        order: &OrderKey,
        kind: Option<IssuedKind>,
        external_id: impl Into<String>,
    ) -> Self {
        self.order = Some(order.as_str().to_owned());
        self.kind = kind;
        self.external_id = Some(external_id.into());
        self
    }

    /// The HTTP status the ingress reports for the fault.
    const fn status(&self) -> u16 {
        match self.code {
            TerminalCode::InvalidInput => 400,
            TerminalCode::AccountMismatch => 409,
            TerminalCode::OutcomeUnknown => 500,
            TerminalCode::Unavailable => 503,
        }
    }
}

impl From<Fault> for TerminalError {
    fn from(fault: Fault) -> Self {
        let body = serde_json::to_string(&fault)
            .unwrap_or_else(|_| format!("{{\"code\":\"{}\"}}", fault.code));
        Self::new_with_code(fault.status(), body)
    }
}

impl From<Fault> for HandlerError {
    fn from(fault: Fault) -> Self {
        TerminalError::from(fault).into()
    }
}

/// A `TerminalError` with a plain HTTP status and message (the `Szamlazz.Agent`
/// service's not-found and rejection errors).
pub(super) fn terminal(status: u16, code: &str, message: impl Into<String>) -> HandlerError {
    let body = serde_json::json!({ "code": code, "message": message.into() });
    TerminalError::new_with_code(status, body.to_string()).into()
}

/// Parses the Virtual Object key as an [`OrderKey`].
pub(super) fn order_key(key: &str) -> Result<OrderKey, Fault> {
    OrderKey::parse(key)
        .map_err(|error| Fault::invalid_input(format!("invalid order key: {error}")))
}

/// The next backoff of a doubling schedule capped at `max`.
pub(super) fn next_backoff(current: Duration, max: Duration) -> Duration {
    current.saturating_mul(2).min(max)
}

/// What a query by one of our external ids found (design §3).
///
/// Every caller matches all three variants: an issuing handler refuses a
/// [`Lookup::Collision`] as `conflict{external_id_collision}` (the newest
/// holder may hide a document of ours), `delete_proforma` answers
/// `not_deleted{external_id_collision}`, and only `get` — a read that must not
/// fail — reports the slot as absent.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Lookup {
    /// szamlazz.hu holds nothing under the id (code 7).
    Absent,
    /// A document that passed validation: ours, live or reversed.
    Ours(Box<InvoiceDocument>),
    /// A document that fails validation: another order, kind, account mode or
    /// supplier. Never trusted.
    Collision(Box<InvoiceDocument>),
}

impl Lookup {
    /// Classifies a query outcome; a failed check is `unavailable`.
    pub(super) fn classify(
        outcome: QueryOutcome,
        order: &OrderKey,
        kind: IssuedKind,
        expect_test: bool,
        expect_supplier_id: Option<u64>,
    ) -> Result<Self, Fault> {
        match outcome {
            QueryOutcome::NotFound => Ok(Self::Absent),
            QueryOutcome::Transport(message) => Err(Fault::unavailable(message)),
            QueryOutcome::Found(found) => {
                if found.is_ours(order, kind, expect_test, expect_supplier_id) {
                    Ok(Self::Ours(found))
                } else {
                    tracing::warn!(number = %found.number(), kind = %kind, "external id collision");
                    Ok(Self::Collision(found))
                }
            }
        }
    }
}

/// The context-bound helpers, stamped out per Restate context type.
///
/// They are deliberately not generic over the SDK's sealed context traits: a
/// generic `async fn` over them trips rust-lang/rust#100013 (`Send` is not
/// general enough) inside the macro-generated dispatcher.
macro_rules! journal_helpers {
    ($module:ident, $ctx:ident) => {
        #[allow(
            dead_code,
            unused_imports,
            reason = "each context type uses a different subset of the helpers"
        )]
        pub(in crate::service) mod $module {
            use std::future::Future;
            use std::sync::Arc;
            use std::time::Duration;

            use restate_sdk::context::{
                ContextSideEffects as _, ContextTimers as _, RunFuture as _, RunRetryPolicy,
            };
            use restate_sdk::errors::HandlerError;
            use restate_sdk::prelude::$ctx;
            use restate_sdk::serde::Json;
            use serde::Serialize;
            use serde::de::DeserializeOwned;

            use super::Lookup;
            use crate::contract::{IssuedKind, Selector};
            use crate::identity::{ExternalId, OrderKey};
            use crate::steps::{InvoiceDocumentExt as _, QueryOutcome, Steps};

            /// Journals the result of `f` under `name`, executing it at most
            /// once per journal entry (`RunRetryPolicy::max_attempts(1)`):
            /// every szamlazz.hu call returns its outcome as data, so a
            /// closure failure is a bug, not a retry.
            pub(in crate::service) async fn run_once<'ctx, T, F, Fut>(
                ctx: &$ctx<'ctx>,
                name: impl Into<String>,
                f: F,
            ) -> Result<T, HandlerError>
            where
                F: FnOnce() -> Fut + Send + 'ctx,
                Fut: Future<Output = T> + Send + 'ctx,
                T: Serialize + DeserializeOwned + Send + 'static,
            {
                let Json(value) = ctx
                    .run(|| async move { Ok(Json(f().await)) })
                    .name(name)
                    .retry_policy(RunRetryPolicy::new().max_attempts(1))
                    .await?;
                Ok(value)
            }

            /// Durable sleep; a zero duration is skipped.
            pub(in crate::service) async fn sleep(
                ctx: &$ctx<'_>,
                duration: Duration,
            ) -> Result<(), HandlerError> {
                if !duration.is_zero() {
                    ctx.sleep(duration).await?;
                }
                Ok(())
            }

            /// Journaled query of document `number` (a verify).
            pub(in crate::service) async fn verify(
                ctx: &$ctx<'_>,
                steps: &Arc<Steps>,
                name: impl Into<String>,
                number: &str,
            ) -> Result<QueryOutcome, HandlerError> {
                let steps = Arc::clone(steps);
                let number = number.to_owned();
                run_once(
                    ctx,
                    name,
                    move || async move { steps.verify(&number).await },
                )
                .await
            }

            /// Journaled query by external id.
            pub(in crate::service) async fn query_external_id(
                ctx: &$ctx<'_>,
                steps: &Arc<Steps>,
                name: impl Into<String>,
                external_id: &ExternalId,
            ) -> Result<QueryOutcome, HandlerError> {
                let steps = Arc::clone(steps);
                let selector = Selector::ExternalId(external_id.as_str().to_owned());
                run_once(
                    ctx,
                    name,
                    move || async move { steps.query(&selector).await },
                )
                .await
            }

            /// Journaled query by one of our external ids, validated against
            /// the identity the document should have (design §3).
            pub(in crate::service) async fn lookup(
                ctx: &$ctx<'_>,
                steps: &Arc<Steps>,
                name: impl Into<String>,
                external_id: &ExternalId,
                order: &OrderKey,
                kind: IssuedKind,
            ) -> Result<Lookup, HandlerError> {
                let outcome = query_external_id(ctx, steps, name, external_id).await?;
                let account = &steps.config().account;
                Ok(Lookup::classify(
                    outcome,
                    order,
                    kind,
                    account.mode.is_test(),
                    account.supplier_id,
                )?)
            }

            /// Journaled order-number hint.
            pub(in crate::service) async fn hint(
                ctx: &$ctx<'_>,
                steps: &Arc<Steps>,
                name: impl Into<String>,
                order: &OrderKey,
            ) -> Result<QueryOutcome, HandlerError> {
                let steps = Arc::clone(steps);
                let order = order.clone();
                run_once(ctx, name, move || async move { steps.hint(&order).await }).await
            }

            /// The storno number of a reversed document, when the order-number
            /// hint is the `SS` referencing it.
            pub(in crate::service) async fn storno_number_of(
                ctx: &$ctx<'_>,
                steps: &Arc<Steps>,
                order: &OrderKey,
                number: &str,
            ) -> Result<Option<String>, HandlerError> {
                Ok(
                    match hint(ctx, steps, format!("hint-storno-{number}"), order).await? {
                        QueryOutcome::Found(found) if found.is_storno_of(number) => {
                            Some(found.number().to_owned())
                        }
                        _ => None,
                    },
                )
            }
        }
    };
}

journal_helpers!(object, ObjectContext);
journal_helpers!(shared, SharedObjectContext);
journal_helpers!(service, Context);
