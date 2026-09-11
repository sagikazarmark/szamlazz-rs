//! SDK-independent request and response contract of the `Szamlazz.Order` and
//! `Szamlazz.Agent` services.
//!
//! Everything here is plain data with a stable JSON shape: domain outcomes are
//! returned as values with HTTP 200 (see [`CreateOutcome`] and [`ConflictReason`]),
//! while the [`TerminalCode`]s are reserved for faults. Three of the eight
//! known codes mean "outcome unknown: reconcile before deliberately renewing"
//! (`outcome_unknown`,
//! `unavailable`, `credentials_rejected`); the other known codes are settled: the same
//! request never succeeds, or szamlazz.hu's own answer is passed through
//! ([`TerminalCode`] says which). Cancellation is independent: a read reports
//! `cancelled`, a write `outcome_unknown` with [`FaultCause::Cancelled`]. Neither
//! authorizes automatic retry. The module depends on
//! [`identity`](crate::identity) alone (one way: `identity` imports nothing
//! of it) and compiles without `restate-sdk`; with the `schemars` feature the
//! types also derive JSON Schemas. Faults are outside the success-output
//! discovery schema; callers use [`crate::service::decode_fault`].
//!
//! Every request type refuses a field it does not know
//! (`#[serde(deny_unknown_fields)]`, `additionalProperties: false` in the
//! schema): a misspelt `reissue` or `additive` is an error naming the field,
//! never a silent default. Response types stay open: a client must tolerate
//! fields added later. Scalar response outcomes, reasons, warnings and fault
//! codes also preserve unknown strings; an unknown fault
//! code has no inferred HTTP status or outcome classification.
//! Tagged response states (`DocumentState`, `CredentialsCheck`) preserve
//! unknown state strings together with their payload fields.
//!
//! The submodules mirror the handler modules of [`service`](crate::service),
//! one per handler family, each holding its requests beside its responses:
//!
//! - [`document`]: the per-call document input (buyer, line items, payment
//!   method, overrides) and its conversion to `szamlazz_agent` types; shared
//!   by every issuing handler.
//! - [`create`]: the issuing handlers: `create_proforma`, `create_invoice`,
//!   `create_prepayment`, `create_final` and `correct_invoice`.
//! - [`storno`]: `storno_invoice`, `delete_proforma` and the `get` live view
//!   ([`OrderStatus`]); the storno contract is shared with
//!   `Szamlazz.Agent.storno`.
//! - [`agent`]: the rest of `Szamlazz.Agent`: `query`, `query_taxpayer`,
//!   `set_credit_entries` and `check_account`.
//!
//! **Handler names** are the Rust method names, verbatim, as the Rust SDK
//! exposes them (`snake_case`; no `#[handler(name = …)]` override anywhere).
//! `Szamlazz.Order`'s name the document, since the key is the order and a
//! handler picks one of its documents (`create_invoice`, `storno_invoice`,
//! `delete_proforma`; `get` alone reads them all); `Szamlazz.Agent`'s name
//! the operation alone where the document is the input (`query`, `storno`:
//! the number is in the body) and keep the object where it is not
//! (`query_taxpayer`, `set_credit_entries`, `check_account`). A handler name is
//! part of the Restate registration, so a rename is a breaking change
//! (`set_payments` became `set_credit_entries` in one, 2026-09-09; `CONTEXT.md`,
//! *Credit entry*). **JSON**: fields `snake_case`; requests closed
//! (`deny_unknown_fields`), responses open (`#[non_exhaustive]`); decimals
//! as strings (a number accepted on input), dates as ISO `YYYY-MM-DD`; an
//! absent optional response field is `null`, a fault's included.

use std::fmt;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

pub mod agent;
pub mod create;
pub mod document;
pub mod recovery;
pub mod storno;

pub use agent::{
    CheckAccountResponse, CheckedAccount, CredentialsCheck, CreditEntryInput, CreditEntryRecord,
    InvalidTaxNumber, QueryRequest, QueryResponse, QueryTaxpayerRequest, QueryTaxpayerResponse,
    Selector, SetCreditEntriesRequest, SetCreditEntriesResponse, TaxpayerAddress,
};
pub use create::{
    ConflictReason, CorrectRequest, CreateOptions, CreateOutcome, CreateRequest, CreateResponse,
    ProformaLink, Reissue, Warning,
};
pub use document::{
    BuyerInput, DocumentInput, DocumentOverrides, ExchangeRateInput, LineItemInput, PaymentMethod,
    PostalAddressInput, TaxpayerStatus,
};
pub use storno::{
    DeleteProformaRequest, DeleteProformaResponse, DeleteReason, DocumentState, DocumentStatus,
    OrderStatus, StornoOutcome, StornoRequest, StornoResponse,
};

/// The caller-supplied identities the requests carry, defined in
/// [`identity`](crate::identity) because each is a segment of the external
/// id: re-exported here as part of the contract.
pub use crate::identity::{
    CorrectionId, DocumentKind, InvalidCorrectionId, InvalidInvoiceNumber, InvoiceNumber,
    IssuedKind,
};

use crate::identity::{ExternalId, OrderKey};

