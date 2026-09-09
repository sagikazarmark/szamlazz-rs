//! Identity of orders and documents: the deployment's [`Namespace`], the
//! `Order` key, the document kinds, the caller-supplied [`CorrectionId`] and
//! [`InvoiceNumber`], and the deterministic external id (`szamlaKulsoAzon`)
//! composed of them.
//!
//! The order key is the trimmed order number, and the external id is derived
//! from the key alone under the namespace so that *any* invocation can find
//! what an earlier one issued. Every segment an external id is composed of is
//! defined here, bounded, and free of the separator, so the module proves the
//! id's length budget on its own and depends on nothing else in the crate:
//! `contract` re-exports the caller-facing types ([`CorrectionId`],
//! [`InvoiceNumber`], [`DocumentKind`], [`IssuedKind`]) and `config` holds
//! the [`Namespace`] in the deployment settings; neither is imported here.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use szamlazz_agent::DocumentType;
use unicode_normalization::{UnicodeNormalization as _, is_nfc};

/// The conversion set every bounded string newtype of the crate shares
/// (C-CONV), derived from its `FromStr` (the one validation): `TryFrom<&str>`
/// through it, and `Display`, `AsRef<str>` and `From<_> for String` giving
/// the text back unchanged. The type writes its `FromStr` and its
/// `TryFrom<String>` (so an owned string is validated without a copy) and
/// stamps the rest with this; `identity::tests` holds the four identity types
/// and `account`'s tests `Endpoint` to the whole set.
macro_rules! bounded_conversions {
    ($name:ident, $error:ty) => {
        impl TryFrom<&str> for $name {
            type Error = $error;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                value.parse()
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

pub(crate) use bounded_conversions;

/// The namespace: the external-id prefix of this deployment, 1–16 bytes of
/// `[a-z0-9-]`.
///
/// Chosen by the operator, opaque to szamlazz.hu and permanent: every
/// external id the deployment issues starts with it, so changing it would
/// hide every document issued so far. `:` is excluded because it is the
/// external-id separator.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Namespace(String);

impl Namespace {
    /// The maximum length in bytes.
    pub const MAX_LEN: usize = 16;

    /// The namespace as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(value: &str) -> Result<(), InvalidNamespace> {
        if value.is_empty() {
            return Err(InvalidNamespace::Empty);
        }
        if value.len() > Self::MAX_LEN {
            return Err(InvalidNamespace::TooLong(value.len()));
        }
        if let Some(invalid) = value
            .chars()
            .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-'))
        {
            return Err(InvalidNamespace::InvalidChar(invalid));
        }
        Ok(())
    }
}

impl FromStr for Namespace {
    type Err = InvalidNamespace;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::validate(value)?;
        Ok(Self(value.to_owned()))
    }
}

impl TryFrom<String> for Namespace {
    type Error = InvalidNamespace;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::validate(&value)?;
        Ok(Self(value))
    }
}

bounded_conversions!(Namespace, InvalidNamespace);

/// Serializes as the plain string.
impl Serialize for Namespace {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// Deserializes from a string, rejecting invalid namespaces.
impl<'de> Deserialize<'de> for Namespace {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// A string that is not a valid [`Namespace`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidNamespace {
    /// The namespace is empty.
    #[error("namespace must not be empty")]
    Empty,
    /// The namespace exceeds [`Namespace::MAX_LEN`] bytes.
    #[error("namespace is {0} bytes long, at most {max} are allowed", max = Namespace::MAX_LEN)]
    TooLong(usize),
    /// A character is outside `[a-z0-9-]`.
    #[error("namespace may only contain lowercase ASCII letters, digits and '-', found {0:?}")]
    InvalidChar(char),
}

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

bounded_conversions!(OrderKey, InvalidOrderKey);

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

    /// The `tipus` the documents of this kind carry on szamlazz.hu.
    #[must_use]
    pub const fn document_type(self) -> DocumentType {
        match self {
            Self::Proforma => DocumentType::Proforma,
            Self::Invoice => DocumentType::Invoice,
            Self::Prepayment => DocumentType::Prepayment,
            Self::Final => DocumentType::Final,
            Self::Corrective => DocumentType::Corrective,
        }
    }

