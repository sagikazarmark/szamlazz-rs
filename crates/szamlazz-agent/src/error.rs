//! Error types.
//!
//! szamlazz.hu signals errors in-band — numeric codes plus Hungarian messages
//! in `szlahu_*` response headers or the response XML — never via HTTP status
//! codes. [`ErrorCode`] gives the documented codes typed names with English
//! documentation; the Hungarian message is kept verbatim in [`ApiError`].

/// A documented Számla Agent error code.
///
/// The set is open: codes not documented (or added later by szamlazz.hu) parse
/// as [`ErrorCode::Unknown`].
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorCode {
    /// 1 — system maintenance or internal error; retry in a few minutes.
    Maintenance,
    /// 3 — authentication failed: invalid agent key or username/password.
    InvalidCredentials,
    /// 7 — missing data (`Hiányzó adat`): a required field is absent from the
    /// request, or the referenced document was not found (an unknown invoice
    /// number, order number, or external identifier on PDF/XML queries).
    /// Receipt operations report an unknown receipt number as code 339, which
    /// parses as [`ErrorCode::Unknown`].
    MissingData,
    /// 53 — the XML was not received as a proper multipart file field.
    XmlNotAFile,
    /// 54 — e-invoice issuance not enabled: missing subscription permission or
    /// certificate.
    EInvoiceNotEnabled,
    /// 55 — e-invoice signing failed: certificate expired or the timestamp
    /// server is unreachable.
    EInvoiceSigningFailed,
    /// 56 — the invoice was issued, but its notification could not be
    /// delivered. Invoice-issuing operations expose this as a non-fatal flag
    /// when the response also contains the issued invoice number.
    InvoiceNotificationDeliveryFailed,
    /// 57 — malformed request XML.
    MalformedXml,
    /// 71 — the order number already exists on another document.
    DuplicateOrderNumber,
    /// 135 — the user is logged into szamlazz.hu in a browser; log out to run
    /// the Agent.
    BrowserSessionActive,
    /// 136 — authentication blocked (expired subscription, pending invoice, or
    /// payment delay); log in via the browser to resolve.
    LoginBlocked,
    /// 152 — the order number already exists on another document; the message
    /// names the offending order number.
    DuplicateOrderNumberNamed,
    /// 164 — the user has access to multiple accounts; the Agent requires
    /// single-account access (use an agent key).
    MultipleAccounts,
    /// 202 — the invoice number prefix (`szamlaszamElotag`) is not registered.
    UnregisteredPrefix,
    /// 259 — line item net value must equal unit price × quantity.
    NetValueMismatch,
    /// 260 — line item VAT value must equal net × rate / 100.
    VatValueMismatch,
    /// 261 — line item gross value must equal net + VAT.
    GrossValueMismatch,
    /// 262 — line item net value error; the offending row is named in the
    /// message.
    NetValueInvalid,
    /// 263 — line item VAT value error; the offending row is named in the
    /// message.
    VatValueInvalid,
    /// 264 — line item gross value error; the offending row is named in the
    /// message.
    GrossValueInvalid,
    /// 335 — proforma not found (or already deleted).
    ProformaNotFound,
    /// 338 — a receipt call identifier has already been used; no duplicate
    /// receipt is issued and the prior success is not replayed.
    DuplicateReceiptCallId,
    /// 537 — an item reached the maximum of 400 data erasure codes.
    ErasureCodeLimit,
    /// 538 — data erasure codes are unavailable on demo/test accounts.
    ErasureCodesUnavailable,
    /// 539 — data erasure codes are disabled in the account settings.
    ErasureCodesDisabled,
    /// Any code without documented meaning, preserved exactly from the wire.
    Unknown(String),
}

impl ErrorCode {
    /// The wire code.
    #[must_use]
    pub fn code(&self) -> &str {
        match self {
            Self::Maintenance => "1",
            Self::InvalidCredentials => "3",
            Self::MissingData => "7",
            Self::XmlNotAFile => "53",
            Self::EInvoiceNotEnabled => "54",
            Self::EInvoiceSigningFailed => "55",
            Self::InvoiceNotificationDeliveryFailed => "56",
            Self::MalformedXml => "57",
            Self::DuplicateOrderNumber => "71",
            Self::BrowserSessionActive => "135",
            Self::LoginBlocked => "136",
            Self::DuplicateOrderNumberNamed => "152",
            Self::MultipleAccounts => "164",
            Self::UnregisteredPrefix => "202",
            Self::NetValueMismatch => "259",
            Self::VatValueMismatch => "260",
            Self::GrossValueMismatch => "261",
            Self::NetValueInvalid => "262",
            Self::VatValueInvalid => "263",
            Self::GrossValueInvalid => "264",
            Self::ProformaNotFound => "335",
            Self::DuplicateReceiptCallId => "338",
            Self::ErasureCodeLimit => "537",
            Self::ErasureCodesUnavailable => "538",
            Self::ErasureCodesDisabled => "539",
            Self::Unknown(code) => code,
        }
    }

    /// Whether retrying the same request later can succeed.
    ///
    /// szamlazz.hu asks integrations to retry at most ~5 times and never in a
    /// tight loop. Note that invoice creation has no idempotency key: a retry
    /// after a *transport* timeout can issue a duplicate legal document. Only
    /// retry on errors the server itself reported.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Maintenance | Self::EInvoiceSigningFailed)
    }
}

