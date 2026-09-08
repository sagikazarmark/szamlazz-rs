//! Identity of orders and documents: the `Order` key and the deterministic
//! external id (`szamlaKulsoAzon`).
//!
//! The order key is the trimmed order number, and the external id is derived
//! from the key alone so that *any* invocation can find what an earlier one
//! issued.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use unicode_normalization::{UnicodeNormalization as _, is_nfc};

use crate::config::Namespace;
use crate::contract::{CorrectionId, DocumentKind, InvoiceNumber, IssuedKind};

/// The key of an `Order` Virtual Object: the order number (`rendelésszám`)
/// trimmed of leading and trailing whitespace, case preserved, exactly what
/// szamlazz.hu matches on.
///
/// The alphabet is strict where the server's behaviour is unverified (ADR
/// 0002): 1–[`MAX_LEN`](Self::MAX_LEN) bytes after trimming, no control
/// character, no internal whitespace of any kind, no `:` (the external-id
/// separator) and Unicode NFC. Nothing is case-folded or normalised: a key
/// outside the alphabet is refused, never rewritten, because a `rendelesszam`
/// the server stored differently from the key would strand the order behind
/// `conflict{external_id_collision}`.
///
/// [`parse`](Self::parse) trims, so the type accepts an order number however
/// it is written. The `Order` handlers do not: a Virtual Object key that is
/// not already trimmed is refused as `invalid_input`, because Restate's
/// per-key lock is on the raw key and ` ORD-1` would be a second instance of
/// `ORD-1`'s order.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OrderKey(String);

impl OrderKey {
    /// The maximum length in bytes after trimming: a dashed UUID (36) fits,
    /// and the longest external id the key composes stays within
    /// [`ExternalId::MAX_LEN`].
    pub const MAX_LEN: usize = 40;

    /// Trims and validates an order number.
    ///
    /// # Errors
    ///
    /// Returns an error when the trimmed value is empty or longer than
    /// [`Self::MAX_LEN`] bytes, contains a control character, contains
    /// whitespace, contains `:`, or is not in Unicode NFC.
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
        if let Some(whitespace) = trimmed.chars().find(|c| c.is_whitespace()) {
            return Err(InvalidOrderKey::InternalWhitespace(whitespace));
        }
        if trimmed.contains(ExternalId::SEPARATOR) {
            return Err(InvalidOrderKey::Separator);
        }
        if !is_nfc(trimmed) {
            return Err(InvalidOrderKey::NotNfc);
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
    /// Contains whitespace after trimming: a space, an NBSP or any other
    /// `White_Space` character between the first and the last non-whitespace
    /// one. Refused rather than collapsed: the server's handling is
    /// unverified.
    #[error("order number must not contain whitespace, found {0:?}")]
    InternalWhitespace(char),
    /// Contains `:`, the separator of the external id's segments.
    #[error("order number must not contain ':' (the external-id separator)")]
    Separator,
    /// Not in Unicode NFC. Refused rather than normalised: the server's
    /// handling is unverified, and a key it stored differently would strand
    /// the order.
    #[error("order number must be in Unicode NFC (normalise it before calling)")]
    NotNfc,
}

/// The external id (`szamlaKulsoAzon`) the service sends or queries.
///
/// Deterministic from the order key alone under the deployment's
/// [`Namespace`]: `{namespace}:{order}:{kind}` for the four document kinds,
/// `{namespace}:{order}:corrective:{correction_id}` for correctives,
/// `{namespace}:{order}:storno:{original_number}` for a storno invoice,
/// `{namespace}:by-number:{number}:storno` for the storno of a document no
/// `Order` manages, and the two-segment `{namespace}:check-account` sentinel
/// that `check_account` probes and nothing the service issues carries. Not
/// unique server-side (a query returns the newest holder), so every document
/// found by it is validated before it is trusted.
///
/// Every composition stays within [`MAX_LEN`](Self::MAX_LEN) bytes because
/// its parts are bounded (the namespace at 16, the [`OrderKey`], the
/// [`CorrectionId`] and the [`InvoiceNumber`] at 40 each), which the
/// compile-time assertions below prove for the longest shape of each.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExternalId(String);

impl ExternalId {
    /// The longest id the service composes, in bytes: 110, the length
    /// verified accepted and queryable on szamlazz.hu (behaviour notes,
    /// A2-create-long). szamlazz.hu documents no limit; a longer id might be
    /// truncated or refused, and a truncated id would make every leading
    /// query by the full id answer "not found", so the parts are bounded to
    /// keep every composition inside the verified length.
    pub const MAX_LEN: usize = 110;

