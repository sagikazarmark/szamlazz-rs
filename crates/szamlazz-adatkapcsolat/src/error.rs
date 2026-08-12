//! Error types.

/// An opaque XML-parsing failure.
///
/// Wraps the underlying parser error so the XML backend is not part of this
/// crate's public API — it can change without a breaking release. The cause is
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
    /// The XML was well formed but omitted required protocol structure.
    #[error("invalid document structure: {0}")]
    Validation(String),
}

impl From<quick_xml::DeError> for ParseError {
    fn from(error: quick_xml::DeError) -> Self {
        Self::Xml(XmlError(Box::new(error)))
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
