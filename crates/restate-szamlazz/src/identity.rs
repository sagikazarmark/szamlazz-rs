//! Identity of orders and documents: the `Order` key, the deterministic
//! external id (`szamlaKulsoAzon`) and the payload fingerprint.
//!
//! See §3 of the design document (ADR 0002): the order key is the trimmed
//! order number, the external id is derived from ledger state before the
//! first szamlazz.hu call, and the fingerprint detects caller payload drift
//! on a repeated request id.

use std::fmt;
use std::str::FromStr;

use hmac::{Hmac, KeyInit as _, Mac as _};
use jiff::civil::Date;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use unicode_normalization::UnicodeNormalization as _;

use crate::config::AccountSlug;
use crate::contract::DocumentKind;

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
/// Deterministic from ledger state: `{slug}:{order}:{kind}:{gen}` for slot
/// kinds, `{slug}:{order}:corrective:{cseq}` for correctives, and the
/// original's id suffixed with `:storno` for a storno invoice. Not unique
/// server-side, so every document found by it is validated before adoption.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExternalId(String);

impl ExternalId {
    /// Wraps an id read back from the ledger.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The id of generation `generation` of the `kind` slot of `order`.
    #[must_use]
    pub fn for_slot(
        slug: &AccountSlug,
        order: &OrderKey,
        kind: DocumentKind,
        generation: u32,
    ) -> Self {
        Self(format!("{slug}:{order}:{}:{generation}", kind.as_str()))
    }

    /// The id of the corrective with sequence number `cseq` of `order`.
    #[must_use]
    pub fn for_corrective(slug: &AccountSlug, order: &OrderKey, cseq: u32) -> Self {
        Self(format!("{slug}:{order}:corrective:{cseq}"))
    }

    /// The id sent on the storno of the document this id identifies.
    #[must_use]
    pub fn storno_of(&self) -> Self {
        Self(format!("{}:storno", self.0))
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
/// service normalises once at validation and journals the result.
#[must_use]
pub fn normalize_buyer_name(name: &str) -> String {
    name.trim().nfc().collect()
}

/// The fingerprint of an issuing request's payload: HMAC-SHA256 over the
/// normalised buyer name, the computed gross total and the caller-supplied
/// dates, hex-encoded.
///
/// Used solely to detect caller payload drift on a repeated request id
/// (`conflict{payload_mismatch}`); it is not a model of the server's replay
/// check. The ledger stores it instead of buyer data.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Fingerprint(String);

impl Fingerprint {
    /// Computes the fingerprint.
    ///
    /// The canonical input is `name\n{gross}\n{issue|-}\n{due|-}\n{fulfil|-}`
    /// with `gross` normalised (no trailing zeros, so `100` and `100.00`
    /// agree) and each date as `YYYY-MM-DD` or `-` when absent.
    #[must_use]
    #[expect(
        clippy::missing_panics_doc,
        reason = "HMAC accepts keys of any length; the Result is always Ok"
    )]
    pub fn compute(
        secret: &[u8],
        normalized_name: &str,
        gross: Decimal,
        issue: Option<Date>,
        due: Option<Date>,
        fulfil: Option<Date>,
    ) -> Self {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(secret).expect("HMAC accepts keys of any length");
        mac.update(normalized_name.as_bytes());
        mac.update(b"\n");
        mac.update(gross.normalize().to_string().as_bytes());
        for date in [issue, due, fulfil] {
            mac.update(b"\n");
            match date {
                Some(date) => mac.update(date.to_string().as_bytes()),
                None => mac.update(b"-"),
            }
        }
        Self(hex(&mac.finalize().into_bytes()))
    }

    /// The fingerprint as a hex string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Fingerprint {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    out
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use rust_decimal::dec;

    use super::*;