/// The code of a `TerminalError` any handler of either service may raise.
///
/// Every fault either service raises carries one of these tokens in `code`,
/// with the HTTP status of [`status`](Self::status). Three of them mean
/// "outcome unknown: reconcile before deliberately renewing":
/// `outcome_unknown`, `unavailable` and
/// `credentials_rejected` ([`is_outcome_unknown`](Self::is_outcome_unknown)
/// names exactly them). The other known codes are settled: the same request
/// never succeeds (`invalid_input`, `unknown_account`, `not_found`) or
/// szamlazz.hu's own answer is passed through
/// (`szamlazz_error`, whose szamlazz.hu code travels in the fault's separate
/// `szamlazz_code` field). Unknown codes are preserved without assigning an
/// HTTP status or telling the caller whether the outcome is unknown.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TerminalCode {
    /// The host denied operator recovery access. HTTP 403.
    Forbidden,
    /// Intentional cancellation during a read or account resolution. HTTP 409.
    /// No write was sent; cancellation does not authorize automatic retry.
    Cancelled,
    /// An external write remains uncertain, or an unresolved marker blocks a
    /// later Order mutation. Also covers write cancellation and unmanaged Agent
    /// storno exhausting its issue policy. Settle earlier work before renewal;
    /// an empty query does not authorize another send. HTTP 500.
    OutcomeUnknown,
    /// szamlazz.hu did not answer a read-only step through every execution
    /// the read policy allows (or answered it with a code nothing can be
    /// concluded from), or the account resolver or credential store could not
    /// answer. Nothing was sent by the execution that raised it. HTTP 503.
    Unavailable,
    /// The caller's request: a malformed body, an untrimmed order key, a tax
    /// number in neither accepted form, an option the handler does not take,
    /// or a request the wire contract cannot carry (a sixth credit entry).
    /// The same request never succeeds. HTTP 400.
    InvalidInput,
    /// szamlazz.hu rejected the account's agent credentials (codes 3, 135,
    /// 136, 164): the worker's configuration is wrong, not the request. The
    /// request that drew the code was not acted on (szamlazz.hu answers
    /// these codes before acting), but the code may have come to a post-send
    /// re-query, and an earlier execution may have landed with a lost reply,
    /// which is why this is a fault and not a `rejected` outcome. Fix the
    /// key, then reconcile any earlier write before renewal. HTTP 503.
    CredentialsRejected,
    /// The request names no account of this deployment: it arrived unscoped
    /// where accounts are reachable by scope only, or under a scope no account
    /// is reachable by. Raised before anything is issued; the same request
    /// never succeeds, so the caller must fix the scope, not retry. HTTP 400.
    UnknownAccount,
    /// The document the request names by number is not known to szamlazz.hu
    /// (code 7): `Szamlazz.Agent.query`'s selector, the invoice of
    /// `Szamlazz.Agent.storno` / `Szamlazz.Order.storno_invoice`, the base of
    /// `correct_invoice`. Nothing was sent; the same request never succeeds.
    /// HTTP 404.
    NotFound,
    /// szamlazz.hu answered the request with an error code of its own that
    /// the handler passes through rather than concludes from: on
    /// `Szamlazz.Agent.query`, `query_taxpayer` (szamlazz.hu's code or NAV's
    /// relayed one). Credit-entry vendor refusals retain `outcome_unknown`
    /// because an earlier execution may have registered the entries. The
    /// szamlazz.hu code is in the fault's `szamlazz_code`, the message is
    /// szamlazz.hu's. HTTP 422.
    SzamlazzError,
    /// A token this version does not know, preserved without classification.
    Other(String),
}

impl TerminalCode {
    /// Known codes, including intentional read cancellation.
    pub const KNOWN: [Self; 9] = [
        Self::Forbidden,
        Self::Cancelled,
        Self::InvalidInput,
        Self::UnknownAccount,
        Self::NotFound,
        Self::SzamlazzError,
        Self::OutcomeUnknown,
        Self::Unavailable,
        Self::CredentialsRejected,
    ];

