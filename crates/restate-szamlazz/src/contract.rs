//! SDK-independent request and response contract of the `Szamlazz.Order` and
//! `Szamlazz.Agent` services.
//!
//! Everything here is plain data with a stable JSON shape: domain outcomes are
//! returned as values (see [`Outcome`] and [`ConflictReason`]), while the
//! [`TerminalCode`]s are reserved for faults. The types compile without
//! `restate-sdk`; with the `schemars` feature they also derive JSON Schemas for
//! the `OpenAPI` export.
//!
//! - [`document`] — the per-call document input (buyer, line items, payment
//!   method, overrides) and its conversion to `szamlazz_agent` types.
//! - [`request`] — handler inputs.
//! - [`response`] — handler outputs, including the [`OrderStatus`] live view.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

pub mod document;
pub mod request;
pub mod response;

pub use document::{
    BuyerInput, DocumentInput, DocumentOverrides, ExchangeRateInput, LineItemInput, PaymentMethod,
    PostalAddressInput, TaxpayerStatus,
};
pub use request::{
    CorrectRequest, CreateOptions, CreateRequest, DeleteProformaRequest, PaymentEntry,
    ProformaLink, QueryRequest, Selector, SetPaymentsRequest, StornoRequest,
};
pub use response::{
    CheckAccountResponse, CheckedAccount, ConflictReason, CreateResponse, CredentialsCheck,
    DeleteProformaResponse, DocumentState, DocumentStatus, OrderStatus, Outcome, PaymentRecord,
    QueryResponse, SetPaymentsResponse, StornoOutcome, StornoResponse, Warning,
};

/// The caller-supplied identity of one corrective invoice.
///
/// Several correctives per invoice are legitimate, so the caller names each
/// one; the id is embedded in the corrective's external id
/// (`{namespace}:{order}:corrective:{id}`) and a new id issues a new corrective by
/// contract. The same id finds the corrective it issued.
///
/// Valid ids match `^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CorrectionId(String);

impl CorrectionId {
    /// The maximum length in bytes (the id is ASCII, so also in characters).
    pub const MAX_LEN: usize = 64;

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
            "description": "Caller-supplied identity of one corrective invoice.",
            "pattern": "^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$",
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
}

/// A document kind of which an order carries at most one live document, each
/// with its own handler and external id.
///
/// Correctives are not kinds in this sense (an order may carry any number of
/// them); see [`IssuedKind`] for the kind of an issued document.
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

/// The kind of a document the service issued: the four [`DocumentKind`]s plus
/// correctives.
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
/// Every one of them means "outcome unknown — retry with a new
/// `Idempotency-Key`, or read `Szamlazz.Order.get`", never "no document
/// exists". The by-number `Szamlazz.Agent` handlers additionally answer a
/// miss as 404 `not_found` and pass a szamlazz.hu error through as 422 with
/// its own code; those are not `TerminalCode`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TerminalCode {
    /// The create or storno step ran out of the issue policy while a document
    /// may or may not have been issued; the next call's external-id query
    /// finds whatever landed.
    OutcomeUnknown,
    /// szamlazz.hu could not be reached for a check that must succeed before
    /// anything is issued.
    Unavailable,
    /// A document found under our identity belongs to a different szamlazz.hu
    /// account.
    AccountMismatch,
    /// The request is malformed or names a document szamlazz.hu does not
    /// know.
    InvalidInput,
    /// szamlazz.hu rejected the account's agent credentials (codes 3, 135,
    /// 136, 164): the worker's configuration is wrong, not the request. The
    /// execution that observed the code issued nothing — szamlazz.hu answers
    /// these codes before acting on a request — but an earlier execution may
    /// have landed with a lost reply, which is why this is a fault and not a
    /// `rejected` outcome. Fix the key, then retry with a new
    /// `Idempotency-Key`. HTTP 503.
    CredentialsRejected,
    /// The request names no account of this deployment: it arrived unscoped
    /// where accounts are reachable by scope only, or under a scope no account
    /// is reachable by. Raised before anything is issued; the same request
    /// never succeeds, so the caller must fix the scope, not retry. HTTP 400.
    UnknownAccount,
}

impl TerminalCode {
    /// The snake-case token carried in the error.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OutcomeUnknown => "outcome_unknown",
            Self::Unavailable => "unavailable",
            Self::AccountMismatch => "account_mismatch",
            Self::InvalidInput => "invalid_input",
            Self::CredentialsRejected => "credentials_rejected",
            Self::UnknownAccount => "unknown_account",
        }
    }
}

impl fmt::Display for TerminalCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correction_id_accepts_valid_ids() {
        for id in [
            "a",
            "0",
            "c-2",
            "order.42_fix-1",
            "A",
            &"x".repeat(CorrectionId::MAX_LEN),
        ] {
            let parsed: CorrectionId = id.parse().expect(id);
            assert_eq!(parsed.as_str(), id);
            assert_eq!(parsed.to_string(), id);
        }
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
            ("á", InvalidCorrectionId::InvalidStart('á')),
            ("aá", InvalidCorrectionId::InvalidChar('á')),
            (too_long.as_str(), InvalidCorrectionId::TooLong(65)),
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

    #[test]
    fn correction_id_serde_validates() {
        let id: CorrectionId = serde_json::from_str("\"c-1\"").expect("valid");
        assert_eq!(id.as_str(), "c-1");
        assert_eq!(serde_json::to_string(&id).expect("serialize"), "\"c-1\"");
        assert!(serde_json::from_str::<CorrectionId>("\"-c\"").is_err());
        assert!(serde_json::from_str::<CorrectionId>("\"\"").is_err());
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

    #[test]
    fn terminal_code_tokens() {
        assert_eq!(
            TerminalCode::CredentialsRejected.as_str(),
            "credentials_rejected"
        );
        for code in [
            TerminalCode::OutcomeUnknown,
            TerminalCode::Unavailable,
            TerminalCode::AccountMismatch,
            TerminalCode::InvalidInput,
            TerminalCode::CredentialsRejected,
        ] {
            let json = serde_json::to_string(&code).expect("serialize");
            assert_eq!(json, format!("\"{}\"", code.as_str()));
            assert_eq!(
                serde_json::from_str::<TerminalCode>(&json).expect("deserialize"),
                code
            );
        }
    }

    #[cfg(feature = "schemars")]
    #[test]
    fn correction_id_schema_carries_the_pattern() {
        let schema = schemars::schema_for!(CorrectionId);
        let json = serde_json::to_value(&schema).expect("serialize");
        assert_eq!(json["pattern"], "^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$");
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
}