    fn slug() -> AccountSlug {
        "acct".parse().expect("valid slug")
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
            ExternalId::for_slot(&slug(), &order, DocumentKind::Proforma, 0).as_str(),
            "acct:ORD-1:proforma:0"
        );
        assert_eq!(
            ExternalId::for_slot(&slug(), &order, DocumentKind::Invoice, 2).to_string(),
            "acct:ORD-1:invoice:2"
        );
        assert_eq!(
            ExternalId::for_slot(&slug(), &order, DocumentKind::Prepayment, 1).as_ref(),
            "acct:ORD-1:prepayment:1"
        );
        assert_eq!(
            String::from(ExternalId::for_slot(
                &slug(),
                &order,
                DocumentKind::Final,
                0
            )),
            "acct:ORD-1:final:0"
        );
        assert_eq!(
            ExternalId::for_corrective(&slug(), &order, 3).as_str(),
            "acct:ORD-1:corrective:3"
        );
        assert_eq!(
            ExternalId::for_slot(&slug(), &order, DocumentKind::Invoice, 0)
                .storno_of()
                .as_str(),
            "acct:ORD-1:invoice:0:storno"
        );
        assert_eq!(
            ExternalId::new("acct:ORD-1:invoice:0"),
            ExternalId::for_slot(&slug(), &order, DocumentKind::Invoice, 0)
        );
        let json = serde_json::to_string(&ExternalId::new("x:y:invoice:0")).expect("serialize");
        assert_eq!(json, "\"x:y:invoice:0\"");
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

    #[test]
    fn fingerprint_is_stable_and_sensitive() {
        let secret = b"fp-secret";
        let base = Fingerprint::compute(
            secret,
            "Próba Kft.",
            dec!(25400),
            None,
            Some(date(2026, 7, 12)),
            Some(date(2026, 7, 4)),
        );
        let again = Fingerprint::compute(
            secret,
            "Próba Kft.",
            dec!(25400.00),
            None,
            Some(date(2026, 7, 12)),
            Some(date(2026, 7, 4)),
        );
        assert_eq!(base, again, "scale of the gross total must not matter");
        assert_eq!(base.as_str().len(), 64);
        assert!(base.as_str().bytes().all(|b| b.is_ascii_hexdigit()));

        let different = [
            Fingerprint::compute(
                b"other-secret",
                "Próba Kft.",
                dec!(25400),
                None,
                Some(date(2026, 7, 12)),
                Some(date(2026, 7, 4)),
            ),
            Fingerprint::compute(
                secret,
                "Próba Bt.",
                dec!(25400),
                None,
                Some(date(2026, 7, 12)),
                Some(date(2026, 7, 4)),
            ),
            Fingerprint::compute(
                secret,
                "Próba Kft.",
                dec!(25401),
                None,
                Some(date(2026, 7, 12)),
                Some(date(2026, 7, 4)),
            ),
            Fingerprint::compute(
                secret,
                "Próba Kft.",
                dec!(25400),
                Some(date(2026, 7, 4)),
                Some(date(2026, 7, 12)),
                Some(date(2026, 7, 4)),
            ),
            Fingerprint::compute(
                secret,
                "Próba Kft.",
                dec!(25400),
                None,
                Some(date(2026, 7, 4)),
                Some(date(2026, 7, 12)),
            ),
            Fingerprint::compute(secret, "Próba Kft.", dec!(25400), None, None, None),
        ];
        for other in &different {
            assert_ne!(&base, other);
        }
    }

    #[test]
    fn fingerprint_matches_reference_vector() {
        // HMAC-SHA256("k", "A\n1\n-\n-\n-") computed independently.
        let fp = Fingerprint::compute(b"k", "A", dec!(1), None, None, None);
        let mut mac = Hmac::<Sha256>::new_from_slice(b"k").expect("any key length");
        mac.update(b"A\n1\n-\n-\n-");
        assert_eq!(fp.as_str(), hex(&mac.finalize().into_bytes()));
        assert_eq!(
            serde_json::to_string(&fp).expect("serialize"),
            format!("\"{fp}\"")
        );
    }

    #[test]
    fn hex_encodes_lowercase() {
        assert_eq!(hex(&[0x00, 0x0f, 0xa5, 0xff]), "000fa5ff");
    }
}