    /// The wire token, preserved verbatim for an unknown code.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Forbidden => "forbidden",
            Self::Cancelled => "cancelled",
            Self::OutcomeUnknown => "outcome_unknown",
            Self::Unavailable => "unavailable",
            Self::InvalidInput => "invalid_input",
            Self::CredentialsRejected => "credentials_rejected",
            Self::UnknownAccount => "unknown_account",
            Self::NotFound => "not_found",
            Self::SzamlazzError => "szamlazz_error",
            Self::Other(token) => token,
        }
    }

    /// Whether the fault means "outcome unknown": the caller reconciles before
    /// renewing the operation, and pages rather
    /// than auto-retries. Exactly three codes do: `outcome_unknown` (the
    /// write step ran out of its policy), `unavailable` (szamlazz.hu, the
    /// account resolver or the credential store did not answer) and
    /// `credentials_rejected` (the key is wrong, and an earlier execution may
    /// have landed). Check [`Fault::is_cancelled`] separately: cancellation
    /// requires reconciliation before deliberately renewing a write, never an
    /// automatic retry. `cancelled` itself is a read cancellation and returns
    /// `Some(false)`. The other four are settled: retrying the same request
    /// repeats the answer, so the caller fixes the request, the number, the
    /// scope or the account, or (for `szamlazz_error`) sends again later with
    /// a new key. An unknown code returns `None`: this version cannot
    /// classify it and gives no retry advice for it.
    ///
    /// ```
    /// use restate_szamlazz::contract::TerminalCode;
    ///
    /// let outcome_unknown: Vec<TerminalCode> = TerminalCode::KNOWN
    ///     .into_iter()
    ///     .filter(|code| code.is_outcome_unknown() == Some(true))
    ///     .collect();
    /// assert_eq!(
    ///     outcome_unknown,
    ///     [
    ///         TerminalCode::OutcomeUnknown,
    ///         TerminalCode::Unavailable,
    ///         TerminalCode::CredentialsRejected,
    ///     ]
    /// );
    /// // The settled four: the same request never succeeds as it is.
    /// for code in [
    ///     TerminalCode::InvalidInput,
    ///     TerminalCode::UnknownAccount,
    ///     TerminalCode::NotFound,
    ///     TerminalCode::SzamlazzError,
    /// ] {
    ///     assert_eq!(code.is_outcome_unknown(), Some(false));
    /// }
    /// ```
    #[must_use]
    pub const fn is_outcome_unknown(&self) -> Option<bool> {
        match self {
            Self::OutcomeUnknown | Self::Unavailable | Self::CredentialsRejected => Some(true),
            Self::Forbidden
            | Self::Cancelled
            | Self::InvalidInput
            | Self::UnknownAccount
            | Self::NotFound
            | Self::SzamlazzError => Some(false),
            Self::Other(_) => None,
        }
    }

    /// The HTTP status the ingress reports for a known code; `None` for an
    /// unknown code. Read the actual ingress status for a newer fault.
    #[must_use]
    pub const fn status(&self) -> Option<u16> {
        match self {
            Self::Forbidden => Some(403),
            // The caller's request: the same request never succeeds.
            Self::InvalidInput | Self::UnknownAccount => Some(400),
            Self::NotFound => Some(404),
            Self::Cancelled => Some(409),
            // szamlazz.hu's own answer, passed through.
            Self::SzamlazzError => Some(422),
            Self::OutcomeUnknown => Some(500),
            // The worker's misconfiguration or szamlazz.hu not answering, not
            // the caller's request: the same request succeeds once the key
            // is fixed or szamlazz.hu answers, so neither a 4xx ("do not
            // retry") nor 401/403 ("you are unauthenticated") fits.
            Self::Unavailable | Self::CredentialsRejected => Some(503),
            Self::Other(_) => None,
        }
    }
}

impl fmt::Display for TerminalCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<String> for TerminalCode {
    fn from(token: String) -> Self {
        match token.as_str() {
            "forbidden" => Self::Forbidden,
            "cancelled" => Self::Cancelled,
            "outcome_unknown" => Self::OutcomeUnknown,
            "unavailable" => Self::Unavailable,
            "invalid_input" => Self::InvalidInput,
            "credentials_rejected" => Self::CredentialsRejected,
            "unknown_account" => Self::UnknownAccount,
            "not_found" => Self::NotFound,
            "szamlazz_error" => Self::SzamlazzError,
            _ => Self::Other(token),
        }
    }
}

impl From<TerminalCode> for String {
    fn from(code: TerminalCode) -> Self {
        match code {
            TerminalCode::Other(token) => token,
            known => known.as_str().to_owned(),
        }
    }
}

impl Serialize for TerminalCode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TerminalCode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from(String::deserialize(deserializer)?))
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for TerminalCode {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TerminalCode".into()
    }

    fn schema_id() -> std::borrow::Cow<'static, str> {
        concat!(module_path!(), "::TerminalCode").into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "The worker fault code. Known values: forbidden (403), cancelled (409), invalid_input (400), unknown_account (400), not_found (404), szamlazz_error (422), outcome_unknown (500), unavailable (503), credentials_rejected (503). The last three mean the outcome is unknown. Cancellation does not authorize automatic retry. Other strings are preserved without an inferred HTTP status or outcome classification.",
        })
    }
}

/// A machine-readable cause, independent of the fault's outcome classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub enum FaultCause {
    /// The invocation was intentionally cancelled; an external write may still
    /// have landed. Reconcile before deliberately renewing the operation.
    Cancelled,
    /// An unfamiliar cause, preserved without invented recovery advice.
    Other(String),
}

impl FaultCause {
    /// The causes this release understands.
    pub const KNOWN: [Self; 1] = [Self::Cancelled];

    /// The cause's wire token.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Cancelled => "cancelled",
            Self::Other(token) => token,
        }
    }
}

impl From<String> for FaultCause {
    fn from(token: String) -> Self {
        match token.as_str() {
            "cancelled" => Self::Cancelled,
            _ => Self::Other(token),
        }
    }
}

impl From<FaultCause> for String {
    fn from(cause: FaultCause) -> Self {
        match cause {
            FaultCause::Cancelled => "cancelled".to_owned(),
            FaultCause::Other(token) => token,
        }
    }
}

