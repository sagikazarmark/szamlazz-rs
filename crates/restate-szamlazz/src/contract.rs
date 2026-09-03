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
//! - [`response`] — handler outputs, including the [`OrderSnapshot`]
//!   projection of the ledger.

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
    CorrectRequest, CreateOptions, CreateRequest, DeleteProformaRequest, ForgetRequest, GetRequest,
    PaymentEntry, ProformaLink, QueryRequest, RecordReversalRequest, RecordedReversal, Selector,
    SetPaymentsRequest, StornoRequest,
};
pub use response::{
    ConflictReason, CorrectiveSnapshot, CreateResponse, DeleteProformaResponse,
    DocumentVerification, ForeignHint, Freshness, HistorySnapshot, OrderSnapshot, Outcome,
    PaymentRecord, QueryResponse, SetPaymentsResponse, SlotSnapshot, SlotsSnapshot, StornoOutcome,
    StornoResponse, VerificationResult, Warning,
};

/// The caller-supplied retry identity of an issuing request.
///
/// The same id returns the entry's current state forever; a different id is a
/// new logical request; a known id with a different payload is
/// `conflict{payload_mismatch}`. It lives in the ledger only and is never sent
/// to szamlazz.hu.
///
/// Valid ids match `^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequestId(String);

impl RequestId {
    /// The maximum length in bytes (the id is ASCII, so also in characters).
    pub const MAX_LEN: usize = 64;

    /// The id as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(value: &str) -> Result<(), InvalidRequestId> {
        let mut chars = value.chars();
        let Some(first) = chars.next() else {
            return Err(InvalidRequestId::Empty);
        };
        if value.len() > Self::MAX_LEN {
            return Err(InvalidRequestId::TooLong(value.len()));
        }
        if !first.is_ascii_alphanumeric() {
            return Err(InvalidRequestId::InvalidStart(first));
        }
        if let Some(invalid) =
            chars.find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')))
        {
            return Err(InvalidRequestId::InvalidChar(invalid));
        }
        Ok(())
    }
}

impl FromStr for RequestId {
    type Err = InvalidRequestId;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::validate(value)?;
        Ok(Self(value.to_owned()))
    }
}

impl TryFrom<String> for RequestId {
    type Error = InvalidRequestId;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::validate(&value)?;
        Ok(Self(value))
    }
}

impl TryFrom<&str> for RequestId {
    type Error = InvalidRequestId;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for RequestId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<RequestId> for String {
    fn from(id: RequestId) -> Self {
        id.0
    }
}

/// Serializes as the plain string.
impl Serialize for RequestId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// Deserializes from a string, rejecting ids that do not match the pattern.
impl<'de> Deserialize<'de> for RequestId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for RequestId {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "RequestId".into()
    }

    fn schema_id() -> std::borrow::Cow<'static, str> {
        concat!(module_path!(), "::RequestId").into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "Caller-supplied retry identity of an issuing request.",
            "pattern": "^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$",
        })
    }
}

/// A string that is not a valid [`RequestId`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidRequestId {
    /// The id is empty.
    #[error("request id must not be empty")]
    Empty,
    /// The id exceeds [`RequestId::MAX_LEN`] bytes.
    #[error("request id is {0} bytes long, at most {max} are allowed", max = RequestId::MAX_LEN)]
    TooLong(usize),
    /// The first character is not an ASCII letter or digit.
    #[error("request id must start with an ASCII letter or digit, found {0:?}")]
    InvalidStart(char),
    /// A later character is outside `[A-Za-z0-9._-]`.
    #[error("request id may only contain ASCII letters, digits, '.', '_' and '-', found {0:?}")]
    InvalidChar(char),
}

/// A document kind that owns a ledger slot: exactly one slot per kind and
/// order.
///
/// Correctives are not slots (an order may carry any number of them); see
/// [`IssuedKind`] for the kind of an issued document.
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
    /// Every slot kind, in ledger order.
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