    /// The kind whose documents carry `document_type`, or `None` for a
    /// storno, a delivery note and a code the agent crate does not know:
    /// nothing the worker issues.
    #[must_use]
    pub fn for_document_type(document_type: &DocumentType) -> Option<Self> {
        match document_type {
            DocumentType::Proforma => Some(Self::Proforma),
            DocumentType::Invoice => Some(Self::Invoice),
            DocumentType::Prepayment => Some(Self::Prepayment),
            DocumentType::Final => Some(Self::Final),
            DocumentType::Corrective => Some(Self::Corrective),
            // A storno, a delivery note, an unknown code, and a code the agent
            // crate learns later: nothing the worker issues.
            _ => None,
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

bounded_conversions!(CorrectionId, InvalidCorrectionId);

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
#[cfg_attr(docsrs, doc(cfg(feature = "schemars")))]
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
/// take it: `Szamlazz.Agent.query`'s selector, `set_credit_entries`, `storno`,
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

bounded_conversions!(InvoiceNumber, InvalidInvoiceNumber);

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
#[cfg_attr(docsrs, doc(cfg(feature = "schemars")))]
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

    /// Every bounded newtype of the crate implements one conversion set
    /// (C-CONV): `FromStr`, `TryFrom<&str>`, `TryFrom<String>` (the three
    /// through the same validation), `Display`, `AsRef<str>`, `as_str` and
    /// `From<_> for String` (the four giving the text back unchanged), so a
    /// caller reads any of them the same way. `Endpoint` (a URL, not a bounded
    /// string) is held to the same set in `account`'s tests.
    #[test]
    fn the_bounded_newtypes_share_one_conversion_set() {
        fn assert_set<T, E>(valid: &str, invalid: &str)
        where
            T: FromStr<Err = E>
                + for<'a> TryFrom<&'a str, Error = E>
                + TryFrom<String, Error = E>
                + fmt::Display
                + AsRef<str>
                + Into<String>
                + PartialEq
                + fmt::Debug,
            E: fmt::Debug + PartialEq,
        {
            let parsed: T = valid.parse().unwrap_or_else(|e| panic!("{valid}: {e:?}"));
            assert_eq!(T::try_from(valid).expect("TryFrom<&str>"), parsed);
            assert_eq!(
                T::try_from(valid.to_owned()).expect("TryFrom<String>"),
                parsed
            );
            assert_eq!(parsed.to_string(), valid);
            assert_eq!(parsed.as_ref(), valid);
            assert_eq!(Into::<String>::into(parsed), valid);

            let by_str = invalid.parse::<T>().expect_err("FromStr refuses");
            assert_eq!(
                T::try_from(invalid).expect_err("TryFrom<&str> refuses"),
                by_str
            );
            assert_eq!(
                T::try_from(invalid.to_owned()).expect_err("TryFrom<String> refuses"),
                by_str,
                "one validation behind the three"
            );
        }

        assert_set::<Namespace, _>("acct", "Acct");
        assert_set::<OrderKey, _>("ORD-1", "ORD:1");
        assert_set::<CorrectionId, _>("c-1", "-c");
        assert_set::<InvoiceNumber, _>("SZ-1", "SZ 1");
    }

    /// The namespace is 1–16 bytes of `[a-z0-9-]`; `:` is excluded because it
    /// is the external-id separator.
    #[test]
    fn namespace_rule_is_enforced_at_parse_time() {
        for accepted in ["a", "acct", "acct-1", "0", "a".repeat(16).as_str()] {
            let namespace: Namespace = accepted.parse().expect(accepted);
            assert_eq!(namespace.as_str(), accepted);
            assert_eq!(namespace.to_string(), accepted);
            assert_eq!(
                serde_json::from_value::<Namespace>(serde_json::json!(accepted)).expect(accepted),
                namespace
            );
        }

        let too_long = "a".repeat(17);
        let rejected = [
            ("", InvalidNamespace::Empty),
            ("Acct", InvalidNamespace::InvalidChar('A')),
            ("acct_1", InvalidNamespace::InvalidChar('_')),
            ("acct 1", InvalidNamespace::InvalidChar(' ')),
            ("acct:1", InvalidNamespace::InvalidChar(':')),
            ("ácct", InvalidNamespace::InvalidChar('á')),
            (too_long.as_str(), InvalidNamespace::TooLong(17)),
        ];
        for (input, expected) in rejected {
            assert_eq!(
                input.parse::<Namespace>(),
                Err(expected.clone()),
                "{input:?}"
            );
            assert_eq!(Namespace::try_from(input.to_owned()), Err(expected));
            assert!(
                serde_json::from_value::<Namespace>(serde_json::json!(input)).is_err(),
                "{input:?} should be rejected"
            );
        }
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
    /// [`OrderKey`], which already makes such a
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
    /// (`Szamlazz.Agent.query`'s selector, `set_credit_entries`, `storno`,
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
    }

    /// Every issued kind names the `tipus` its documents carry, and reads
    /// back from it; a storno, a delivery note and an unknown code are no
    /// kind the worker issues.
    #[test]
    fn issued_kinds_map_onto_document_types_and_back() {
        use szamlazz_agent::DocumentType;

        for kind in IssuedKind::ALL {
            assert_eq!(
                IssuedKind::for_document_type(&kind.document_type()),
                Some(kind),
                "{kind:?}"
            );
        }
        assert_eq!(IssuedKind::Invoice.document_type(), DocumentType::Invoice);
        assert_eq!(IssuedKind::Proforma.document_type(), DocumentType::Proforma);
        assert_eq!(
            IssuedKind::Prepayment.document_type(),
            DocumentType::Prepayment
        );
        assert_eq!(IssuedKind::Final.document_type(), DocumentType::Final);
        assert_eq!(
            IssuedKind::Corrective.document_type(),
            DocumentType::Corrective
        );
        for not_issued in [
            DocumentType::Storno,
            DocumentType::DeliveryNote,
            DocumentType::Other("XX".to_owned()),
        ] {
            assert_eq!(
                IssuedKind::for_document_type(&not_issued),
                None,
                "{not_issued}"
            );
        }
    }
}