/// The body of a fault: what a `TerminalError` either service raises carries,
/// as the caller receives it inside Restate's ingress envelope.
///
/// `code` is always a [`TerminalCode`] token (a szamlazz.hu code never
/// travels in it); `szamlazz_code` is the szamlazz.hu code when szamlazz.hu's
/// answer is what the fault is about; `order`, `kind` and `external_id`
/// identify the document the fault is about when the handler knows one (the
/// `Szamlazz.Order` handlers' faults; a by-number fault of `Szamlazz.Agent`
/// carries none). For known codes, [`TerminalCode::status`] gives the HTTP
/// status the ingress reports; for an unknown code, read the ingress status.
///
/// On the wire the fault is the JSON **string** inside Restate's ingress
/// envelope: `{"code": <HTTP status>, "message": "<fault JSON>", "source":
/// "invocation"}` under `x-restate-error-source: invocation` (server 1.7.8;
/// the SDK carries a terminal error as a code and a message and offers no
/// other channel), so a caller parses `message` a second time, into this
/// type. A response type: open (a client tolerates fields added later) and
/// `#[non_exhaustive]`, built with [`Fault::new`] and the setters; like every
/// response type's, its optional fields are present as `null` when absent.
///
/// Conversion to the SDK's `TerminalError` is fallible (`TryFrom<Fault>`):
/// an unknown code returns `service::FaultConversionError::UnknownStatus`
/// rather than acquiring a made-up HTTP status. Conversion to `HandlerError`
/// keeps known faults terminal and treats a conversion failure as an internal
/// retryable error, not a terminal fault or retry advice for the decoded code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct Fault {
    /// The fault's code; unknown tokens are preserved for newer faults.
    pub code: TerminalCode,
    /// What happened and what the caller does next, in prose.
    pub message: String,
    /// Why the operation ended, when separately known. A cancelled write keeps
    /// `outcome_unknown` and carries `cancelled` here. Older faults omit it.
    #[serde(default)]
    pub cause: Option<FaultCause>,
    /// The szamlazz.hu code, when szamlazz.hu's answer is what the fault is
    /// about (`szamlazz_error` always; `credentials_rejected`; `unavailable`
    /// on an inconclusive code).
    #[serde(default)]
    pub szamlazz_code: Option<String>,
    /// The order the fault is about (the `Szamlazz.Order` key), when known.
    #[serde(default)]
    pub order: Option<String>,
    /// The kind of the document the fault is about, when known.
    #[serde(default)]
    pub kind: Option<IssuedKind>,
    /// The external id of the document the fault is about, when known.
    #[serde(default)]
    pub external_id: Option<String>,
}

impl Fault {
    /// A fault of `code` with `message` and nothing else.
    pub fn new(code: TerminalCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            cause: None,
            szamlazz_code: None,
            order: None,
            kind: None,
            external_id: None,
        }
    }

    /// The same fault carrying a machine-readable cause.
    #[must_use]
    pub fn with_cause(mut self, cause: FaultCause) -> Self {
        self.cause = Some(cause);
        self
    }

    /// Whether this fault reports intentional cancellation. This is separate
    /// from [`TerminalCode::is_outcome_unknown`]: a cancelled write is both.
    /// Unknown codes or causes return `None`; no prose is inspected. An older
    /// fault without a cause makes no machine-readable cancellation claim.
    #[must_use]
    pub const fn is_cancelled(&self) -> Option<bool> {
        if matches!(self.code, TerminalCode::Other(_))
            || matches!(self.cause, Some(FaultCause::Other(_)))
        {
            return None;
        }
        Some(
            matches!(self.code, TerminalCode::Cancelled)
                || matches!(self.cause, Some(FaultCause::Cancelled)),
        )
    }

    /// The same fault carrying the szamlazz.hu code its answer had.
    #[must_use]
    pub fn with_szamlazz_code(mut self, szamlazz_code: impl Into<String>) -> Self {
        self.szamlazz_code = Some(szamlazz_code.into());
        self
    }

    /// The same fault about the document identified by `order` (the
    /// `Szamlazz.Order` key), `kind` and `external_id`.
    #[must_use]
    pub fn about(
        mut self,
        order: &OrderKey,
        kind: Option<IssuedKind>,
        external_id: &ExternalId,
    ) -> Self {
        self.order = Some(order.as_str().to_owned());
        self.kind = kind;
        self.external_id = Some(external_id.as_str().to_owned());
        self
    }

    /// The HTTP status of a known fault code, or `None` for an unknown code.
    #[must_use]
    pub const fn status(&self) -> Option<u16> {
        self.code.status()
    }
}

/// `gross − Σ credit entries`, when the gross total is known and the arithmetic
/// fits a decimal. The one definition of the outstanding amount both
/// `create_*` and `query` report. Checked: the amounts are szamlazz.hu's, but
/// a panic would run on the SDK's connection task.
pub(crate) fn outstanding(gross: Option<Decimal>, credit_entries: &[Decimal]) -> Option<Decimal> {
    let paid = credit_entries
        .iter()
        .try_fold(Decimal::ZERO, |sum, amount| sum.checked_add(*amount))?;
    gross?.checked_sub(paid)
}

#[cfg(test)]
mod tests {
    use rust_decimal::dec;

    use super::*;

    #[test]
    fn outstanding_needs_a_gross_total() {
        assert_eq!(outstanding(None, &[dec!(1)]), None);
        assert_eq!(outstanding(Some(dec!(100)), &[]), Some(dec!(100)));
        assert_eq!(
            outstanding(Some(dec!(100)), &[dec!(30), dec!(80)]),
            Some(dec!(-10))
        );
    }

    /// The amounts come from szamlazz.hu, but a sum that does not fit a
    /// decimal is `None` (unknown), never a panic on the connection task.
    #[test]
    fn outstanding_that_does_not_fit_a_decimal_is_unknown_not_a_panic() {
        assert_eq!(
            outstanding(Some(dec!(1)), &[Decimal::MAX, Decimal::MAX]),
            None
        );
        assert_eq!(outstanding(Some(Decimal::MAX), &[Decimal::MIN]), None);
    }