/// The kind of a document the service issued: the four slot kinds plus
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

    /// The slot kind, or `None` for a corrective.
    #[must_use]
    pub const fn slot_kind(self) -> Option<DocumentKind> {
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

/// The code of a `TerminalError` raised by an issuing or storno handler.
///
/// Every one of them means "outcome unknown — call again with the same
/// [`RequestId`], or read `Szamlazz.Order.get`", never "no document exists".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TerminalCode {
    /// The attempt budget is exhausted while a document may or may not have
    /// been issued; the slot stays `pending`.
    OutcomeUnknown,
    /// szamlazz.hu could not be reached for a check that must succeed before
    /// anything is issued.
    Unavailable,
    /// A document found under our identity belongs to a different szamlazz.hu
    /// account (`szallito/id` differs from the recorded supplier id).
    AccountMismatch,
    /// The request is malformed or contradicts the ledger (for example
    /// `reissue: true` with a known request id).
    InvalidInput,
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
    fn request_id_accepts_valid_ids() {
        for id in [
            "a",
            "0",
            "r-2",
            "order.42_retry-1",
            "A",
            &"x".repeat(RequestId::MAX_LEN),
        ] {
            let parsed: RequestId = id.parse().expect(id);
            assert_eq!(parsed.as_str(), id);
            assert_eq!(parsed.to_string(), id);
        }
    }

    #[test]
    fn request_id_rejects_invalid_ids() {
        let too_long = "x".repeat(RequestId::MAX_LEN + 1);
        let cases = [
            ("", InvalidRequestId::Empty),
            ("-a", InvalidRequestId::InvalidStart('-')),
            (".a", InvalidRequestId::InvalidStart('.')),
            ("a b", InvalidRequestId::InvalidChar(' ')),
            ("a/b", InvalidRequestId::InvalidChar('/')),
            ("á", InvalidRequestId::InvalidStart('á')),
            ("aá", InvalidRequestId::InvalidChar('á')),
            (too_long.as_str(), InvalidRequestId::TooLong(65)),
        ];
        for (input, expected) in cases {
            assert_eq!(
                input.parse::<RequestId>(),
                Err(expected.clone()),
                "{input:?}"
            );
            assert_eq!(RequestId::try_from(input.to_owned()), Err(expected));
        }
    }

    #[test]
    fn request_id_serde_validates() {
        let id: RequestId = serde_json::from_str("\"r-1\"").expect("valid");
        assert_eq!(id.as_str(), "r-1");
        assert_eq!(serde_json::to_string(&id).expect("serialize"), "\"r-1\"");
        assert!(serde_json::from_str::<RequestId>("\"-r\"").is_err());
        assert!(serde_json::from_str::<RequestId>("\"\"").is_err());
    }

    #[test]
    fn request_id_orders_as_string() {
        let a: RequestId = "a".parse().expect("valid");
        let b: RequestId = "b".parse().expect("valid");
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
            assert_eq!(IssuedKind::from(kind).slot_kind(), Some(kind));
            assert_eq!(IssuedKind::from(kind).as_str(), kind.as_str());
        }
        assert_eq!(IssuedKind::Corrective.slot_kind(), None);
    }

    #[test]
    fn terminal_code_tokens() {
        for code in [
            TerminalCode::OutcomeUnknown,
            TerminalCode::Unavailable,
            TerminalCode::AccountMismatch,
            TerminalCode::InvalidInput,
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
    fn request_id_schema_carries_the_pattern() {
        let schema = schemars::schema_for!(RequestId);
        let json = serde_json::to_value(&schema).expect("serialize");
        assert_eq!(json["pattern"], "^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$");
    }

    #[cfg(feature = "schemars")]
    #[test]
    fn contract_types_have_schemas() {
        let schema = schemars::schema_for!(CreateRequest);
        let json = serde_json::to_value(&schema).expect("serialize");
        assert_eq!(json["title"], "CreateRequest");
        assert!(json["properties"]["request_id"].is_object());
        assert!(json["$defs"]["RequestId"].is_object());
        assert!(json["$defs"]["DocumentInput"].is_object());

        for schema in [
            schemars::schema_for!(CorrectRequest),
            schemars::schema_for!(StornoRequest),
            schemars::schema_for!(DeleteProformaRequest),
            schemars::schema_for!(GetRequest),
            schemars::schema_for!(RecordReversalRequest),
            schemars::schema_for!(ForgetRequest),
            schemars::schema_for!(QueryRequest),
            schemars::schema_for!(SetPaymentsRequest),
            schemars::schema_for!(CreateResponse),
            schemars::schema_for!(StornoResponse),
            schemars::schema_for!(DeleteProformaResponse),
            schemars::schema_for!(SetPaymentsResponse),
            schemars::schema_for!(QueryResponse),
            schemars::schema_for!(OrderSnapshot),
        ] {
            serde_json::to_string(&schema).expect("schema serializes");
        }

        let snapshot =
            serde_json::to_value(schemars::schema_for!(OrderSnapshot)).expect("serialize");
        assert!(snapshot["$defs"]["SlotsSnapshot"]["properties"]["final"].is_object());
        assert!(snapshot["$defs"]["SlotSnapshot"]["properties"]["gen"].is_object());
    }
}
