//! Exact provider document numbers used by reads and recovery evidence.

use serde::{Deserialize, Serialize};

/// An exact provider-issued document number for a read or recovery evidence.
///
/// Nonblank XML 1.0 text, preserved without trimming, case folding or Unicode
/// normalization. Whitespace and `:` are accepted. The provider's query XSD
/// declares an unrestricted string; no field-specific length limit is imposed.
/// Request-body limits still apply, and acceptance here does not guarantee that
/// szamlazz.hu can query every spelling or length.
///
/// Unlike [`super::InvoiceNumber`], this never becomes an external-id segment
/// or permission for a mutation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProviderDocumentNumber(String);

impl ProviderDocumentNumber {
    /// The provider's exact number; nothing is trimmed or normalized.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ProviderDocumentNumber {
    type Error = InvalidProviderDocumentNumber;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.trim().is_empty() {
            return Err(InvalidProviderDocumentNumber::Blank);
        }
        szamlazz_agent::wire::validate_xml_text(&value)?;
        Ok(Self(value))
    }
}

impl std::str::FromStr for ProviderDocumentNumber {
    type Err = InvalidProviderDocumentNumber;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.to_owned().try_into()
    }
}

impl From<ProviderDocumentNumber> for String {
    fn from(number: ProviderDocumentNumber) -> Self {
        number.0
    }
}

impl From<super::InvoiceNumber> for ProviderDocumentNumber {
    fn from(number: super::InvoiceNumber) -> Self {
        Self(number.into())
    }
}

impl AsRef<str> for ProviderDocumentNumber {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for ProviderDocumentNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A provider number that cannot identify a queryable document.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidProviderDocumentNumber {
    /// Empty or entirely Unicode whitespace.
    #[error("provider document number must not be blank")]
    Blank,
    /// A character cannot be represented in XML 1.0.
    #[error(transparent)]
    Xml(#[from] szamlazz_agent::RequestError),
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for ProviderDocumentNumber {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ProviderDocumentNumber".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "Exact provider document number: nonblank XML 1.0 text, no field-specific length bound or normalization. Request-body limits apply; provider acceptance is not guaranteed.",
            "minLength": 1,
            "pattern": r"[^\u0009-\u000D\u0020\u0085\u00A0\u1680\u2000-\u200A\u2028\u2029\u202F\u205F\u3000]",
            "not": {"pattern": r"[\u0000-\u0008\u000B\u000C\u000E-\u001F\uFFFE\uFFFF]"}
        })
    }
}
