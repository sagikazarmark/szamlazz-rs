//! Plumbing shared by the `Szamlazz.Order` and `Szamlazz.Agent` handlers: the
//! fault → `TerminalError` mapping, journaled runs, the validation of
//! documents found under our external ids and the account check of documents
//! found by number.

use restate_sdk::errors::{HandlerError, TerminalError};
use serde::Serialize;
use serde::de::DeserializeOwned;
use szamlazz_agent::Date;
use szamlazz_agent::ops::query_xml::InvoiceDocument;

use super::prologue::Resolution;
use crate::account::Account;
use crate::config::Namespace;
use crate::contract::{IssuedKind, StornoOutcome, StornoResponse, TerminalCode};
use crate::gateway::{
    CreateOutcome, DeleteOutcome, InvoiceDocumentExt as _, LookupOutcome, ProbeOutcome,
    QueryOutcome, SetPaymentsOutcome, StornoLookupOutcome, StornoOutcome as GatewayStornoOutcome,
    TaxpayerOutcome,
};
use crate::identity::{ExternalId, OrderKey};

/// A type the services journal as the result of a `ctx.run` — the bound of
/// `run_once`, `run_retrying` and `run_reading`, so this list is exactly what
/// the journal can hold.
///
/// Implementing it is a promise that the type's serde layout is
/// **additive-only**, as the [`gateway`](crate::gateway) module docs state
/// (ADR 0005, journal compatibility). The promise is checked by the fixtures
/// under `tests/journal/<type>/` (`service::journal`), one per variant: a new
/// implementor is pinned there before it is journaled, and a new variant of
/// one of these enums fails to compile until its sample is listed.
pub(super) trait Journaled: Serialize + DeserializeOwned {}

impl Journaled for Namespace {}
impl Journaled for Resolution {}
impl Journaled for QueryOutcome {}
impl Journaled for LookupOutcome {}
impl Journaled for CreateOutcome {}
impl Journaled for StornoLookupOutcome {}
impl Journaled for GatewayStornoOutcome {}
impl Journaled for DeleteOutcome {}
impl Journaled for SetPaymentsOutcome {}
impl Journaled for ProbeOutcome {}
impl Journaled for TaxpayerOutcome {}