    /// The bound reaches every request that names a document by number, and
    /// it is applied where the body is decoded: a refused number is a
    /// *Malformed body*, `invalid_input` before the Prologue with serde's
    /// message naming the rule.
    #[test]
    fn every_by_number_request_refuses_an_invoice_number_outside_the_bound() {
        fn refused<T: serde::de::DeserializeOwned + std::fmt::Debug>(
            name: &str,
            body: serde_json::Value,
        ) {
            let error = serde_json::from_value::<T>(body)
                .expect_err(name)
                .to_string();
            assert!(
                error.contains("invoice number is 41 bytes long, at most 40 are allowed"),
                "{name}: names the rule: {error}"
            );
        }

        let too_long = "x".repeat(InvoiceNumber::MAX_LEN + 1);
        let document = serde_json::to_value(document::tests::sample_document()).expect("json");
        refused::<StornoRequest>(
            "StornoRequest",
            serde_json::json!({"invoice_number": too_long}),
        );
        refused::<CorrectRequest>(
            "CorrectRequest",
            serde_json::json!({
                "invoice_number": too_long, "correction_id": "c-1", "document": document
            }),
        );
        refused::<SetCreditEntriesRequest>(
            "SetCreditEntriesRequest",
            serde_json::json!({"invoice_number": too_long, "entries": []}),
        );
        refused::<QueryRequest>(
            "QueryRequest",
            serde_json::json!({"selector": {"invoice_number": too_long}}),
        );
        refused::<CreateRequest>(
            "CreateRequest.options.proforma",
            serde_json::json!({
                "document": document, "options": {"proforma": {"number": too_long}}
            }),
        );
        let ok: StornoRequest =
            serde_json::from_value(serde_json::json!({"invoice_number": "SZ-1"})).expect("valid");
        assert_eq!(ok.invoice_number.as_str(), "SZ-1");
        assert_eq!(
            serde_json::to_value(&ok).expect("json")["invoice_number"],
            "SZ-1",
            "serialises as the plain string"
        );
    }

    /// The fault body is the public contract: it round-trips through JSON
    /// with the optional fields `null` when absent (the one rule of every
    /// response type), its status is its code's, and a fault written by the
    /// services reads back as this type (what the e2e harness decodes).
    #[test]
    fn fault_round_trips_with_absent_fields_null() {
        let bare = Fault::new(TerminalCode::InvalidInput, "malformed request body");
        let json = serde_json::to_value(&bare).expect("json");
        assert_eq!(
            json,
            serde_json::json!({
                "code": "invalid_input",
                "message": "malformed request body",
                "cause": null,
                "szamlazz_code": null,
                "order": null,
                "kind": null,
                "external_id": null,
            })
        );
        assert_eq!(bare.status(), Some(400));
        assert_eq!(serde_json::from_value::<Fault>(json).expect("back"), bare);

        let about = Fault::new(TerminalCode::NotFound, "invoice SZ-9 is not known (code 7)")
            .with_szamlazz_code("7")
            .about(
                &OrderKey::parse("ORD-1").expect("key"),
                Some(IssuedKind::Invoice),
                &ExternalId::new("acct:ORD-1:invoice"),
            );
        let json = serde_json::to_value(&about).expect("json");
        assert_eq!(
            json,
            serde_json::json!({
                "code": "not_found",
                "message": "invoice SZ-9 is not known (code 7)",
                "cause": null,
                "szamlazz_code": "7",
                "order": "ORD-1",
                "kind": "invoice",
                "external_id": "acct:ORD-1:invoice",
            })
        );
        assert_eq!(about.status(), Some(404));
        assert_eq!(serde_json::from_value::<Fault>(json).expect("back"), about);

        // Open: a field added later does not fail an older reader.
        let newer: Fault = serde_json::from_value(serde_json::json!({
            "code": "unavailable", "message": "m", "hint": "later"
        }))
        .expect("tolerates unknown fields");
        assert_eq!(newer.code, TerminalCode::Unavailable);
    }

    /// Every fault either service raises is one of these eight codes, each
    /// with the HTTP status the ingress reports for it; the token is the
    /// snake-case variant name and round-trips through serde.
    #[test]
    fn terminal_code_tokens() {
        let expected = [
            (TerminalCode::Forbidden, "forbidden", 403, false),
            (TerminalCode::Cancelled, "cancelled", 409, false),
            (TerminalCode::OutcomeUnknown, "outcome_unknown", 500, true),
            (TerminalCode::Unavailable, "unavailable", 503, true),
            (TerminalCode::InvalidInput, "invalid_input", 400, false),
            (
                TerminalCode::CredentialsRejected,
                "credentials_rejected",
                503,
                true,
            ),
            (TerminalCode::UnknownAccount, "unknown_account", 400, false),
            (TerminalCode::NotFound, "not_found", 404, false),
            (TerminalCode::SzamlazzError, "szamlazz_error", 422, false),
        ];
        assert_eq!(TerminalCode::KNOWN.len(), expected.len());
        for (code, token, status, outcome_unknown) in expected {
            assert!(TerminalCode::KNOWN.contains(&code), "{token} is in KNOWN");
            assert_eq!(code.as_str(), token);
            assert_eq!(code.status(), Some(status), "{token}");
            assert_eq!(code.is_outcome_unknown(), Some(outcome_unknown), "{token}");
            let json = serde_json::to_string(&code).expect("serialize");
            assert_eq!(json, format!("\"{token}\""));
            assert_eq!(
                serde_json::from_str::<TerminalCode>(&json).expect("deserialize"),
                code
            );
        }
    }

    /// The crate README's fault table lists every code with its status, as a
    /// row `` | `code` | status | ``. A new variant fails here until the
    /// table carries it.
    #[test]
    fn every_terminal_code_is_in_the_fault_table() {
        let readme = include_str!("../README.md");
        for code in TerminalCode::KNOWN {
            let status = code.status().expect("known code");
            let row = format!("| `{}` | {status} |", code.as_str());
            assert!(
                readme.contains(&row),
                "README.md lists `{}` with status {status}",
                code.as_str(),
            );
        }
    }

