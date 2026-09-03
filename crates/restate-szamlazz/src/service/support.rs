//! Plumbing shared by the `Szamlazz.Order` and `Szamlazz.Agent` handlers: the
//! fault → `TerminalError` mapping, ledger state access, journaled runs and
//! the validation of documents found by a query.

use std::time::Duration;

use jiff::Timestamp;
use restate_sdk::errors::{HandlerError, TerminalError};
use rust_decimal::Decimal;
use serde::Serialize;

use crate::config::Config;
use crate::contract::{IssuedKind, RequestId, TerminalCode};
use crate::identity::OrderKey;
use crate::ledger::{Ledger, LedgerError};
use crate::steps::{FoundDocument, document_type_of};

/// The Virtual Object state key holding the [`Ledger`].
pub(super) const LEDGER_KEY: &str = "ledger";

/// A fault raised as a `TerminalError` (design §8): never a domain outcome.
///
/// Serialised as the error message so that the ingress body carries the
/// [`TerminalCode`] token and the identity of the request it is about.
#[derive(Debug, Clone, Serialize)]
pub(super) struct Fault {
    code: TerminalCode,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    order: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<IssuedKind>,
    #[serde(rename = "gen", skip_serializing_if = "Option::is_none")]
    generation: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    external_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_id: Option<RequestId>,
}

