//! Error types.

/// An opaque XML-parsing failure.
///
/// Wraps the underlying parser error so the XML backend is not part of this
/// crate's public API: it can change without a breaking release. The cause is
/// available through [`Display`](std::fmt::Display) and, type-erased, through
/// [`Error::source`](std::error::Error::source).
#[derive(Debug)]
pub struct XmlError(Box<dyn std::error::Error + Send + Sync + 'static>);

impl std::fmt::Display for XmlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl std::error::Error for XmlError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&*self.0)
    }
}

/// A pushed document that could not be interpreted.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ParseError {
    /// The body is not valid UTF-8. Adatkapcsolat XML is sent as UTF-8 and
    /// replacement characters must not silently alter business data.
    #[error("document is not valid UTF-8: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    /// The body is not well-formed XML or does not match the document schema.
    #[error("invalid document XML: {0}")]
    Xml(#[source] XmlError),
    /// The body's root element is not a known document type.
    #[error("unknown document root element: {0}")]
    UnknownRoot(String),
    /// The body has no root element at all.
    #[error("empty request body")]
    Empty,
    /// The root element has a namespace other than its official namespace.
    #[error("wrong namespace for {root}: expected {expected}, got {actual}")]
    WrongNamespace {
        /// Root element local name.
        root: String,
        /// Namespace required by the current official schema.
        expected: &'static str,
        /// Namespace found on the root element, if any.
        actual: String,
    },
    /// The document parsed but does not conform to its XSD: an element the
    /// schema requires is missing, an enumeration carries an unknown value,
    /// a VAT rate is negative. Raised only by [`Document::parse_strict`];
    /// [`Document::parse`] reads such a document leniently.
    ///
    /// [`Document::parse`]: crate::Document::parse
    /// [`Document::parse_strict`]: crate::Document::parse_strict
    #[error("invalid document structure: {0}")]
    Validation(#[from] ValidationError),
}

impl From<quick_xml::DeError> for ParseError {
    fn from(error: quick_xml::DeError) -> Self {
        Self::Xml(XmlError(Box::new(error)))
    }
}

/// A parsed document that does not conform to its XSD: the first requirement
/// it misses, as [`Document::validate`] reports it.
///
/// The parse does not need what the schema requires; this is the signal for a
/// receiver that does. Every variant names the element by its wire path,
/// prefixed with the document it sits in (`invoice alap/kelt`, `bank
/// transaction irany`, `receipt tetel/afakulcs`), so a receiver can act on
/// *which* requirement failed without parsing the message; the `Display` text
/// is unchanged from 0.3 (`missing required invoice alap/kelt`).
///
/// Breaking change in 0.4: this was an opaque struct carrying the message.
///
/// [`Document::validate`]: crate::Document::validate
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ValidationError {
    /// An element the XSD marks `minOccurs="1"` is absent, empty, or (a date)
    /// holds a text that is not one.
    #[error("missing required {path}")]
    MissingRequired {
        /// The element's wire path, prefixed with the document.
        path: String,
    },
    /// An enumerated element carries a token outside the XSD's enumeration
    /// (an `<irany>` other than `BE` / `KI`).
    #[error("{path} has unknown value {token}")]
    UnknownToken {
        /// The element's wire path, prefixed with the document.
        path: String,
        /// The token as received.
        token: String,
    },
    /// A VAT rate is negative.
    #[error("{path} must not be negative")]
    Negative {
        /// The element's wire path, prefixed with the document.
        path: String,
    },
    /// A list the XSD requires at least one child of has none (`tetelek`
    /// without a `tetel`, `osszegek` without an `afakulcsossz`, a batch
    /// without a `nyugta`).
    #[error("{path} must contain at least one {child}")]
    Empty {
        /// The list's wire path, prefixed with the document.
        path: String,
        /// The child element the list lacks.
        child: &'static str,
    },
}

impl ValidationError {
    pub(crate) fn missing(path: impl Into<String>) -> Self {
        Self::MissingRequired { path: path.into() }
    }

    pub(crate) fn unknown_token(path: impl Into<String>, token: impl Into<String>) -> Self {
        Self::UnknownToken {
            path: path.into(),
            token: token.into(),
        }
    }

    pub(crate) fn negative(path: impl Into<String>) -> Self {
        Self::Negative { path: path.into() }
    }

    pub(crate) fn empty(path: impl Into<String>, child: &'static str) -> Self {
        Self::Empty {
            path: path.into(),
            child,
        }
    }
}

/// An Ack that cannot be represented as well-formed protocol XML.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AckError {
    /// Caller-provided text contains a character forbidden by XML 1.0.
    #[error("Ack XML contains character U+{codepoint:04X}, which XML 1.0 forbids")]
    InvalidXmlCharacter {
        /// Unicode code point of the invalid character.
        codepoint: u32,
    },
}
