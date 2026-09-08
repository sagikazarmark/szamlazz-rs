//! SDK-independent request and response contract of the `Szamlazz.Order` and
//! `Szamlazz.Agent` services.
//!
//! Everything here is plain data with a stable JSON shape: domain outcomes are
//! returned as values with HTTP 200 (see [`Outcome`] and [`ConflictReason`]),
//! while the [`TerminalCode`]s are reserved for faults and always mean
//! "outcome unknown: retry with a new `Idempotency-Key`". The types compile
//! without `restate-sdk`; with the `schemars` feature they also derive JSON
//! Schemas for the `OpenAPI` export.
//!
//! Every request type refuses a field it does not know
//! (`#[serde(deny_unknown_fields)]`, `additionalProperties: false` in the
//! schema): a misspelt `reissue` or `additive` is an error naming the field,
//! never a silent default. Response types stay open: a client must tolerate
//! fields added later.
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
//!   `set_payments` and `check_account`.

use std::fmt;
use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::identity::ExternalId;

pub mod agent;
pub mod create;
pub mod document;
pub mod storno;

pub use agent::{
    CheckAccountResponse, CheckedAccount, CredentialsCheck, InvalidTaxNumber, PaymentEntry,
    PaymentRecord, QueryRequest, QueryResponse, QueryTaxpayerRequest, QueryTaxpayerResponse,
    Selector, SetPaymentsRequest, SetPaymentsResponse, TaxpayerAddress,
};
pub use create::{
    ConflictReason, CorrectRequest, CreateOptions, CreateRequest, CreateResponse, Outcome,
    ProformaLink, Warning,
};
pub use document::{
    BuyerInput, DocumentInput, DocumentOverrides, ExchangeRateInput, LineItemInput, PaymentMethod,
    PostalAddressInput, TaxpayerStatus,
};
pub use storno::{
    DeleteProformaRequest, DeleteProformaResponse, DocumentState, DocumentStatus, OrderStatus,
    StornoOutcome, StornoRequest, StornoResponse,
};

/// The caller-supplied identity of one corrective invoice.
///
/// Several correctives per invoice are legitimate, so the caller names each
/// one; the id is embedded in the corrective's external id
/// (`{namespace}:{order}:corrective:{id}`) and a new id issues a new corrective by
/// contract. The same id finds the corrective it issued.
///
/// Valid ids match `^[A-Za-z0-9][A-Za-z0-9._-]{0,39}$` and are not one of the
/// tokens external ids are composed of ([`ExternalId::TOKENS`]: `invoice`,
/// `storno`, `check-account`, … in any letter case), so no composition reads
/// as another. The length keeps the longest composed id within
/// [`ExternalId::MAX_LEN`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CorrectionId(String);

impl CorrectionId {
    /// The maximum length in bytes (the id is ASCII, so also in characters).
    /// A dashed UUID (36) fits.
    pub const MAX_LEN: usize = 40;

    /// The id as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(value: &str) -> Result<(), InvalidCorrectionId> {
        let mut chars = value.chars();
        let Some(first) = chars.next() else {
            return Err(InvalidCorrectionId::Empty);
        };
        if value.len() > Self::MAX_LEN {
            return Err(InvalidCorrectionId::TooLong(value.len()));
        }
        if !first.is_ascii_alphanumeric() {
            return Err(InvalidCorrectionId::InvalidStart(first));
        }
        if let Some(invalid) =
            chars.find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')))
        {
            return Err(InvalidCorrectionId::InvalidChar(invalid));
        }
        if ExternalId::is_token(value) {
            return Err(InvalidCorrectionId::Reserved(value.to_owned()));
        }
        Ok(())
    }
}

impl FromStr for CorrectionId {
    type Err = InvalidCorrectionId;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::validate(value)?;
        Ok(Self(value.to_owned()))
    }
}

impl TryFrom<String> for CorrectionId {
    type Error = InvalidCorrectionId;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::validate(&value)?;
        Ok(Self(value))
    }
}