    /// The character between an id's segments. Excluded from every
    /// caller-supplied segment (the [`Namespace`], the [`OrderKey`], the
    /// [`CorrectionId`] and the invoice number), so no composition can read
    /// as another.
    pub const SEPARATOR: char = ':';

    /// The segment token of a storno's id (`…:storno:{number}`,
    /// `…:{number}:storno`).
    pub const STORNO: &'static str = "storno";
    /// The segment token in place of an order for a document no `Order`
    /// manages (`{namespace}:by-number:{number}:storno`).
    pub const BY_NUMBER: &'static str = "by-number";
    /// The segment token of the probe sentinel (`{namespace}:check-account`).
    pub const CHECK_ACCOUNT: &'static str = "check-account";

    /// Every fixed token an id is composed of: the five [`IssuedKind`] tokens
    /// and the three above. A [`CorrectionId`] equal to one of them is
    /// refused, so `{namespace}:{order}:corrective:{id}` never reads as
    /// another composition.
    pub const TOKENS: [&'static str; 8] = [
        IssuedKind::Proforma.as_str(),
        IssuedKind::Invoice.as_str(),
        IssuedKind::Prepayment.as_str(),
        IssuedKind::Final.as_str(),
        IssuedKind::Corrective.as_str(),
        Self::STORNO,
        Self::BY_NUMBER,
        Self::CHECK_ACCOUNT,
    ];

    /// Whether `value` is one of [`TOKENS`](Self::TOKENS), in any letter
    /// case.
    #[must_use]
    pub fn is_token(value: &str) -> bool {
        Self::TOKENS
            .iter()
            .any(|token| token.eq_ignore_ascii_case(value))
    }

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
        Self(format!(
            "{namespace}:{order}:{}:{id}",
            IssuedKind::Corrective.as_str()
        ))
    }

    /// The id sent on the storno of `original_number`, a document of `order`:
    /// `{namespace}:{order}:storno:{original_number}`.
    #[must_use]
    pub fn for_storno(namespace: &Namespace, order: &OrderKey, original_number: &str) -> Self {
        Self(format!(
            "{namespace}:{order}:{}:{original_number}",
            Self::STORNO
        ))
    }

    /// The id sent on the storno of `number`, a document no `Order` manages:
    /// `{namespace}:by-number:{number}:storno`.
    #[must_use]
    pub fn for_unmanaged_storno(namespace: &Namespace, number: &str) -> Self {
        Self(format!(
            "{namespace}:{}:{number}:{}",
            Self::BY_NUMBER,
            Self::STORNO
        ))
    }

    /// The sentinel id `Szamlazz.Agent.check_account` queries:
    /// `{namespace}:check-account`. Two segments, where every id the service
    /// issues has at least three: nothing the service issues carries it, so
    /// the expected answer is "not found" and the query proves only that the
    /// credentials were accepted.
    #[must_use]
    pub fn for_probe(namespace: &Namespace) -> Self {
        Self(format!("{namespace}:{}", Self::CHECK_ACCOUNT))
    }