impl From<&str> for ErrorCode {
    fn from(code: &str) -> Self {
        let code = code.trim();

        match code {
            "1" => Self::Maintenance,
            "3" => Self::InvalidCredentials,
            "7" => Self::MissingData,
            "53" => Self::XmlNotAFile,
            "54" => Self::EInvoiceNotEnabled,
            "55" => Self::EInvoiceSigningFailed,
            "56" => Self::InvoiceNotificationDeliveryFailed,
            "57" => Self::MalformedXml,
            "71" => Self::DuplicateOrderNumber,
            "135" => Self::BrowserSessionActive,
            "136" => Self::LoginBlocked,
            "152" => Self::DuplicateOrderNumberNamed,
            "164" => Self::MultipleAccounts,
            "202" => Self::UnregisteredPrefix,
            "259" => Self::NetValueMismatch,
            "260" => Self::VatValueMismatch,
            "261" => Self::GrossValueMismatch,
            "262" => Self::NetValueInvalid,
            "263" => Self::VatValueInvalid,
            "264" => Self::GrossValueInvalid,
            "335" => Self::ProformaNotFound,
            "338" => Self::DuplicateReceiptCallId,
            "537" => Self::ErasureCodeLimit,
            "538" => Self::ErasureCodesUnavailable,
            "539" => Self::ErasureCodesDisabled,
            other => Self::Unknown(other.to_owned()),
        }
    }
}

impl From<String> for ErrorCode {
    fn from(code: String) -> Self {
        Self::from(code.as_str())
    }
}

impl From<u16> for ErrorCode {
    fn from(code: u16) -> Self {
        Self::from(code.to_string())
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

/// A request that cannot satisfy the Számla Agent wire contract.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RequestError {
    /// Invoice and receipt creation require at least one line item.
    #[error("document creation requires at least one line item")]
    MissingLineItems,
    /// A final invoice must identify its prepayment invoice by invoice number
    /// or by the shared order number.
    #[error("a final invoice requires a prepayment invoice number or order number")]
    MissingPrepaymentReference,
    /// Foreign-currency documents require the quoting bank and exchange rate.
    #[error("foreign-currency documents require an exchange rate")]
    MissingExchangeRate,
    /// Exchange-rate details contain no bank, or request automatic lookup from
    /// a bank other than MNB.
    #[error("invalid foreign-currency exchange-rate details")]
    InvalidExchangeRate,
    /// A line item requests more data erasure codes than szamlazz.hu's
    /// documented per-item maximum of 400 (rejected server-side as error 537).
    #[error("line item requests {0} data erasure codes; the maximum is 400")]
    ErasureCodeCountOutOfRange(u32),
    /// A waybill parcel count exceeds the nonnegative XML Schema `int` range.
    #[error("waybill parcel count {0} exceeds 2147483647")]
    ParcelCountOutOfRange(u32),
    /// An operation produced bytes that are not UTF-8 XML.
    #[error("request XML is not valid UTF-8")]
    InvalidXmlEncoding,
    /// An operation contains a character forbidden by XML 1.0.
    #[error("request XML contains character U+{0:04X}, which XML 1.0 forbids")]
    InvalidXmlCharacter(u32),
}

/// An error reported by szamlazz.hu.
#[doc(alias = "hibakód")]
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("szamlazz.hu error {code}: {message}")]
pub struct ApiError {
    /// The typed error code.
    pub code: ErrorCode,
    /// The verbatim (Hungarian) error message.
    #[doc(alias = "hibaüzenet")]
    pub message: String,
}

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

/// A response that could not be interpreted.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ParseError {
    /// The response body is not well-formed XML.
    #[error("invalid response XML: {0}")]
    Xml(#[source] XmlError),
    /// A required element or header is missing.
    #[error("missing {0} in response")]
    Missing(&'static str),
    /// A field failed to parse into its typed representation.
    #[error("invalid value for {field}: {message}")]
    Invalid {
        /// The field that failed to parse.
        field: &'static str,
        /// What went wrong.
        message: String,
    },
    /// Base64-encoded content (a PDF) failed to decode.
    #[error("invalid base64 payload: {0}")]
    Base64(String),
    /// The body matched none of the shapes the operation can produce.
    #[error("unexpected response body: {0}")]
    UnexpectedBody(String),
}

impl From<quick_xml::DeError> for ParseError {
    fn from(error: quick_xml::DeError) -> Self {
        Self::Xml(XmlError(Box::new(error)))
    }
}

impl From<base64::DecodeError> for ParseError {
    fn from(error: base64::DecodeError) -> Self {
        Self::Base64(error.to_string())
    }
}

/// Failure of a Számla Agent call: either the server rejected it, or its
/// response could not be interpreted.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ResponseError {
    /// szamlazz.hu reported an error.
    #[error(transparent)]
    Api(#[from] ApiError),
    /// Számla Agent reported temporary system unavailability through the
    /// `szlahu_down` response header.
    #[error("szamlazz.hu is temporarily unavailable: {0}")]
    ServiceUnavailable(String),
    /// The response could not be parsed.
    #[error(transparent)]
    Parse(#[from] ParseError),
}