impl TryFrom<&str> for CorrectionId {
    type Error = InvalidCorrectionId;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl fmt::Display for CorrectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for CorrectionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<CorrectionId> for String {
    fn from(id: CorrectionId) -> Self {
        id.0
    }
}

/// Serializes as the plain string.
impl Serialize for CorrectionId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// Deserializes from a string, rejecting ids that do not match the pattern.
impl<'de> Deserialize<'de> for CorrectionId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for CorrectionId {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "CorrectionId".into()
    }

    fn schema_id() -> std::borrow::Cow<'static, str> {
        concat!(module_path!(), "::CorrectionId").into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "Caller-supplied identity of one corrective invoice; not one of the external-id tokens (invoice, proforma, prepayment, final, corrective, storno, by-number, check-account).",
            "pattern": "^[A-Za-z0-9][A-Za-z0-9._-]{0,39}$",
        })
    }
}

/// A string that is not a valid [`CorrectionId`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidCorrectionId {
    /// The id is empty.
    #[error("correction id must not be empty")]
    Empty,
    /// The id exceeds [`CorrectionId::MAX_LEN`] bytes.
    #[error("correction id is {0} bytes long, at most {max} are allowed", max = CorrectionId::MAX_LEN)]
    TooLong(usize),
    /// The first character is not an ASCII letter or digit.
    #[error("correction id must start with an ASCII letter or digit, found {0:?}")]
    InvalidStart(char),
    /// A later character is outside `[A-Za-z0-9._-]`.
    #[error("correction id may only contain ASCII letters, digits, '.', '_' and '-', found {0:?}")]
    InvalidChar(char),
    /// The id is one of the tokens external ids are composed of
    /// ([`ExternalId::TOKENS`]), in any letter case.
    #[error(
        "correction id {0:?} is reserved: the external-id tokens ({tokens}) are not correction ids",
        tokens = ExternalId::TOKENS.join(", ")
    )]
    Reserved(String),
}

/// A caller-supplied invoice number (`számlaszám`), as the by-number requests
/// take it: `Szamlazz.Agent.query`'s selector, `set_payments`, `storno`,
/// `Szamlazz.Order.storno_invoice`, the base of `correct_invoice` and the
/// `options.proforma: {number}` link.
///
/// Bounded because it flows into step names and into the storno external ids
/// (`{namespace}:{order}:storno:{number}`, `{namespace}:by-number:{number}:storno`):
/// 1–[`MAX_LEN`](Self::MAX_LEN) bytes, no whitespace, no control character,
/// no `:`. Nothing is trimmed: a padded number is refused, never sent, since
/// szamlazz.hu would answer 7 (`not_found`) to it and the rule is the better
/// diagnosis. szamlazz.hu's own numbers (`E-TST-2026-123`) are far inside the
/// bound; NAV's `invoiceNumber` allows 50 characters, and the longest composed
/// external id keeps the bound within [`ExternalId::MAX_LEN`].
///
/// Distinct from `szamlazz_agent::InvoiceNumber`, the unvalidated wire type a
/// number szamlazz.hu *reports* is carried in; the worker's response types
/// echo numbers as plain strings.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InvoiceNumber(String);

impl InvoiceNumber {
    /// The maximum length in bytes.
    pub const MAX_LEN: usize = 40;

    /// The number as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(value: &str) -> Result<(), InvalidInvoiceNumber> {
        if value.is_empty() {
            return Err(InvalidInvoiceNumber::Empty);
        }
        if value.len() > Self::MAX_LEN {
            return Err(InvalidInvoiceNumber::TooLong(value.len()));
        }
        if let Some(control) = value.chars().find(|c| c.is_control()) {
            return Err(InvalidInvoiceNumber::ControlChar(control));
        }
        if let Some(whitespace) = value.chars().find(|c| c.is_whitespace()) {
            return Err(InvalidInvoiceNumber::Whitespace(whitespace));
        }
        if value.contains(ExternalId::SEPARATOR) {
            return Err(InvalidInvoiceNumber::Separator);
        }
        Ok(())
    }
}