    #[test]
    fn an_unknown_fault_preserves_its_code_and_context_without_classification() {
        let wire = serde_json::json!({
            "code": " future_fault/é 🧾 ",
            "message": "a newer worker's explanation",
            "cause": null,
            "szamlazz_code": "999",
            "order": "ORD-1",
            "kind": "invoice",
            "external_id": "acct:ORD-1:invoice",
        });
        let fault: Fault = serde_json::from_value(wire.clone()).expect("open fault");
        assert_eq!(
            fault.code,
            TerminalCode::Other(" future_fault/é 🧾 ".to_owned())
        );
        assert_eq!(fault.status(), None);
        assert_eq!(fault.code.is_outcome_unknown(), None);
        assert_eq!(serde_json::to_value(fault).expect("json"), wire);
    }

    #[test]
    fn response_tokens_preserve_unknown_strings_and_refuse_non_strings() {
        fn check<T>(known: impl IntoIterator<Item = T>, other: fn(String) -> T)
        where
            T: serde::de::DeserializeOwned
                + Serialize
                + PartialEq
                + std::fmt::Debug
                + std::fmt::Display
                + From<String>
                + Into<String>
                + Clone,
        {
            for token in known {
                let wire = serde_json::to_value(&token).expect("json");
                let text = wire.as_str().expect("scalar string");
                assert_eq!(token.to_string(), text);
                assert_eq!(Into::<String>::into(token.clone()), text);
                assert_eq!(T::from(text.to_owned()), token);
                assert_eq!(serde_json::from_value::<T>(wire).expect("known"), token);
            }
            for text in ["", "future_token", " MixedCase/é 🧾 \n"] {
                let decoded: T = serde_json::from_value(serde_json::json!(text)).expect("open");
                assert_eq!(decoded, other(text.to_owned()));
                assert_eq!(decoded.to_string(), text);
                assert_eq!(serde_json::to_value(&decoded).expect("json"), text);
                assert_eq!(Into::<String>::into(decoded), text);
            }
            for value in [
                serde_json::json!(null),
                serde_json::json!(42),
                serde_json::json!({"other": "future"}),
                serde_json::json!(["issued"]),
            ] {
                assert!(serde_json::from_value::<T>(value).is_err());
            }
        }
        check(CreateOutcome::KNOWN, CreateOutcome::Other);
        check(ConflictReason::KNOWN, ConflictReason::Other);
        check(Warning::KNOWN, Warning::Other);
        check(StornoOutcome::KNOWN, StornoOutcome::Other);
        check(TerminalCode::KNOWN, TerminalCode::Other);
    }

    #[cfg(feature = "schemars")]
    #[test]
    fn response_token_schemas_are_open_strings_with_known_values_documented() {
        fn check<T: schemars::JsonSchema + std::fmt::Display>(known: impl IntoIterator<Item = T>) {
            let schema = serde_json::to_value(schemars::schema_for!(T)).expect("json");
            assert_eq!(schema["type"], "string", "{schema}");
            for closed in ["enum", "const", "oneOf", "anyOf", "allOf"] {
                assert!(schema.get(closed).is_none(), "{schema}");
            }
            let description = schema["description"].as_str().expect("description");
            for token in known {
                assert!(
                    description.contains(&token.to_string()),
                    "{token}: {description}"
                );
            }
        }
        check(CreateOutcome::KNOWN);
        check(ConflictReason::KNOWN);
        check(Warning::KNOWN);
        check(StornoOutcome::KNOWN);
        check(TerminalCode::KNOWN);

        let create = serde_json::to_value(schemars::schema_for!(CreateResponse)).expect("json");
        assert_eq!(
            create["properties"]["outcome"]["$ref"],
            "#/$defs/CreateOutcome"
        );
        assert_eq!(
            create["properties"]["warnings"]["items"]["$ref"],
            "#/$defs/Warning"
        );
        let fault = serde_json::to_value(schemars::schema_for!(Fault)).expect("json");
        assert_eq!(fault["properties"]["code"]["$ref"], "#/$defs/TerminalCode");
    }

    #[cfg(feature = "schemars")]
    #[test]
    fn structured_response_schemas_leave_state_open_and_keep_known_payload_requirements() {
        for (schema, known) in [
            (
                schemars::schema_for!(DocumentState),
                DocumentState::KNOWN.as_slice(),
            ),
            (
                schemars::schema_for!(CredentialsCheck),
                CredentialsCheck::KNOWN.as_slice(),
            ),
        ] {
            let schema = serde_json::to_value(schema).expect("json");
            assert_eq!(schema["type"], "object");
            assert_eq!(
                schema["properties"]["state"],
                serde_json::json!({"type": "string"})
            );
            assert_eq!(schema["required"], serde_json::json!(["state"]));
            assert!(schema.get("additionalProperties").is_none());
            for token in known {
                assert!(
                    schema["description"]
                        .as_str()
                        .expect("description")
                        .contains(token)
                );
            }
        }
        let document = serde_json::to_value(schemars::schema_for!(DocumentState)).expect("json");
        assert_eq!(
            document["allOf"][1]["if"]["properties"]["state"]["const"],
            "consumed"
        );
        assert_eq!(
            document["allOf"][1]["then"]["required"],
            serde_json::json!(["by"])
        );
        let credentials =
            serde_json::to_value(schemars::schema_for!(CredentialsCheck)).expect("json");
        assert_eq!(
            credentials["if"]["properties"]["state"]["const"],
            "rejected"
        );
        assert_eq!(
            credentials["then"]["required"],
            serde_json::json!(["code", "message"])
        );
        let status = serde_json::to_value(schemars::schema_for!(DocumentStatus)).expect("json");
        assert_eq!(
            status["properties"]["state"],
            serde_json::json!({"type": "string"})
        );
        assert!(
            status["allOf"].is_array(),
            "flattening retains payload conditions: {status}"
        );
    }