    /// The id as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The longest shape of each composition fits [`ExternalId::MAX_LEN`]: proven
/// at compile time from the parts' bounds, so a raised bound on any part fails
/// the build until the budget is re-balanced. The byte counts of the fixed
/// tokens are spelled out beside the token they count.
const _: () = {
    const SEP: usize = 1; // ':'
    const NAMESPACE: usize = Namespace::MAX_LEN;
    const ORDER: usize = OrderKey::MAX_LEN;
    const CORRECTION: usize = CorrectionId::MAX_LEN;
    const NUMBER: usize = InvoiceNumber::MAX_LEN;
    const PREPAYMENT: usize = IssuedKind::Prepayment.as_str().len(); // the longest kind token
    const CORRECTIVE: usize = IssuedKind::Corrective.as_str().len();
    const STORNO: usize = ExternalId::STORNO.len();
    const BY_NUMBER: usize = ExternalId::BY_NUMBER.len();
    const CHECK_ACCOUNT: usize = ExternalId::CHECK_ACCOUNT.len();

    assert!(PREPAYMENT >= IssuedKind::Proforma.as_str().len());
    assert!(PREPAYMENT >= IssuedKind::Invoice.as_str().len());
    assert!(PREPAYMENT >= IssuedKind::Final.as_str().len());

    // {namespace}:{order}:{kind}
    assert!(NAMESPACE + SEP + ORDER + SEP + PREPAYMENT <= ExternalId::MAX_LEN);
    // {namespace}:{order}:corrective:{correction_id}
    assert!(NAMESPACE + SEP + ORDER + SEP + CORRECTIVE + SEP + CORRECTION <= ExternalId::MAX_LEN);
    // {namespace}:{order}:storno:{number}
    assert!(NAMESPACE + SEP + ORDER + SEP + STORNO + SEP + NUMBER <= ExternalId::MAX_LEN);
    // {namespace}:by-number:{number}:storno
    assert!(NAMESPACE + SEP + BY_NUMBER + SEP + NUMBER + SEP + STORNO <= ExternalId::MAX_LEN);
    // {namespace}:check-account
    assert!(NAMESPACE + SEP + CHECK_ACCOUNT <= ExternalId::MAX_LEN);
};

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
/// execution: trimmed and in Unicode NFC.
///
/// szamlazz.hu's replay check compares the buyer name byte-exact, so the
/// service normalises once at validation and serialises the result
/// identically on every execution.
#[must_use]
pub(crate) fn normalize_buyer_name(name: &str) -> String {
    name.trim().nfc().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn namespace() -> Namespace {
        "acct".parse().expect("valid namespace")
    }

    /// The key's alphabet: trimmed, 1–40 bytes, case preserved, no control
    /// character, **no internal whitespace of any kind** (a space, an NBSP:
    /// the server's handling is unverified, and a normalised `rendelesszam`
    /// would strand the order behind `external_id_collision`), no `:` (the
    /// external-id separator) and NFC (not normalised: refused, like the
    /// whitespace). Non-ASCII text in NFC is fine.
    #[test]
    fn order_key_table() {
        let longest = "x".repeat(OrderKey::MAX_LEN);
        let uuid = "123e4567-e89b-12d3-a456-426614174000";
        let accepted = [
            ("ORD-1", "ORD-1"),
            ("  ORD-1\t", "ORD-1"),
            ("Ord-1", "Ord-1"),
            ("rendelés-42", "rendelés-42"),
            ("democon-2026-ABC12", "democon-2026-ABC12"),
            ("ORD/2026/0001#a.b_c", "ORD/2026/0001#a.b_c"),
            (uuid, uuid),
            (longest.as_str(), longest.as_str()),
        ];
        for (input, expected) in accepted {
            let key = OrderKey::parse(input).expect(input);
            assert_eq!(key.as_str(), expected);
            assert_eq!(key.to_string(), expected);
            assert_eq!(input.parse::<OrderKey>(), Ok(key));
        }
        assert_eq!(OrderKey::MAX_LEN, 40, "a dashed UUID (36) fits with room");

        assert_ne!(
            OrderKey::parse("ord-1").expect("valid"),
            OrderKey::parse("ORD-1").expect("valid")
        );

        let too_long = "x".repeat(OrderKey::MAX_LEN + 1);
        let rejected = [
            ("", InvalidOrderKey::Empty),
            ("   ", InvalidOrderKey::Empty),
            ("\t", InvalidOrderKey::Empty),
            ("a b", InvalidOrderKey::InternalWhitespace(' ')),
            ("rendelés #42", InvalidOrderKey::InternalWhitespace(' ')),
            ("a  b", InvalidOrderKey::InternalWhitespace(' ')),
            ("a\u{a0}b", InvalidOrderKey::InternalWhitespace('\u{a0}')),
            (
                "a\u{2003}b",
                InvalidOrderKey::InternalWhitespace('\u{2003}'),
            ),
            ("a\tb", InvalidOrderKey::ControlChar('\t')),
            ("a\nb", InvalidOrderKey::ControlChar('\n')),
            ("a\u{7f}b", InvalidOrderKey::ControlChar('\u{7f}')),
            ("ORD:1", InvalidOrderKey::Separator),
            (":", InvalidOrderKey::Separator),
            ("ORD-1:invoice", InvalidOrderKey::Separator),
            ("rendele\u{301}s-42", InvalidOrderKey::NotNfc),
            (too_long.as_str(), InvalidOrderKey::TooLong(41)),
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
        assert!(serde_json::from_str::<OrderKey>("\"a b\"").is_err());
        assert!(serde_json::from_str::<OrderKey>("\"a:b\"").is_err());
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

    /// Every composable external id stays within [`ExternalId::MAX_LEN`],
    /// the 110 bytes verified accepted and queryable on szamlazz.hu
    /// (behaviour notes A2-create-long; the real limit is unknown, probe #5
    /// of the 2026-09-06 review). Built from the longest legal parts: a
    /// 16-byte namespace, a 40-byte order key, a 40-byte correction id, a
    /// 40-byte invoice number.
    #[test]
    fn the_longest_composable_external_id_fits_the_verified_length() {
        let namespace: Namespace = "n".repeat(Namespace::MAX_LEN).parse().expect("namespace");
        let order = OrderKey::parse(&"o".repeat(OrderKey::MAX_LEN)).expect("order");
        let correction: CorrectionId = "c".repeat(CorrectionId::MAX_LEN).parse().expect("id");
        let number = "s".repeat(InvoiceNumber::MAX_LEN);

        let longest_kind = DocumentKind::ALL
            .into_iter()
            .map(|kind| ExternalId::for_kind(&namespace, &order, kind))
            .max_by_key(|id| id.as_str().len())
            .expect("four kinds");
        let ids = [
            ("kind", longest_kind),
            (
                "corrective",
                ExternalId::for_corrective(&namespace, &order, &correction),
            ),
            (
                "storno",
                ExternalId::for_storno(&namespace, &order, &number),
            ),
            (
                "unmanaged storno",
                ExternalId::for_unmanaged_storno(&namespace, &number),
            ),
            ("probe", ExternalId::for_probe(&namespace)),
        ];
        for (shape, id) in &ids {
            assert!(
                id.as_str().len() <= ExternalId::MAX_LEN,
                "{shape}: {} bytes > {}: {id}",
                id.as_str().len(),
                ExternalId::MAX_LEN
            );
        }
        assert_eq!(ExternalId::MAX_LEN, 110, "the verified length");
        // The worked example: the corrective is the longest shape.
        assert_eq!(ids[1].1.as_str().len(), 16 + 1 + 40 + 1 + 10 + 1 + 40);
        assert_eq!(ids[1].1.as_str().len(), 109);
        assert_eq!(ids[2].1.as_str().len(), 16 + 1 + 40 + 1 + 6 + 1 + 40);
        assert_eq!(ids[3].1.as_str().len(), 16 + 1 + 9 + 1 + 40 + 1 + 6);
        assert_eq!(
            ids[0].1.as_str().len(),
            16 + 1 + 40 + 1 + "prepayment".len()
        );
    }

    /// The probe id has two segments; every id the service issues has at
    /// least three (`{namespace}:{order}:{kind}`), so nothing the service
    /// issues carries it.
    #[test]
    fn the_probe_id_cannot_be_an_issued_documents_id() {
        let probe = ExternalId::for_probe(&namespace());
        assert_eq!(probe.as_str(), "acct:check-account");
        assert_eq!(probe.as_str().split(':').count(), 2);
        let order = OrderKey::parse("check-account").expect("valid");
        assert!(
            ExternalId::for_kind(&namespace(), &order, DocumentKind::Invoice)
                .as_str()
                .split(':')
                .count()
                >= 3
        );
        assert_ne!(
            probe,
            ExternalId::for_kind(&namespace(), &order, DocumentKind::Invoice)
        );
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