impl FromStr for InvoiceNumber {
    type Err = InvalidInvoiceNumber;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::validate(value)?;
        Ok(Self(value.to_owned()))
    }
}

impl TryFrom<String> for InvoiceNumber {
    type Error = InvalidInvoiceNumber;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::validate(&value)?;
        Ok(Self(value))
    }
}

impl TryFrom<&str> for InvoiceNumber {
    type Error = InvalidInvoiceNumber;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl fmt::Display for InvoiceNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for InvoiceNumber {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<InvoiceNumber> for String {
    fn from(number: InvoiceNumber) -> Self {
        number.0
    }
}

/// Serializes as the plain string.
impl Serialize for InvoiceNumber {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// Deserializes from a string, rejecting numbers outside the bound.
impl<'de> Deserialize<'de> for InvoiceNumber {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for InvoiceNumber {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "InvoiceNumber".into()
    }

    fn schema_id() -> std::borrow::Cow<'static, str> {
        concat!(module_path!(), "::InvoiceNumber").into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "An invoice number (számlaszám) as the caller names it: 1–40 bytes, no whitespace, no control character, no ':'.",
            "minLength": 1,
            "maxLength": InvoiceNumber::MAX_LEN,
            "pattern": "^[^\\s\\x00-\\x1F\\x7F:]+$",
        })
    }
}

/// A string that is not a valid [`InvoiceNumber`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidInvoiceNumber {
    /// The number is empty.
    #[error("invoice number must not be empty")]
    Empty,
    /// The number exceeds [`InvoiceNumber::MAX_LEN`] bytes.
    #[error("invoice number is {0} bytes long, at most {max} are allowed", max = InvoiceNumber::MAX_LEN)]
    TooLong(usize),
    /// Contains a control character.
    #[error("invoice number must not contain control characters, found {0:?}")]
    ControlChar(char),
    /// Contains whitespace, anywhere; nothing is trimmed.
    #[error("invoice number must not contain whitespace, found {0:?}")]
    Whitespace(char),
    /// Contains `:`, the separator of the external id's segments.
    #[error("invoice number must not contain ':' (the external-id separator)")]
    Separator,
}

/// A document kind of which an order carries at most one live document, each
/// with its own handler and external id.
///
/// Correctives are not kinds in this sense (an order may carry any number of
/// them); the kind of an issued document, correctives included, is
/// `IssuedKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    /// Proforma (`díjbekérő`).
    Proforma,
    /// Invoice (`számla`).
    Invoice,
    /// Prepayment invoice (`előlegszámla`).
    Prepayment,
    /// Final invoice (`végszámla`).
    Final,
}

impl DocumentKind {
    /// Every kind, in the order `Szamlazz.Order.get` reports them.
    pub const ALL: [Self; 4] = [Self::Proforma, Self::Invoice, Self::Prepayment, Self::Final];

    /// The snake-case token used on the wire and inside external ids.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Proforma => "proforma",
            Self::Invoice => "invoice",
            Self::Prepayment => "prepayment",
            Self::Final => "final",
        }
    }

    /// Whether the kind is a legal invoice (everything except a proforma).
    #[must_use]
    pub const fn is_invoice_kind(self) -> bool {
        !matches!(self, Self::Proforma)
    }
}

impl fmt::Display for DocumentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The kind of a document the service issued: the four kinds of
/// `DocumentKind` plus correctives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum IssuedKind {
    /// Proforma (`díjbekérő`).
    Proforma,
    /// Invoice (`számla`).
    Invoice,
    /// Prepayment invoice (`előlegszámla`).
    Prepayment,
    /// Final invoice (`végszámla`).
    Final,
    /// Corrective invoice (`helyesbítő számla`).
    Corrective,
}