    /// A by-number request's schema references the bounded invoice-number
    /// type (whose own schema is pinned in `identity`).
    #[cfg(feature = "schemars")]
    #[test]
    fn by_number_request_schemas_reference_the_bounded_invoice_number() {
        let storno = serde_json::to_value(schemars::schema_for!(StornoRequest)).expect("json");
        assert!(
            storno["$defs"]["InvoiceNumber"].is_object(),
            "the request schema references the bounded type: {storno}"
        );
    }

    #[cfg(feature = "schemars")]
    #[test]
    fn contract_types_have_schemas() {
        let schema = schemars::schema_for!(CreateRequest);
        let json = serde_json::to_value(&schema).expect("serialize");
        assert_eq!(json["title"], "CreateRequest");
        assert!(json["properties"]["document"].is_object());
        assert!(json["properties"]["options"].is_object());
        assert!(json["$defs"]["DocumentInput"].is_object());

        let correct = serde_json::to_value(schemars::schema_for!(CorrectRequest)).expect("json");
        assert!(correct["properties"]["correction_id"].is_object());
        assert!(correct["$defs"]["CorrectionId"].is_object());

        for schema in [
            schemars::schema_for!(CorrectRequest),
            schemars::schema_for!(StornoRequest),
            schemars::schema_for!(DeleteProformaRequest),
            schemars::schema_for!(QueryRequest),
            schemars::schema_for!(SetCreditEntriesRequest),
            schemars::schema_for!(CreateResponse),
            schemars::schema_for!(StornoResponse),
            schemars::schema_for!(DeleteProformaResponse),
            schemars::schema_for!(SetCreditEntriesResponse),
            schemars::schema_for!(QueryResponse),
            schemars::schema_for!(OrderStatus),
        ] {
            serde_json::to_string(&schema).expect("schema serializes");
        }

        let status = serde_json::to_value(schemars::schema_for!(OrderStatus)).expect("serialize");
        assert!(status["properties"]["final"].is_object());
        assert!(status["$defs"]["DocumentStatus"].is_object());
    }

    /// The delete response's `reason` is a string in the schema (an open
    /// set: the worker's three tokens and any szamlazz.hu code), referenced
    /// as its own definition whose description names the tokens, so the
    /// `OpenAPI` export tells a caller what to branch on; the tokens the
    /// description names are the type's own constants.
    #[cfg(feature = "schemars")]
    #[test]
    fn the_delete_reason_schema_is_a_string_naming_the_workers_tokens() {
        let schema =
            serde_json::to_value(schemars::schema_for!(DeleteProformaResponse)).expect("json");
        assert_eq!(
            schema["properties"]["reason"]["anyOf"][0]["$ref"], "#/$defs/DeleteReason",
            "{schema}"
        );
        let reason = &schema["$defs"]["DeleteReason"];
        assert_eq!(reason["type"], "string", "{reason}");
        let description = reason["description"].as_str().expect("description");
        for token in [
            DeleteReason::ABSENT,
            DeleteReason::PROFORMA_PAID,
            DeleteReason::EXTERNAL_ID_COLLISION,
        ] {
            assert!(description.contains(token), "{token}: {description}");
        }
        assert!(reason.get("enum").is_none(), "an open set: {reason}");
    }

