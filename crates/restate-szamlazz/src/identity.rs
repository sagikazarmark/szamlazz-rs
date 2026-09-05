//! Identity of orders and documents: the `Order` key and the deterministic
//! external id (`szamlaKulsoAzon`).
//!
//! See §3 of the design document (ADR 0002, ADR 0005): the order key is the
//! trimmed order number, and the external id is derived from the key alone so
//! that *any* invocation can find what an earlier one issued.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization as _;

use crate::config::Namespace;
use crate::contract::{CorrectionId, DocumentKind};

/// The key of an `Order` Virtual Object: the order number (`rendelésszám`)
/// trimmed of leading and trailing whitespace, case preserved — exactly what
/// szamlazz.hu matches on.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OrderKey(String);

impl OrderKey {
    /// The maximum length in bytes after trimming.
    pub const MAX_LEN: usize = 64;

    /// Trims and validates an order number.
    ///
    /// # Errors
    ///
    /// Returns an error when the trimmed value is empty or longer than
    /// [`Self::MAX_LEN`] bytes, contains a control character, or contains two
    /// or more consecutive whitespace characters.
    pub fn parse(value: &str) -> Result<Self, InvalidOrderKey> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(InvalidOrderKey::Empty);
        }
        if trimmed.len() > Self::MAX_LEN {
            return Err(InvalidOrderKey::TooLong(trimmed.len()));
        }
        if let Some(control) = trimmed.chars().find(|c| c.is_control()) {
            return Err(InvalidOrderKey::ControlChar(control));
        }
        let mut previous_was_whitespace = false;
        for c in trimmed.chars() {
            if c.is_whitespace() && previous_was_whitespace {
                return Err(InvalidOrderKey::WhitespaceRun);
            }
            previous_was_whitespace = c.is_whitespace();
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// The key as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for OrderKey {
    type Err = InvalidOrderKey;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl TryFrom<String> for OrderKey {
    type Error = InvalidOrderKey;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl fmt::Display for OrderKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for OrderKey {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<OrderKey> for String {
    fn from(key: OrderKey) -> Self {
        key.0
    }
}

/// Serializes as the plain string.
impl Serialize for OrderKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// Deserializes from a string, trimming and validating it.
impl<'de> Deserialize<'de> for OrderKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// An order number that cannot be an [`OrderKey`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidOrderKey {
    /// Empty after trimming.
    #[error("order number must not be empty")]
    Empty,
    /// Longer than [`OrderKey::MAX_LEN`] bytes after trimming.
    #[error("order number is {0} bytes long, at most {max} are allowed", max = OrderKey::MAX_LEN)]
    TooLong(usize),
    /// Contains a control character.
    #[error("order number must not contain control characters, found {0:?}")]
    ControlChar(char),
    /// Contains two or more consecutive whitespace characters.
    #[error("order number must not contain consecutive whitespace")]
    WhitespaceRun,
}

/// The external id (`szamlaKulsoAzon`) of a document the service issues.
///
/// Deterministic from the order key alone under the deployment's
/// [`Namespace`]: `{namespace}:{order}:{kind}` for the four document kinds,
/// `{namespace}:{order}:corrective:{correction_id}` for correctives,
/// `{namespace}:{order}:storno:{original_number}` for a storno invoice and
/// `{namespace}:by-number:{number}:storno` for the storno of a document no
/// `Order` manages. Not unique server-side — a query returns the newest
/// holder — so every document found by it is validated before it is trusted.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExternalId(String);

impl ExternalId {
    /// Wraps an id.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The id of the `kind` document of `order`: `{namespace}:{order}:{kind}`.
    #[must_use]
    pub fn for_kind(namespace: &Namespace, order: &OrderKey, kind: DocumentKind) -> Self {
        Self(format!("{namespace}:{order}:{}", kind.as_str()))
    }

    /// The id of the corrective `id` of `order`:
    /// `{namespace}:{order}:corrective:{id}`.
    #[must_use]
    pub fn for_corrective(namespace: &Namespace, order: &OrderKey, id: &CorrectionId) -> Self {
        Self(format!("{namespace}:{order}:corrective:{id}"))
    }

    /// The id sent on the storno of `original_number`, a document of `order`:
    /// `{namespace}:{order}:storno:{original_number}`.
    #[must_use]
    pub fn for_storno(namespace: &Namespace, order: &OrderKey, original_number: &str) -> Self {
        Self(format!("{namespace}:{order}:storno:{original_number}"))
    }

    /// The id sent on the storno of `number`, a document no `Order` manages:
    /// `{namespace}:by-number:{number}:storno`.
    #[must_use]
    pub fn for_unmanaged_storno(namespace: &Namespace, number: &str) -> Self {
        Self(format!("{namespace}:by-number:{number}:storno"))
    }

    /// The id as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ExternalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for ExternalId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<ExternalId> for String {
    fn from(id: ExternalId) -> Self {
        id.0
    }
}

/// Normalises a buyer name the way it is sent to szamlazz.hu on every
/// attempt: trimmed and in Unicode NFC.
///
/// szamlazz.hu's replay check compares the buyer name byte-exact, so the
/// service normalises once at validation and serialises the result
/// identically on every attempt.
#[must_use]
pub fn normalize_buyer_name(name: &str) -> String {
    name.trim().nfc().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn namespace() -> Namespace {
        "acct".parse().expect("valid namespace")
    }

    #[test]
    fn order_key_table() {
        let accepted = [
            ("ORD-1", "ORD-1"),
            ("  ORD-1\t", "ORD-1"),
            ("a b", "a b"),
            ("Ord-1", "Ord-1"),
            ("rendelés #42", "rendelés #42"),
            (&"x".repeat(64), &"x".repeat(64)),
        ];
        for (input, expected) in accepted {
            let key = OrderKey::parse(input).expect(input);
            assert_eq!(key.as_str(), expected);
            assert_eq!(key.to_string(), expected);
            assert_eq!(input.parse::<OrderKey>(), Ok(key));
        }

        assert_ne!(
            OrderKey::parse("ord-1").expect("valid"),
            OrderKey::parse("ORD-1").expect("valid")
        );

        let too_long = "x".repeat(65);
        let rejected = [
            ("", InvalidOrderKey::Empty),
            ("   ", InvalidOrderKey::Empty),
            ("\t", InvalidOrderKey::Empty),
            ("a  b", InvalidOrderKey::WhitespaceRun),
            ("a \u{a0}b", InvalidOrderKey::WhitespaceRun),
            ("a\tb", InvalidOrderKey::ControlChar('\t')),
            ("a \tb", InvalidOrderKey::ControlChar('\t')),
            ("a\nb", InvalidOrderKey::ControlChar('\n')),
            ("a\u{7f}b", InvalidOrderKey::ControlChar('\u{7f}')),
            (too_long.as_str(), InvalidOrderKey::TooLong(65)),
        ];
        for (input, expected) in rejected {
            assert_eq!(OrderKey::parse(input), Err(expected), "{input:?}");
        }
    }

    #[test]
    fn order_key_serde_trims_and_validates() {
        let key: OrderKey = serde_json::from_str("\" ORD-1 \"").expect("valid");
        assert_eq!(key.as_str(), "ORD-1");
        assert_eq!(serde_json::to_string(&key).expect("serialize"), "\"ORD-1\"");
        assert!(serde_json::from_str::<OrderKey>("\"a  b\"").is_err());
    }

    #[test]
    fn external_id_formats() {
        let order = OrderKey::parse("ORD-1").expect("valid");
        assert_eq!(
            ExternalId::for_kind(&namespace(), &order, DocumentKind::Proforma).as_str(),
            "acct:ORD-1:proforma"
        );
        assert_eq!(
            ExternalId::for_kind(&namespace(), &order, DocumentKind::Invoice).to_string(),
            "acct:ORD-1:invoice"
        );
        assert_eq!(
            ExternalId::for_kind(&namespace(), &order, DocumentKind::Prepayment).as_ref(),
            "acct:ORD-1:prepayment"
        );
        assert_eq!(
            String::from(ExternalId::for_kind(
                &namespace(),
                &order,
                DocumentKind::Final
            )),
            "acct:ORD-1:final"
        );
        let correction: CorrectionId = "c-3".parse().expect("valid correction id");
        assert_eq!(
            ExternalId::for_corrective(&namespace(), &order, &correction).as_str(),
            "acct:ORD-1:corrective:c-3"
        );
        assert_eq!(
            ExternalId::for_storno(&namespace(), &order, "SZ-1").as_str(),
            "acct:ORD-1:storno:SZ-1"
        );
        assert_eq!(
            ExternalId::for_unmanaged_storno(&namespace(), "SZ-9").as_str(),
            "acct:by-number:SZ-9:storno"
        );
        assert_eq!(
            ExternalId::new("acct:ORD-1:invoice"),
            ExternalId::for_kind(&namespace(), &order, DocumentKind::Invoice)
        );
        let json = serde_json::to_string(&ExternalId::new("x:y:invoice")).expect("serialize");
        assert_eq!(json, "\"x:y:invoice\"");
    }

    #[test]
    fn buyer_name_normalisation() {
        assert_eq!(normalize_buyer_name("Próba Kft. "), "Próba Kft.");
        assert_eq!(
            normalize_buyer_name("Pro\u{301}ba Kft."),
            normalize_buyer_name("Pr\u{f3}ba Kft.")
        );
        assert_eq!(normalize_buyer_name("Pro\u{301}ba"), "Pr\u{f3}ba");
        assert_ne!(normalize_buyer_name("kft."), normalize_buyer_name("Kft."));
        assert_eq!(normalize_buyer_name("  a  b  "), "a  b");
    }
}