impl IssuedKind {
    /// Every issued kind: the four [`DocumentKind`]s in their order, then
    /// `Corrective`.
    pub const ALL: [Self; 5] = [
        Self::Proforma,
        Self::Invoice,
        Self::Prepayment,
        Self::Final,
        Self::Corrective,
    ];

    /// The snake-case token used on the wire and inside external ids.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Proforma => "proforma",
            Self::Invoice => "invoice",
            Self::Prepayment => "prepayment",
            Self::Final => "final",
            Self::Corrective => "corrective",
        }
    }

    /// The document kind, or `None` for a corrective.
    #[must_use]
    pub const fn document_kind(self) -> Option<DocumentKind> {
        match self {
            Self::Proforma => Some(DocumentKind::Proforma),
            Self::Invoice => Some(DocumentKind::Invoice),
            Self::Prepayment => Some(DocumentKind::Prepayment),
            Self::Final => Some(DocumentKind::Final),
            Self::Corrective => None,
        }
    }
}

impl From<DocumentKind> for IssuedKind {
    fn from(kind: DocumentKind) -> Self {
        match kind {
            DocumentKind::Proforma => Self::Proforma,
            DocumentKind::Invoice => Self::Invoice,
            DocumentKind::Prepayment => Self::Prepayment,
            DocumentKind::Final => Self::Final,
        }
    }
}

impl fmt::Display for IssuedKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The code of a `TerminalError` any handler of either service may raise.
///
/// Every fault either service raises carries one of these tokens in `code`,
/// with the HTTP status of [`status`](Self::status). Three of them mean
/// "outcome unknown: retry with a new `Idempotency-Key`, or read
/// `Szamlazz.Order.get`": `outcome_unknown`, `unavailable` and
/// `credentials_rejected`. The rest are settled: the same request never
/// succeeds (`invalid_input`, `unknown_account`, `not_found`) or
/// szamlazz.hu's own answer is passed through
/// (`szamlazz_error`, whose szamlazz.hu code travels in the fault's separate
/// `szamlazz_code` field; `code` is always one of these tokens).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TerminalCode {
    /// The create or storno step ran out of the issue policy while a document
    /// may or may not have been issued; the next call's external-id query
    /// finds whatever landed. HTTP 500.
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
    /// key, then retry with a new `Idempotency-Key`. HTTP 503.
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
    /// relayed one) and `set_payments` (the credit entries refused). The
    /// szamlazz.hu code is in the fault's `szamlazz_code`, the message is
    /// szamlazz.hu's. HTTP 422.
    SzamlazzError,
}

impl TerminalCode {
    /// Every code, in the order of the fault tables the READMEs carry.
    /// (`account_mismatch`, 409, was the eighth until the account pins were
    /// dropped: no handler compares a found document with the account any
    /// more, so nothing could raise it.)
    pub const ALL: [Self; 7] = [
        Self::InvalidInput,
        Self::UnknownAccount,
        Self::NotFound,
        Self::SzamlazzError,
        Self::OutcomeUnknown,
        Self::Unavailable,
        Self::CredentialsRejected,
    ];

    /// The snake-case token carried in the error.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OutcomeUnknown => "outcome_unknown",
            Self::Unavailable => "unavailable",
            Self::InvalidInput => "invalid_input",
            Self::CredentialsRejected => "credentials_rejected",
            Self::UnknownAccount => "unknown_account",
            Self::NotFound => "not_found",
            Self::SzamlazzError => "szamlazz_error",
        }
    }

    /// The HTTP status the ingress reports for a fault with this code.
    #[must_use]
    pub const fn status(self) -> u16 {
        match self {
            // The caller's request: the same request never succeeds.
            Self::InvalidInput | Self::UnknownAccount => 400,
            Self::NotFound => 404,
            // szamlazz.hu's own answer, passed through.
            Self::SzamlazzError => 422,
            Self::OutcomeUnknown => 500,
            // The worker's misconfiguration or szamlazz.hu not answering, not
            // the caller's request: the same request succeeds once the key
            // is fixed or szamlazz.hu answers, so neither a 4xx ("do not
            // retry") nor 401/403 ("you are unauthenticated") fits.
            Self::Unavailable | Self::CredentialsRejected => 503,
        }
    }
}