/// A fault raised as a `TerminalError` (design §7): never a domain outcome.
///
/// Serialised as the error message so that the ingress body carries the
/// [`TerminalCode`] token, the szamlazz.hu code when szamlazz.hu's answer is
/// what the fault is about, and the identity of the document it is about.
/// `code` is always a `TerminalCode` token; a szamlazz.hu code never travels
/// in it.
#[derive(Debug, Clone, Serialize)]
pub(super) struct Fault {
    code: TerminalCode,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    szamlazz_code: Option<String>,
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
            szamlazz_code: None,
            order: None,
            kind: None,
            external_id: None,
        }
    }

    /// The same fault carrying the szamlazz.hu code its answer had.
    fn answered_with(mut self, szamlazz_code: impl Into<String>) -> Self {
        self.szamlazz_code = Some(szamlazz_code.into());
        self
    }

    pub(super) fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(TerminalCode::InvalidInput, message)
    }

    /// The document the request names by number is not known to szamlazz.hu
    /// (code 7). Nothing was sent.
    pub(super) fn not_found(message: impl Into<String>) -> Self {
        Self::new(TerminalCode::NotFound, message)
    }

    /// szamlazz.hu answered with an error code the handler passes through
    /// rather than concludes from: the `szamlazz_error` fault (422) with the
    /// code in `szamlazz_code` and a message that repeats szamlazz.hu's.
    pub(super) fn szamlazz_error(code: impl Into<String>, message: impl Into<String>) -> Self {
        let code = code.into();
        Self::new(
            TerminalCode::SzamlazzError,
            format!("szamlazz.hu error {code}: {}", message.into()),
        )
        .answered_with(code)
    }

    pub(super) fn unavailable(message: impl Into<String>) -> Self {
        Self::new(TerminalCode::Unavailable, message)
    }

    /// szamlazz.hu answered a read with a code the handler cannot conclude a
    /// document from (neither 7 nor a credential code). An answer, so it is
    /// journaled and never retried by the read policy; still a fault, since
    /// nothing may be concluded from it.
    pub(super) fn inconclusive_answer(code: impl Into<String>, message: impl Into<String>) -> Self {
        let code = code.into();
        Self::unavailable(format!(
            "szamlazz.hu answered the query with code {code}: {}; nothing may be concluded — retry with a new Idempotency-Key or read get",
            message.into()
        ))
        .answered_with(code)
    }

    /// The verified original of a storno carries no `telj` (ADR 0007).
    /// szamlazz.hu's query schema has the element mandatory — the legal "no
    /// separate date" case is an equal `telj`, never an absent one — so this
    /// is szamlazz.hu breaking its own schema: the same class as an
    /// inconclusive answer, and answered the same way. The storno must repeat
    /// that date and no default can be right, so nothing is sent.
    pub(super) fn missing_fulfillment_date(number: &str) -> Self {
        Self::unavailable(format!(
            "szamlazz.hu returned invoice {number} without a fulfillment date (telj), which the storno must repeat; nothing was sent — retry with a new Idempotency-Key, or query the invoice"
        ))
    }

    pub(super) fn outcome_unknown(message: impl Into<String>) -> Self {
        Self::new(TerminalCode::OutcomeUnknown, message)
    }

    pub(super) fn account_mismatch(message: impl Into<String>) -> Self {
        Self::new(TerminalCode::AccountMismatch, message)
    }

    /// The request names no account of this deployment (unscoped where
    /// accounts are scoped, or an unknown scope).
    pub(super) fn unknown_account(message: impl Into<String>) -> Self {
        Self::new(TerminalCode::UnknownAccount, message)
    }

    /// szamlazz.hu rejected the account's agent credentials with `code`
    /// (3, 135, 136 or 164). Logs the warning that pages the operator — tagged
    /// with the namespace and the code, never the key — and builds the fault.
    /// The attempt that observed the code issued nothing: szamlazz.hu answers
    /// these codes before acting on a request.
    pub(super) fn credentials_rejected(
        namespace: &Namespace,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        let code = code.into();
        let message = message.into();
        tracing::warn!(
            namespace = %namespace,
            code = %code,
            "szamlazz.hu rejected the agent credentials; fix the account's agent key"
        );
        Self::new(
            TerminalCode::CredentialsRejected,
            format!(
                "szamlazz.hu rejected the agent credentials (code {code}: {message}); this attempt issued nothing — fix the account's agent key, then retry with a new Idempotency-Key or read get"
            ),
        )
        .answered_with(code)
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

    /// The HTTP status the ingress reports for the fault: the code's.
    const fn status(&self) -> u16 {
        self.code.status()
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

/// The fault of a read step that ended without an answer: the read policy is
/// exhausted (500, carrying the last `Unanswered`'s message) or the
/// invocation was cancelled (409). The caller attaches the document it was
/// reading about when it knows one.
pub(super) fn read_exhausted(step: &str, error: &TerminalError) -> Fault {
    Fault::unavailable(format!(
        "the {step} read ended without an answer from szamlazz.hu ({}): {}; retry with a new Idempotency-Key or read get",
        error.code(),
        error.message()
    ))
}

/// Parses the Virtual Object key as an [`OrderKey`].
///
/// The key must arrive trimmed (design §3). Restate's per-key lock is on the
/// *raw* key, so `ORD-1` and ` ORD-1` would be two instances with two locks
/// that map to one szamlazz.hu order and identical external ids — two
/// concurrent creates under them would both pass their lookup and both send,
/// leaving szamlazz.hu's order-number-repetition toggle as the only guard. A
/// key whose trimmed form differs from the raw one is therefore refused as
/// `invalid_input` naming the rule; [`OrderKey::parse`] itself stays lenient
/// for the places that parse an order number rather than a key.
pub(super) fn order_key(key: &str) -> Result<OrderKey, Fault> {
    if key.trim() != key {
        return Err(Fault::invalid_input(format!(
            "invalid order key {key:?}: the order key must not have leading or trailing whitespace — Restate locks on the raw key, so trim it before calling"
        )));
    }
    OrderKey::parse(key)
        .map_err(|error| Fault::invalid_input(format!("invalid order key: {error}")))
}

/// The document a verify by number found, or the fault for anything else:
/// 404 `not_found` naming the invoice on code 7, `unavailable` on a code the
/// verify cannot conclude from (`Fault::inconclusive_answer`), a credential
/// code as `credentials_rejected`. Shared by every verify — `Szamlazz.Order`'s
/// attach the order identity to the fault ([`Fault::about`]),
/// `Szamlazz.Agent.storno`'s carries none.
///
/// # Errors
///
/// The fault for every outcome but `Found`.
pub(super) fn verified_document(
    outcome: QueryOutcome,
    number: &str,
    namespace: &Namespace,
) -> Result<Box<InvoiceDocument>, Fault> {
    match outcome {
        QueryOutcome::Found(found) => Ok(found),
        QueryOutcome::NotFound => Err(Fault::not_found(format!(
            "invoice {number} is not known to szamlazz.hu (code 7)"
        ))),
        QueryOutcome::Api { code, message } => Err(Fault::inconclusive_answer(code, message)),
        QueryOutcome::CredentialsRejected { code, message } => {
            Err(Fault::credentials_rejected(namespace, code, message))
        }
    }
}

/// The account pins of a document found by number: it must belong to the
/// account the invocation resolved to (design §3) — `teszt` equal to the
/// account's mode and, when the account pins a supplier id and the document
/// carries one, `szallito/id` equal to it. Every handler that finds a document
/// runs this check — `Szamlazz.Order` on its verifies, `Szamlazz.Agent.query`
/// and `storno` on what they find — so a misconfigured account (a test account
/// configured as live, a wrong supplier id) fails loudly on its first found
/// document instead of acting on the wrong account. Two handlers are exempt:
/// `Szamlazz.Agent.set_payments` sends without a query, and a credit entry is
/// not a legal document; `Szamlazz.Agent.query_taxpayer` finds no document
/// at all — a taxpayer record is NAV's, not the account's, and carries no
/// pins. Not to be confused with the `check_account` probe, which finds
/// nothing and echoes configuration.
///
/// # Errors
///
/// The `account_mismatch` fault (409), naming the document and the observed
/// and expected pins. No document carries the agent key, so neither does the
/// message.
pub(super) fn check_pins(account: &Account, found: &InvoiceDocument) -> Result<(), Fault> {
    if found.account_matches(account.mode.is_test(), account.supplier_id) {
        Ok(())
    } else {
        Err(Fault::account_mismatch(format!(
            "document {} belongs to another szamlazz.hu account: it carries teszt = {}, supplier {:?}; the resolved account expects teszt = {}, supplier {:?}",
            found.number(),
            found.info.test,
            found.supplier.id,
            account.mode.is_test(),
            account.supplier_id,
        )))
    }
}

/// What the storno step sends (design §6 step 3), built from what the verify
/// step found. Shared by `Szamlazz.Order.storno_invoice` and
/// `Szamlazz.Agent.storno`, whose storno external ids differ.
#[derive(Debug, Clone)]
pub(super) struct StornoIntent {
    /// The invoice to reverse.
    pub(super) number: String,
    /// `{namespace}:{order}:storno:{number}` or
    /// `{namespace}:by-number:{number}:storno`.
    pub(super) storno_id: ExternalId,
    pub(super) comment: Option<String>,
    /// The verified document's `eszamla` when known, else the account
    /// default — an open code set for which the account's own default is a
    /// legitimate choice.
    pub(super) e_invoice: bool,
    /// The verified document's `telj`, which the storno repeats as its
    /// `teljesitesDatum` (ADR 0007): a fiscal fact of the document for which
    /// no default can be right, so it is never defaulted.
    pub(super) fulfillment_date: Date,
}

impl StornoIntent {
    /// The intent for reversing the verified `found` — `number`, as the
    /// caller named it — under `storno_id`: `e_invoice` lifted from the
    /// document with `account`'s default as fallback, `fulfillment_date` the
    /// document's own `telj`. A pure function of the journaled verify result,
    /// so every execution rebuilds the same request.
    ///
    /// # Errors
    ///
    /// [`Fault::missing_fulfillment_date`] when the document carries no
    /// `telj`; the callers raise it after every answer that needs no send.
    pub(super) fn from_verified(
        found: &InvoiceDocument,
        account: &Account,
        number: String,
        storno_id: ExternalId,
        comment: Option<String>,
    ) -> Result<Self, Fault> {
        let fulfillment_date = found
            .info
            .fulfillment_date
            .ok_or_else(|| Fault::missing_fulfillment_date(&number))?;
        Ok(Self {
            e_invoice: found.e_invoice().unwrap_or(account.defaults.e_invoice),
            number,
            storno_id,
            comment,
            fulfillment_date,
        })
    }
}

/// The settled storno step as the handlers' `StornoResponse`: reversed (now
/// or already), not stornoable, or rejected. Rejected credentials are the
/// caller's fault to raise, with the identity it knows.
pub(super) fn storno_response(
    outcome: GatewayStornoOutcome,
    number: String,
) -> Result<StornoResponse, (String, String)> {
    Ok(match outcome {
        GatewayStornoOutcome::Reversed(storno) => {
            StornoResponse::new(StornoOutcome::Reversed, number)
                .with_storno_number(storno.invoice_number.as_str())
        }
        GatewayStornoOutcome::AlreadyReversed { storno_number } => {
            StornoResponse::new(StornoOutcome::Reversed, number)
                .with_storno_number(storno_number)
        }
        GatewayStornoOutcome::NotStornoable => StornoResponse::new(StornoOutcome::Rejected, number)
            .with_code("not_stornoable")
            .with_message(
                "szamlazz.hu echoed the document unchanged: it cannot be reversed (only invoices can be stornoed)",
            ),
        GatewayStornoOutcome::Rejected { code, message } => {
            StornoResponse::new(StornoOutcome::Rejected, number)
                .with_code(code)
                .with_message(message)
        }
        GatewayStornoOutcome::CredentialsRejected { code, message } => return Err((code, message)),
    })
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
    /// Classifies an answered query: another szamlazz.hu code is
    /// `unavailable` (nothing may be concluded), rejected credentials are
    /// `credentials_rejected`. (A query szamlazz.hu did not answer never
    /// reaches here: it is the read's `Unanswered`, retried by the read
    /// policy.)
    pub(super) fn classify(
        outcome: QueryOutcome,
        namespace: &Namespace,
        order: &OrderKey,
        kind: IssuedKind,
        expect_test: bool,
        expect_supplier_id: Option<u64>,
    ) -> Result<Self, Fault> {
        match outcome {
            QueryOutcome::NotFound => Ok(Self::Absent),
            QueryOutcome::Api { code, message } => Err(Fault::inconclusive_answer(code, message)),
            QueryOutcome::CredentialsRejected { code, message } => {
                Err(Fault::credentials_rejected(namespace, code, message))
            }
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
            use std::error::Error as StdError;
            use std::future::Future;
            use std::sync::Arc;

            use restate_sdk::context::{ContextSideEffects as _, RunFuture as _, RunRetryPolicy};
            use restate_sdk::errors::{HandlerError, TerminalError};
            use restate_sdk::prelude::$ctx;
            use restate_sdk::serde::Json;
            use super::{Fault, Journaled, Lookup, StornoIntent};
            use crate::account::Accounts;
            use crate::config::WorkerConfig;
            use crate::contract::{IssuedKind, Selector};
            use crate::gateway::{
                InvoiceDocumentExt as _, QueryOutcome, StornoLookupOutcome,
                StornoOutcome as GatewayStornoOutcome, StornoStepRequest, Unanswered,
            };
            use crate::identity::{ExternalId, OrderKey};
            use crate::service::prologue::{self as decisions, Execution};

            /// The prologue of every handler (design §4): pin → resolve →
            /// fetch → open.
            ///
            /// 1. **Pin** the namespace in a pure durable step (`namespace`):
            ///    a redeploy with a changed namespace cannot make a running
            ///    invocation issue under a new id.
            /// 2. **Resolve** the request's scope to its account in a durable
            ///    step named `account` under the resolve policy: unscoped and
            ///    unknown are journaled as data and become the terminal
            ///    `unknown_account`; an unavailable resolver is retryable and
            ///    journals nothing; exhaustion is `unavailable`.
            /// 3. **Fetch** the account's credentials outside the journal —
            ///    on every execution, including replays — with a short
            ///    in-process retry, then terminal `unavailable`.
            /// 4. **Open** the gateway for this execution over a fresh client.
            pub(in crate::service) async fn prologue(
                ctx: &$ctx<'_>,
                accounts: &Accounts,
                config: &WorkerConfig,
            ) -> Result<Execution, HandlerError> {
                // 1. Pin.
                let pinned = {
                    let namespace = config.namespace.clone();
                    run_once(ctx, "namespace", move || async move { namespace }).await?
                };
                let config = WorkerConfig {
                    namespace: pinned,
                    ..config.clone()
                };

                // 2. Resolve.
                let scope = ctx.scope().map(str::to_owned);
                let resolution = {
                    let accounts = accounts.clone();
                    run_retrying(
                        ctx,
                        "account",
                        config.resolve.run_retry_policy(),
                        move || async move {
                            decisions::resolution(accounts.resolve(scope.as_deref()).await)
                        },
                    )
                    .await
                    .map_err(|error| decisions::resolve_exhausted(&error))?
                };
                let account = decisions::account_of(resolution)?;

                // 3. Fetch, outside the journal.
                let credentials = decisions::fetch_credentials(accounts, &account).await?;

                // 4. Open.
                let gateway = decisions::open(account, credentials)?;
                Ok(Execution { gateway, config })
            }

            /// Journals the result of `f` under `name`, executing it at most
            /// once per journal entry (`RunRetryPolicy::max_attempts(1)`):
            /// the pure `namespace` pin and the write steps that have no
            /// retry of their own (`delete-proforma-*`, `set-payments-*`)
            /// return every outcome as data, so a closure failure is a bug,
            /// not a retry. Reads go through [`run_reading`].
            pub(in crate::service) async fn run_once<'ctx, T, F, Fut>(
                ctx: &$ctx<'ctx>,
                name: impl Into<String>,
                f: F,
            ) -> Result<T, HandlerError>
            where
                F: FnOnce() -> Fut + Send + 'ctx,
                Fut: Future<Output = T> + Send + 'ctx,
                T: Journaled + Send + 'static,
            {
                let Json(value) = ctx
                    .run(|| async move { Ok(Json(f().await)) })
                    .name(name)
                    .retry_policy(RunRetryPolicy::new().max_attempts(1))
                    .await?;
                Ok(value)
            }

            /// Journals the result of `f` under `name`, re-executing it under
            /// `policy` while it fails with `E` — the step's own "not
            /// settled" error, which the SDK treats as retryable. The whole
            /// handler replays to this entry after the policy's delay, so the
            /// closure begins again from its first line.
            ///
            /// # Errors
            ///
            /// The `TerminalError` the run ends with: exhaustion of the
            /// policy (500, carrying the last `E`'s message) or cancellation
            /// (409). The caller decides what it means.
            pub(in crate::service) async fn run_retrying<'ctx, T, E, F, Fut>(
                ctx: &$ctx<'ctx>,
                name: impl Into<String>,
                policy: RunRetryPolicy,
                f: F,
            ) -> Result<T, TerminalError>
            where
                F: FnOnce() -> Fut + Send + 'ctx,
                Fut: Future<Output = Result<T, E>> + Send + 'ctx,
                T: Journaled + Send + 'static,
                E: StdError + Send + Sync + 'static,
            {
                let Json(value) = ctx
                    .run(|| async move { Ok(Json(f().await?)) })
                    .name(name)
                    .retry_policy(policy)
                    .await?;
                Ok(value)
            }

            /// A read-only durable step under the read policy: journals the
            /// answer of `f` under `name`, re-executing it while szamlazz.hu
            /// does not answer (`Unanswered`). Every answer is data; a read
            /// writes nothing, so a re-executed closure's answer is exactly as
            /// fresh as a first one.
            ///
            /// # Errors
            ///
            /// The `unavailable` fault of a read that ended without an answer
            /// — the read policy exhausted or the invocation cancelled —
            /// naming the step and the last failure. The caller attaches the
            /// document when it knows one.
            pub(in crate::service) async fn run_reading<'ctx, T, F, Fut>(
                ctx: &$ctx<'ctx>,
                name: impl Into<String>,
                exec: &Execution,
                f: F,
            ) -> Result<T, Fault>
            where
                F: FnOnce() -> Fut + Send + 'ctx,
                Fut: Future<Output = Result<T, Unanswered>> + Send + 'ctx,
                T: Journaled + Send + 'static,
            {
                let name = name.into();
                run_retrying(ctx, name.clone(), exec.config.read.run_retry_policy(), f)
                    .await
                    .map_err(|error| super::read_exhausted(&name, &error))
            }

            /// Journaled query of document `number` (a verify), under the read
            /// policy.
            pub(in crate::service) async fn verify(
                ctx: &$ctx<'_>,
                exec: &Execution,
                name: impl Into<String>,
                number: &str,
            ) -> Result<QueryOutcome, Fault> {
                let gateway = Arc::clone(&exec.gateway);
                let number = number.to_owned();
                run_reading(ctx, name, exec, move || async move {
                    gateway.verify(&number).await
                })
                .await
            }

            /// Journaled query by external id, under the read policy.
            pub(in crate::service) async fn query_external_id(
                ctx: &$ctx<'_>,
                exec: &Execution,
                name: impl Into<String>,
                external_id: &ExternalId,
            ) -> Result<QueryOutcome, Fault> {
                let gateway = Arc::clone(&exec.gateway);
                let selector = Selector::ExternalId(external_id.as_str().to_owned());
                run_reading(ctx, name, exec, move || async move {
                    gateway.query(&selector).await
                })
                .await
            }

            /// Journaled query by one of our external ids, under the read
            /// policy, validated against the identity the document should
            /// have (design §3) and the gateway's account. A fault carries
            /// that identity.
            pub(in crate::service) async fn lookup(
                ctx: &$ctx<'_>,
                exec: &Execution,
                name: impl Into<String>,
                external_id: &ExternalId,
                order: &OrderKey,
                kind: IssuedKind,
            ) -> Result<Lookup, Fault> {
                let about = |fault: Fault| fault.about(order, Some(kind), external_id.as_str());
                let outcome = query_external_id(ctx, exec, name, external_id)
                    .await
                    .map_err(about)?;
                let account = exec.gateway.account();
                Lookup::classify(
                    outcome,
                    &exec.config.namespace,
                    order,
                    kind,
                    account.mode.is_test(),
                    account.supplier_id,
                )
                .map_err(about)
            }

            /// Journaled order-number hint, under the read policy.
            pub(in crate::service) async fn hint(
                ctx: &$ctx<'_>,
                exec: &Execution,
                name: impl Into<String>,
                order: &OrderKey,
            ) -> Result<QueryOutcome, Fault> {
                let gateway = Arc::clone(&exec.gateway);
                let order = order.clone();
                run_reading(ctx, name, exec, move || async move {
                    gateway.hint(&order).await
                })
                .await
            }

            /// The storno lookup step (design §6 step 2): one read-only
            /// journaled query of the storno external id, under the read
            /// policy.
            pub(in crate::service) async fn lookup_storno(
                ctx: &$ctx<'_>,
                exec: &Execution,
                intent: &StornoIntent,
            ) -> Result<StornoLookupOutcome, Fault> {
                let gateway = Arc::clone(&exec.gateway);
                let external_id = intent.storno_id.clone();
                let number = intent.number.clone();
                run_reading(
                    ctx,
                    format!("lookup-storno-{number}"),
                    exec,
                    move || async move { gateway.lookup_storno(&external_id, &number).await },
                )
                .await
            }

            /// The storno step (design §6 step 3): one durable step under the
            /// issue policy's run retry policy, query-first on every execution
            /// (the query is inside the closure: a separate journaled query
            /// would replay its stale "nothing" on the retry and re-send).
            /// The request is rebuilt from the intent on every execution —
            /// the date included — so every send is byte-identical.
            ///
            /// # Errors
            ///
            /// The `TerminalError` the run ends with — exhaustion (500) or
            /// cancellation (409); the caller maps it to `outcome_unknown`
            /// about its document. Nothing is recorded: the next call's
            /// lookup finds whatever landed.
            pub(in crate::service) async fn storno_step(
                ctx: &$ctx<'_>,
                exec: &Execution,
                intent: &StornoIntent,
            ) -> Result<GatewayStornoOutcome, TerminalError> {
                let gateway = Arc::clone(&exec.gateway);
                let number = intent.number.clone();
                let external_id = intent.storno_id.clone();
                let comment = intent.comment.clone();
                let e_invoice = intent.e_invoice;
                let fulfillment_date = intent.fulfillment_date;
                run_retrying(
                    ctx,
                    format!("storno-{}", intent.number),
                    exec.config.issue.run_retry_policy(),
                    move || async move {
                        gateway
                            .storno(StornoStepRequest {
                                invoice_number: &number,
                                external_id: &external_id,
                                comment: comment.as_deref(),
                                e_invoice,
                                fulfillment_date,
                            })
                            .await
                    },
                )
                .await
            }

            /// The storno number of a reversed document, when the order-number
            /// hint is the `SS` referencing it. Best effort: a hint that is
            /// not that `SS` — or that the read policy could not get answered
            /// — is `None`, since the handler's answer (`reversed`) is already
            /// known; only rejected credentials are a fault.
            pub(in crate::service) async fn storno_number_of(
                ctx: &$ctx<'_>,
                exec: &Execution,
                order: &OrderKey,
                number: &str,
            ) -> Result<Option<String>, Fault> {
                let outcome = match hint(ctx, exec, format!("hint-storno-{number}"), order).await
                {
                    Ok(outcome) => outcome,
                    Err(fault) => {
                        tracing::warn!(
                            number,
                            fault = %fault.message,
                            "the storno number could not be read; reporting the reversal without it"
                        );
                        return Ok(None);
                    }
                };
                Ok(match outcome {
                    QueryOutcome::Found(found) if found.is_storno_of(number) => {
                        Some(found.number().to_owned())
                    }
                    QueryOutcome::CredentialsRejected { code, message } => {
                        return Err(Fault::credentials_rejected(
                            &exec.config.namespace,
                            code,
                            message,
                        ));
                    }
                    QueryOutcome::Found(_) | QueryOutcome::NotFound | QueryOutcome::Api { .. } => {
                        None
                    }
                })
            }
        }
    };
}

journal_helpers!(object, ObjectContext);
journal_helpers!(shared, SharedObjectContext);
journal_helpers!(service, Context);