    /// Every request type's schema (and every object it nests, the object
    /// variants of its enums included) is closed (`additionalProperties:
    /// false`), so the `OpenAPI` export tightens with the code. Response
    /// schemas stay open: a client must tolerate fields added later.
    #[cfg(feature = "schemars")]
    #[test]
    fn request_schemas_are_closed_and_response_schemas_are_open() {
        /// Every object schema under `schema` (the root, each `$defs` entry
        /// and each object variant of a `oneOf`) as `(title, schema)`.
        fn objects(schema: &serde_json::Value) -> Vec<(String, &serde_json::Value)> {
            fn is_object(schema: &serde_json::Value) -> bool {
                schema["type"] == "object" || schema["properties"].is_object()
            }
            fn collect<'a>(
                title: &str,
                schema: &'a serde_json::Value,
                found: &mut Vec<(String, &'a serde_json::Value)>,
            ) {
                if is_object(schema) {
                    found.push((title.to_owned(), schema));
                }
                if let Some(variants) = schema["oneOf"].as_array() {
                    for (index, variant) in variants.iter().enumerate() {
                        collect(&format!("{title}/oneOf/{index}"), variant, found);
                    }
                }
            }
            let title = schema["title"].as_str().unwrap_or("<root>");
            let mut found = Vec::new();
            collect(title, schema, &mut found);
            if let Some(defs) = schema["$defs"].as_object() {
                for (name, def) in defs {
                    collect(name, def, &mut found);
                }
            }
            found
        }

        let requests = [
            ("CreateRequest", schemars::schema_for!(CreateRequest)),
            ("CorrectRequest", schemars::schema_for!(CorrectRequest)),
            ("StornoRequest", schemars::schema_for!(StornoRequest)),
            (
                "DeleteProformaRequest",
                schemars::schema_for!(DeleteProformaRequest),
            ),
            ("QueryRequest", schemars::schema_for!(QueryRequest)),
            (
                "QueryTaxpayerRequest",
                schemars::schema_for!(QueryTaxpayerRequest),
            ),
            (
                "SetCreditEntriesRequest",
                schemars::schema_for!(SetCreditEntriesRequest),
            ),
        ];
        let mut nested = std::collections::BTreeSet::new();
        for (name, schema) in &requests {
            let json = serde_json::to_value(schema).expect("serialize");
            assert_eq!(json["title"], *name);
            for (title, object) in objects(&json) {
                assert_eq!(
                    object["additionalProperties"],
                    serde_json::Value::Bool(false),
                    "{name}: {title} must refuse unknown fields: {object}"
                );
                nested.insert(title);
            }
        }
        // The nested request objects are all there (none slipped through as
        // a bare `properties` map without the guard), and so are the object
        // variants of the request enums (`{"number": …}`, `{"other": …}`,
        // the selectors).
        for expected in [
            "CreateOptions",
            "DocumentInput",
            "BuyerInput",
            "PostalAddressInput",
            "LineItemInput",
            "DocumentOverrides",
            "ExchangeRateInput",
            "CreditEntryInput",
            "ProformaLink/oneOf/2",
            "PaymentMethod/oneOf/7",
            "Selector/oneOf/0",
            "Selector/oneOf/1",
            "Selector/oneOf/2",
        ] {
            assert!(nested.contains(expected), "{expected} not in {nested:?}");
        }

        for schema in [
            schemars::schema_for!(CreateResponse),
            schemars::schema_for!(StornoResponse),
            schemars::schema_for!(QueryResponse),
            schemars::schema_for!(OrderStatus),
            schemars::schema_for!(CheckAccountResponse),
            schemars::schema_for!(QueryTaxpayerResponse),
        ] {
            let json = serde_json::to_value(&schema).expect("serialize");
            for (title, object) in objects(&json) {
                assert_eq!(
                    object.get("additionalProperties"),
                    None,
                    "{title} is a response and stays open: {object}"
                );
            }
        }
    }

    /// The doc comments of the contract types become the `description`s of
    /// the discovery manifest and the `OpenAPI` export, which render them as
    /// prose: rustdoc's link syntax, ``[`Type`]``, ``[text](path)``, would
    /// appear verbatim there. Every description of every schema (the root's,
    /// each property's, each `$defs` entry's and each enum variant's) is
    /// plain prose.
    #[cfg(feature = "schemars")]
    #[test]
    fn schema_descriptions_carry_no_rustdoc_link_syntax() {
        /// Every `(path, description)` under `value`.
        fn descriptions(path: &str, value: &serde_json::Value, found: &mut Vec<(String, String)>) {
            match value {
                serde_json::Value::Object(map) => {
                    for (key, child) in map {
                        let child_path = format!("{path}/{key}");
                        if key == "description"
                            && let Some(text) = child.as_str()
                        {
                            found.push((child_path, text.to_owned()));
                        } else {
                            descriptions(&child_path, child, found);
                        }
                    }
                }
                serde_json::Value::Array(items) => {
                    for (index, item) in items.iter().enumerate() {
                        descriptions(&format!("{path}/{index}"), item, found);
                    }
                }
                _ => {}
            }
        }

        let schemas = [
            ("CreateRequest", schemars::schema_for!(CreateRequest)),
            ("CorrectRequest", schemars::schema_for!(CorrectRequest)),
            ("StornoRequest", schemars::schema_for!(StornoRequest)),
            (
                "DeleteProformaRequest",
                schemars::schema_for!(DeleteProformaRequest),
            ),
            ("QueryRequest", schemars::schema_for!(QueryRequest)),
            (
                "QueryTaxpayerRequest",
                schemars::schema_for!(QueryTaxpayerRequest),
            ),
            (
                "SetCreditEntriesRequest",
                schemars::schema_for!(SetCreditEntriesRequest),
            ),
            ("CreateResponse", schemars::schema_for!(CreateResponse)),
            ("StornoResponse", schemars::schema_for!(StornoResponse)),
            (
                "DeleteProformaResponse",
                schemars::schema_for!(DeleteProformaResponse),
            ),
            ("QueryResponse", schemars::schema_for!(QueryResponse)),
            (
                "QueryTaxpayerResponse",
                schemars::schema_for!(QueryTaxpayerResponse),
            ),
            (
                "SetCreditEntriesResponse",
                schemars::schema_for!(SetCreditEntriesResponse),
            ),
            (
                "CheckAccountResponse",
                schemars::schema_for!(CheckAccountResponse),
            ),
            ("OrderStatus", schemars::schema_for!(OrderStatus)),
            ("DocumentKind", schemars::schema_for!(DocumentKind)),
            ("IssuedKind", schemars::schema_for!(IssuedKind)),
        ];
        let mut leaks = Vec::new();
        for (name, schema) in &schemas {
            let json = serde_json::to_value(schema).expect("serialize");
            let mut found = Vec::new();
            descriptions(name, &json, &mut found);
            assert!(!found.is_empty(), "{name}: the walk found no description");
            for (path, text) in found {
                if ["[`", "](", "`]"]
                    .iter()
                    .any(|marker| text.contains(marker))
                {
                    leaks.push(format!("{path}: {text}"));
                }
            }
        }
        assert!(
            leaks.is_empty(),
            "rustdoc link syntax leaks into these OpenAPI descriptions:\n{}",
            leaks.join("\n---\n")
        );
    }
}