impl fmt::Display for TerminalCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `gross − Σ payments`, when the gross total is known and the arithmetic
/// fits a decimal. The one definition of the outstanding amount both
/// `create_*` and `query` report. Checked: the amounts are szamlazz.hu's, but
/// a panic would run on the SDK's connection task.
pub(crate) fn outstanding(gross: Option<Decimal>, payments: &[Decimal]) -> Option<Decimal> {
    let paid = payments
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

    #[test]
    fn correction_id_accepts_valid_ids() {
        for id in [
            "a",
            "0",
            "c-2",
            "order.42_fix-1",
            "A",
            "123e4567-e89b-12d3-a456-426614174000",
            "invoice-2",
            "storno.1",
            &"x".repeat(CorrectionId::MAX_LEN),
        ] {
            let parsed: CorrectionId = id.parse().expect(id);
            assert_eq!(parsed.as_str(), id);
            assert_eq!(parsed.to_string(), id);
        }
        assert_eq!(
            CorrectionId::MAX_LEN,
            40,
            "a dashed UUID (36) fits with room"
        );
    }

    #[test]
    fn correction_id_rejects_invalid_ids() {
        let too_long = "x".repeat(CorrectionId::MAX_LEN + 1);
        let cases = [
            ("", InvalidCorrectionId::Empty),
            ("-a", InvalidCorrectionId::InvalidStart('-')),
            (".a", InvalidCorrectionId::InvalidStart('.')),
            ("a b", InvalidCorrectionId::InvalidChar(' ')),
            ("a/b", InvalidCorrectionId::InvalidChar('/')),
            ("a:b", InvalidCorrectionId::InvalidChar(':')),
            ("á", InvalidCorrectionId::InvalidStart('á')),
            ("aá", InvalidCorrectionId::InvalidChar('á')),
            (
                too_long.as_str(),
                InvalidCorrectionId::TooLong(CorrectionId::MAX_LEN + 1),
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(
                input.parse::<CorrectionId>(),
                Err(expected.clone()),
                "{input:?}"
            );
            assert_eq!(CorrectionId::try_from(input.to_owned()), Err(expected));
        }
    }

    /// A correction id equal to one of the tokens the external ids are
    /// composed of (the kinds, `corrective`, `storno`, `by-number`,
    /// `check-account`) is refused, in any letter case, so that
    /// `{namespace}:{order}:corrective:{id}` never reads as another
    /// composition. Belt and braces beside the `:`-free
    /// [`OrderKey`](crate::identity::OrderKey), which already makes such a
    /// collision impossible.
    #[test]
    fn correction_id_refuses_the_external_id_tokens() {
        for token in ExternalId::TOKENS {
            for spelling in [token.to_owned(), token.to_ascii_uppercase()] {
                assert_eq!(
                    spelling.parse::<CorrectionId>(),
                    Err(InvalidCorrectionId::Reserved(spelling.clone())),
                    "{spelling:?}"
                );
                let error = serde_json::from_str::<CorrectionId>(&format!("\"{spelling}\""))
                    .expect_err("refused through serde too")
                    .to_string();
                assert!(
                    error.contains("reserved") && error.contains(token),
                    "{spelling:?}: names the rule and the token: {error}"
                );
            }
        }
        assert_eq!(ExternalId::TOKENS.len(), 8);
        for kind in IssuedKind::ALL {
            assert!(ExternalId::TOKENS.contains(&kind.as_str()), "{kind}");
        }
    }

    #[test]
    fn correction_id_serde_validates() {
        let id: CorrectionId = serde_json::from_str("\"c-1\"").expect("valid");
        assert_eq!(id.as_str(), "c-1");
        assert_eq!(serde_json::to_string(&id).expect("serialize"), "\"c-1\"");
        assert!(serde_json::from_str::<CorrectionId>("\"-c\"").is_err());
        assert!(serde_json::from_str::<CorrectionId>("\"\"").is_err());
    }

    /// A caller-supplied invoice number as the by-number requests take it
    /// (`Szamlazz.Agent.query`'s selector, `set_payments`, `storno`,
    /// `Szamlazz.Order.storno_invoice`, `correct_invoice`'s base and the
    /// `options.proforma: {number}` link): at most 40 bytes, no whitespace, no
    /// control character, no `:`. It flows into step names and into the
    /// storno external ids, so it is bounded like the other segments;
    /// szamlazz.hu's own numbers (`E-TST-2026-123`) are far inside.
    #[test]
    fn invoice_number_table() {
        let longest = "x".repeat(InvoiceNumber::MAX_LEN);
        for number in [
            "SZ-1",
            "E-TST-2026-123",
            "D-2026-7",
            "2026/0001",
            "É-2026-1",
            longest.as_str(),
        ] {
            let parsed: InvoiceNumber = number.parse().expect(number);
            assert_eq!(parsed.as_str(), number);
            assert_eq!(parsed.to_string(), number);
            assert_eq!(InvoiceNumber::try_from(number.to_owned()), Ok(parsed));
        }
        assert_eq!(InvoiceNumber::MAX_LEN, 40);

        let too_long = "x".repeat(InvoiceNumber::MAX_LEN + 1);
        for (input, expected) in [
            ("", InvalidInvoiceNumber::Empty),
            (" SZ-1", InvalidInvoiceNumber::Whitespace(' ')),
            ("SZ-1 ", InvalidInvoiceNumber::Whitespace(' ')),
            ("SZ 1", InvalidInvoiceNumber::Whitespace(' ')),
            ("SZ\u{a0}1", InvalidInvoiceNumber::Whitespace('\u{a0}')),
            ("SZ\t1", InvalidInvoiceNumber::ControlChar('\t')),
            ("SZ\u{7f}1", InvalidInvoiceNumber::ControlChar('\u{7f}')),
            ("SZ:1", InvalidInvoiceNumber::Separator),
            (
                too_long.as_str(),
                InvalidInvoiceNumber::TooLong(InvoiceNumber::MAX_LEN + 1),
            ),
        ] {
            assert_eq!(
                input.parse::<InvoiceNumber>(),
                Err(expected.clone()),
                "{input:?}"
            );
            assert_eq!(InvoiceNumber::try_from(input.to_owned()), Err(expected));
        }
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
        refused::<SetPaymentsRequest>(
            "SetPaymentsRequest",
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

    #[test]
    fn correction_id_orders_as_string() {
        let a: CorrectionId = "a".parse().expect("valid");
        let b: CorrectionId = "b".parse().expect("valid");
        assert!(a < b);
        let mut map = std::collections::BTreeMap::new();
        map.insert(b.clone(), 2);
        map.insert(a.clone(), 1);
        assert_eq!(map.keys().collect::<Vec<_>>(), vec![&a, &b]);
    }

    #[test]
    fn kinds_serialize_snake_case() {
        assert_eq!(
            serde_json::to_string(&DocumentKind::Prepayment).expect("serialize"),
            "\"prepayment\""
        );
        assert_eq!(
            serde_json::to_string(&IssuedKind::Corrective).expect("serialize"),
            "\"corrective\""
        );
        assert_eq!(
            serde_json::from_str::<DocumentKind>("\"final\"").expect("deserialize"),
            DocumentKind::Final
        );
        assert!(serde_json::from_str::<DocumentKind>("\"corrective\"").is_err());
        for kind in DocumentKind::ALL {
            assert_eq!(IssuedKind::from(kind).document_kind(), Some(kind));
            assert_eq!(IssuedKind::from(kind).as_str(), kind.as_str());
        }
        assert_eq!(IssuedKind::Corrective.document_kind(), None);
    }

    /// Every fault either service raises is one of these seven codes, each
    /// with the HTTP status the ingress reports for it; the token is the
    /// snake-case variant name and round-trips through serde.
    #[test]
    fn terminal_code_tokens() {
        let expected = [
            (TerminalCode::OutcomeUnknown, "outcome_unknown", 500),
            (TerminalCode::Unavailable, "unavailable", 503),
            (TerminalCode::InvalidInput, "invalid_input", 400),
            (
                TerminalCode::CredentialsRejected,
                "credentials_rejected",
                503,
            ),
            (TerminalCode::UnknownAccount, "unknown_account", 400),
            (TerminalCode::NotFound, "not_found", 404),
            (TerminalCode::SzamlazzError, "szamlazz_error", 422),
        ];
        assert_eq!(TerminalCode::ALL.len(), expected.len());
        for (code, token, status) in expected {
            assert!(TerminalCode::ALL.contains(&code), "{token} is in ALL");
            assert_eq!(code.as_str(), token);
            assert_eq!(code.status(), status, "{token}");
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
    /// table carries it. (The endpoint README carries the same table; the
    /// endpoint crate's `config` tests hold it to the codes: it lives outside
    /// this package, which `cargo package` cannot include.)
    #[test]
    fn every_terminal_code_is_in_the_fault_table() {
        let readme = include_str!("../README.md");
        for code in TerminalCode::ALL {
            let row = format!("| `{}` | {} |", code.as_str(), code.status());
            assert!(
                readme.contains(&row),
                "README.md lists `{}` with status {}",
                code.as_str(),
                code.status()
            );
        }
    }

    #[cfg(feature = "schemars")]
    #[test]
    fn correction_id_schema_carries_the_pattern() {
        let schema = schemars::schema_for!(CorrectionId);
        let json = serde_json::to_value(&schema).expect("serialize");
        assert_eq!(json["pattern"], "^[A-Za-z0-9][A-Za-z0-9._-]{0,39}$");
    }

    /// The schema carries the bound the type enforces: the length as
    /// `maxLength`, and a pattern that refuses whitespace, ASCII control
    /// characters and the separator, in the ECMA-262 subset JSON Schema
    /// guarantees, so no `\p{…}` class.
    #[cfg(feature = "schemars")]
    #[test]
    fn invoice_number_schema_carries_the_bound() {
        let schema = schemars::schema_for!(InvoiceNumber);
        let json = serde_json::to_value(&schema).expect("serialize");
        assert_eq!(json["maxLength"], InvoiceNumber::MAX_LEN);
        assert_eq!(json["minLength"], 1);
        assert_eq!(json["pattern"], "^[^\\s\\x00-\\x1F\\x7F:]+$");
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
            schemars::schema_for!(SetPaymentsRequest),
            schemars::schema_for!(CreateResponse),
            schemars::schema_for!(StornoResponse),
            schemars::schema_for!(DeleteProformaResponse),
            schemars::schema_for!(SetPaymentsResponse),
            schemars::schema_for!(QueryResponse),
            schemars::schema_for!(OrderStatus),
        ] {
            serde_json::to_string(&schema).expect("schema serializes");
        }

        let status = serde_json::to_value(schemars::schema_for!(OrderStatus)).expect("serialize");
        assert!(status["properties"]["final"].is_object());
        assert!(status["$defs"]["DocumentStatus"].is_object());
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
                "SetPaymentsRequest",
                schemars::schema_for!(SetPaymentsRequest),
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
            "PaymentEntry",
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
                "SetPaymentsRequest",
                schemars::schema_for!(SetPaymentsRequest),
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
                "SetPaymentsResponse",
                schemars::schema_for!(SetPaymentsResponse),
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