impl Fault {
    pub(super) fn new(code: TerminalCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            order: None,
            kind: None,
            generation: None,
            external_id: None,
            request_id: None,
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
        kind: IssuedKind,
        generation: u32,
        external_id: impl Into<String>,
        request_id: Option<&RequestId>,
    ) -> Self {
        self.order = Some(order.as_str().to_owned());
        self.kind = Some(kind);
        self.generation = Some(generation);
        self.external_id = Some(external_id.into());
        self.request_id = request_id.cloned();
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

/// A ledger precondition failure surfacing from a handler is a programming
/// error in the dispatch logic (the handler checked the status first), except
/// for the operator handlers, which map it to `invalid_input` explicitly.
impl From<LedgerError> for Fault {
    fn from(error: LedgerError) -> Self {
        match error {
            LedgerError::SupplierMismatch { recorded, seen } => Self::account_mismatch(format!(
                "document belongs to supplier {seen}, the ledger records supplier {recorded}"
            )),
            other => Self::invalid_input(other.to_string()),
        }
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

/// The remainder of `backoff` since `last`, or zero.
pub(super) fn remaining_backoff(backoff: Duration, last: Timestamp, now: Timestamp) -> Duration {
    let elapsed = Duration::try_from(now.duration_since(last)).unwrap_or(Duration::ZERO);
    backoff.saturating_sub(elapsed)
}

/// The next backoff of a doubling schedule capped at `max`.
pub(super) fn next_backoff(current: Duration, max: Duration) -> Duration {
    current.saturating_mul(2).min(max)
}

/// Why a document found under our external id is not ours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mismatch {
    /// `rendelesszam` differs from the order key.
    Order,
    /// `tipus` is not the kind's document type.
    Kind,
    /// `teszt` differs from the account mode.
    Test,
    /// `szallito/id` differs from the expected supplier id.
    Supplier,
}

/// Validates a document found by a query against the identity it should have
/// (design §3): same order, the kind's `tipus`, the account's `teszt` and,
/// when both are known, the supplier id.
pub(super) fn validate_found(
    found: &FoundDocument,
    order: &OrderKey,
    kind: IssuedKind,
    config: &Config,
    supplier_id: Option<u64>,
) -> Result<(), Mismatch> {
    if found.order_number.as_deref().map(str::trim) != Some(order.as_str()) {
        return Err(Mismatch::Order);
    }
    if found.document_type != document_type_of(kind) {
        return Err(Mismatch::Kind);
    }
    if found.test != config.account.mode.is_test() {
        return Err(Mismatch::Test);
    }
    if let (Some(expected), Some(seen)) = (supplier_id, found.supplier_id)
        && expected != seen
    {
        return Err(Mismatch::Supplier);
    }
    Ok(())
}

/// `gross − Σ payments`, when the gross total is known.
pub(super) fn outstanding(gross: Option<Decimal>, payments: &[Decimal]) -> Option<Decimal> {
    gross.map(|gross| gross - payments.iter().copied().sum::<Decimal>())
}

/// Records the supplier id a query reported; a different recorded id is an
/// `account_mismatch` fault.
pub(super) fn learn_supplier(ledger: &mut Ledger, found: &FoundDocument) -> Result<(), Fault> {
    match found.supplier_id {
        Some(id) => ledger.learn_supplier_id(id).map_err(Fault::from),
        None => Ok(()),
    }
}

/// The supplier id to expect on a found document: the configured pin or the
/// one the ledger learned.
pub(super) fn expected_supplier(config: &Config, ledger: &Ledger) -> Option<u64> {
    config.account.supplier_id.or(ledger.supplier_id())
}

/// What the order-number hint says about a proforma szamlazz.hu no longer
/// returns by number (design §6 step 2, proforma kind).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ProformaFate {
    /// An `SZ` or `ES` referencing the proforma exists: it was consumed.
    Consumed(Box<FoundDocument>),
    /// Nothing references it: it was deleted.
    Deleted,
}

/// The context-bound helpers, stamped out per Restate context type.
///
/// They are deliberately not generic over the SDK's sealed context traits: a
/// generic `async fn` over them trips rust-lang/rust#100013 (`Send` is not
/// general enough) inside the macro-generated dispatcher.
macro_rules! journal_helpers {
    ($module:ident, $ctx:ident, $($state:tt)*) => {
        #[allow(
            dead_code,
            unused_imports,
            reason = "each context type uses a different subset of the helpers"
        )]
        pub(in crate::service) mod $module {
            use std::future::Future;
            use std::sync::Arc;
            use std::time::Duration;

            use jiff::Timestamp;
            use restate_sdk::context::{
                ContextReadState as _, ContextSideEffects as _, ContextTimers as _,
                ContextWriteState as _, RunFuture as _, RunRetryPolicy,
            };
            use restate_sdk::errors::HandlerError;
            use restate_sdk::prelude::$ctx;
            use restate_sdk::serde::Json;
            use serde::Serialize;
            use serde::de::DeserializeOwned;

            use super::{Fault, LEDGER_KEY, ProformaFate};
            use crate::contract::Selector;
            use crate::identity::{ExternalId, OrderKey};
            use crate::ledger::Ledger;
            use crate::steps::{QueryOutcome, Steps};

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

            /// The journaled current time: deterministic on replay.
            pub(in crate::service) async fn now(ctx: &$ctx<'_>) -> Result<Timestamp, HandlerError> {
                run_once(ctx, "now", || async { Timestamp::now() }).await
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
                run_once(ctx, name, move || async move { steps.verify(&number).await }).await
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
                run_once(ctx, name, move || async move { steps.query(&selector).await }).await
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

            /// Disambiguates a code 7 on proforma `number` via the
            /// order-number hint.
            pub(in crate::service) async fn proforma_fate(
                ctx: &$ctx<'_>,
                steps: &Arc<Steps>,
                order: &OrderKey,
                number: &str,
            ) -> Result<ProformaFate, HandlerError> {
                match hint(ctx, steps, format!("hint-proforma-{number}"), order).await? {
                    QueryOutcome::Found(found)
                        if matches!(found.document_type.as_str(), "SZ" | "ES")
                            && found.referenced_proforma.as_deref() == Some(number) =>
                    {
                        Ok(ProformaFate::Consumed(Box::new(found)))
                    }
                    QueryOutcome::Found(_) | QueryOutcome::NotFound => Ok(ProformaFate::Deleted),
                    QueryOutcome::Transport(message) => Err(Fault::unavailable(message).into()),
                }
            }

            journal_helpers!(@state $ctx $($state)*);
        }
    };
    (@state $ctx:ident read) => {
        /// Loads the ledger, or an empty one when the object has no state
        /// yet.
        pub(in crate::service) async fn load(ctx: &$ctx<'_>) -> Result<Ledger, HandlerError> {
            Ok(ctx
                .get::<Json<Ledger>>(LEDGER_KEY)
                .await?
                .map_or_else(Ledger::new, Json::into_inner))
        }
    };
    (@state $ctx:ident read_write) => {
        journal_helpers!(@state $ctx read);

        /// Writes the ledger. Called immediately after every mutation that
        /// must survive a crash.
        pub(in crate::service) fn save(ctx: &$ctx<'_>, ledger: &Ledger) {
            ctx.set(LEDGER_KEY, Json(ledger.clone()));
        }
    };
    (@state $ctx:ident none) => {};
}

journal_helpers!(object, ObjectContext, read_write);
journal_helpers!(shared, SharedObjectContext, read);
journal_helpers!(service, Context, none);
