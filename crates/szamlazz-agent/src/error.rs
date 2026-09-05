//! Error types.
//!
//! szamlazz.hu signals errors in-band — numeric codes plus Hungarian messages
//! in `szlahu_*` response headers or the response XML — never via HTTP status
//! codes. [`ErrorCode`] gives the documented codes typed names with English
//! documentation; the Hungarian message is kept verbatim in [`ApiError`].
//!
//! Which channel carries the error depends on the operation: invoice creation,
//! storno, and proforma deletion set `szlahu_error_code`/`szlahu_error`
//! headers *and* a `<hibakod>`/`<hibauzenet>` body, while the XML query (code
//! 7) and credit-entry registration (code 463) report in the body only. Every
//! parser in this crate therefore reads the body's `<hibakod>` as well as the
//! headers; see [`RawResponse::header_error`](crate::wire::RawResponse::header_error).

/// A documented Számla Agent error code.
///
/// The set is open: codes not documented (or added later by szamlazz.hu) parse
/// as [`ErrorCode::Unknown`]. Codes marked *observed* are undocumented but were
/// reproduced against a szamlazz.hu test account; their Hungarian messages are
/// quoted verbatim.
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
    ///
    /// On queries the code is reported in the body only (no `szlahu_error_code`
    /// header). A proforma that has been converted into an invoice — by an
    /// explicit reference or by an invoice issued under the same order number —
    /// also returns 7 by number and by external identifier, exactly like a
    /// deleted one.
    MissingData,
    /// 14 (observed) — the referenced document is itself a storno or credit
    /// invoice and cannot be reversed or credited: `Sztornó és jóváíró számlát
    /// nem lehet sem sztornózni, sem jóváírni.` Returned by the storno
    /// operation when [`StornoInvoice::invoice_number`](crate::ops::storno::StornoInvoice::invoice_number)
    /// names a storno invoice.
    StornoOfReversalInvoice,
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
    ///
    /// Fires only when the account setting *Rendelésszám ismétlődés tiltása*
    /// (disable order number repetition) is on. The check is scoped per
    /// document type — an invoice, a proforma, a prepayment invoice, a final
    /// invoice, and a delivery note may all carry the same order number — and
    /// storno and corrective invoices are exempt (a storno invoice inherits
    /// its original's order number; a corrective invoice may repeat it).
    /// Reversing an invoice frees its order number for reuse, and a
    /// byte-identical resend while the first document is live returns that
    /// document instead of an error.
    DuplicateOrderNumber,
    /// 73 (observed) — the referenced prepayment invoice cannot be identified:
    /// `A hivatkozott előlegszámla nem beazonosítható. Rendelésszám: …,
    /// előlegszámla száma: ….` Returned for a
    /// [final invoice](crate::ops::invoice::InvoiceKind::Final) whose
    /// prepayment invoice number or order number does not resolve to a live
    /// prepayment invoice. This is
    /// also how the server enforces one final invoice per prepayment invoice:
    /// once a prepayment invoice has been settled by a final invoice, a second
    /// final invoice referencing it gets 73 even with the correct number and
    /// order number. It is checked before the duplicate-order-number rule
    /// (71/152).
    PrepaymentInvoiceNotIdentifiable,
    /// 135 — the user is logged into szamlazz.hu in a browser; log out to run
    /// the Agent.
    BrowserSessionActive,
    /// 136 — authentication blocked (expired subscription, pending invoice, or
    /// payment delay); log in via the browser to resolve.
    LoginBlocked,
    /// 152 — the order number already exists on another document; the message
    /// names the offending order number.
    ///
    /// Same rule as [`ErrorCode::DuplicateOrderNumber`]: requires the account
    /// setting *Rendelésszám ismétlődés tiltása*, is scoped per document type,
    /// exempts storno and corrective invoices, and a reversed invoice's order
    /// number becomes reusable. The message (`Már létező rendelésszám: ….
    /// Az ismétlődés engedélyezhető a Beállítások oldalon.`) names the order
    /// number — whitespace-trimmed — but never the existing invoice number;
    /// recovering that requires a query by order number.
    DuplicateOrderNumberNamed,
    /// 164 — the user has access to multiple accounts; the Agent requires
    /// single-account access (use an agent key).
    MultipleAccounts,
    /// 202 — the invoice number prefix (`szamlaszamElotag`) is not registered.
    UnregisteredPrefix,
    /// 221 (observed) — the invoice has a corrective invoice and cannot be
    /// reversed: `Ez a számla nem sztornózható (van helyesbítő számlája).`
    /// Returned by the storno operation; the corrective invoice remains the
    /// only way to change such an invoice.
    HasCorrectiveInvoice,
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
    /// 352 (observed) — the issue date (`keltDatum`) may only be today:
    /// `A számla kelte csak a mai nap lehet: ….` Observed on a storno request
    /// carrying an earlier `keltDatum` on an e-invoice account; omit
    /// [`StornoInvoice::issue_date`](crate::ops::storno::StornoInvoice::issue_date)
    /// to let the server date the storno invoice.
    IssueDateMustBeToday,
    /// 463 (observed) — a credit entry was registered against a reversed or
    /// reversing invoice: `Sztornózó vagy sztornózott számlához nem tartozhat
    /// kifizetettségi információ.` Reported in the body only (no
    /// `szlahu_error_code` header). Reversal also removes the original
    /// invoice's recorded payments from its queried XML.
    PaymentOnReversedInvoice,
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
            Self::StornoOfReversalInvoice => "14",
            Self::XmlNotAFile => "53",
            Self::EInvoiceNotEnabled => "54",
            Self::EInvoiceSigningFailed => "55",
            Self::InvoiceNotificationDeliveryFailed => "56",
            Self::MalformedXml => "57",
            Self::DuplicateOrderNumber => "71",
            Self::PrepaymentInvoiceNotIdentifiable => "73",
            Self::BrowserSessionActive => "135",
            Self::LoginBlocked => "136",
            Self::DuplicateOrderNumberNamed => "152",
            Self::MultipleAccounts => "164",
            Self::UnregisteredPrefix => "202",
            Self::HasCorrectiveInvoice => "221",
            Self::NetValueMismatch => "259",
            Self::VatValueMismatch => "260",
            Self::GrossValueMismatch => "261",
            Self::NetValueInvalid => "262",
            Self::VatValueInvalid => "263",
            Self::GrossValueInvalid => "264",
            Self::ProformaNotFound => "335",
            Self::DuplicateReceiptCallId => "338",
            Self::IssueDateMustBeToday => "352",
            Self::PaymentOnReversedInvoice => "463",
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
            "14" => Self::StornoOfReversalInvoice,
            "53" => Self::XmlNotAFile,
            "54" => Self::EInvoiceNotEnabled,
            "55" => Self::EInvoiceSigningFailed,
            "56" => Self::InvoiceNotificationDeliveryFailed,
            "57" => Self::MalformedXml,
            "71" => Self::DuplicateOrderNumber,
            "73" => Self::PrepaymentInvoiceNotIdentifiable,
            "135" => Self::BrowserSessionActive,
            "136" => Self::LoginBlocked,
            "152" => Self::DuplicateOrderNumberNamed,
            "164" => Self::MultipleAccounts,
            "202" => Self::UnregisteredPrefix,
            "221" => Self::HasCorrectiveInvoice,
            "259" => Self::NetValueMismatch,
            "260" => Self::VatValueMismatch,
            "261" => Self::GrossValueMismatch,
            "262" => Self::NetValueInvalid,
            "263" => Self::VatValueInvalid,
            "264" => Self::GrossValueInvalid,
            "335" => Self::ProformaNotFound,
            "338" => Self::DuplicateReceiptCallId,
            "352" => Self::IssueDateMustBeToday,
            "463" => Self::PaymentOnReversedInvoice,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every named variant, so the round-trip test cannot silently skip one.
    const NAMED: [ErrorCode; 30] = [
        ErrorCode::Maintenance,
        ErrorCode::InvalidCredentials,
        ErrorCode::MissingData,
        ErrorCode::StornoOfReversalInvoice,
        ErrorCode::XmlNotAFile,
        ErrorCode::EInvoiceNotEnabled,
        ErrorCode::EInvoiceSigningFailed,
        ErrorCode::InvoiceNotificationDeliveryFailed,
        ErrorCode::MalformedXml,
        ErrorCode::DuplicateOrderNumber,
        ErrorCode::PrepaymentInvoiceNotIdentifiable,
        ErrorCode::BrowserSessionActive,
        ErrorCode::LoginBlocked,
        ErrorCode::DuplicateOrderNumberNamed,
        ErrorCode::MultipleAccounts,
        ErrorCode::UnregisteredPrefix,
        ErrorCode::HasCorrectiveInvoice,
        ErrorCode::NetValueMismatch,
        ErrorCode::VatValueMismatch,
        ErrorCode::GrossValueMismatch,
        ErrorCode::NetValueInvalid,
        ErrorCode::VatValueInvalid,
        ErrorCode::GrossValueInvalid,
        ErrorCode::ProformaNotFound,
        ErrorCode::DuplicateReceiptCallId,
        ErrorCode::IssueDateMustBeToday,
        ErrorCode::PaymentOnReversedInvoice,
        ErrorCode::ErasureCodeLimit,
        ErrorCode::ErasureCodesUnavailable,
        ErrorCode::ErasureCodesDisabled,
    ];

    #[test]
    fn named_codes_round_trip_through_the_wire_code() {
        for code in NAMED {
            assert_eq!(ErrorCode::from(code.code()), code, "{code:?}");
            assert_eq!(ErrorCode::from(code.code().to_owned()), code, "{code:?}");
            assert_eq!(code.to_string(), code.code(), "{code:?}");
            assert!(
                !matches!(ErrorCode::from(code.code()), ErrorCode::Unknown(_)),
                "{code:?} must not parse as Unknown"
            );
        }
    }

    #[test]
    fn numeric_codes_round_trip() {
        for code in NAMED {
            let numeric: u16 = code.code().parse().expect("named codes are numeric");
            assert_eq!(ErrorCode::from(numeric), code, "{code:?}");
        }
    }

    #[test]
    fn observed_storno_and_credit_codes_are_typed() {
        assert_eq!(ErrorCode::from("14"), ErrorCode::StornoOfReversalInvoice);
        assert_eq!(
            ErrorCode::from("73"),
            ErrorCode::PrepaymentInvoiceNotIdentifiable
        );
        assert_eq!(ErrorCode::from("221"), ErrorCode::HasCorrectiveInvoice);
        assert_eq!(ErrorCode::from("352"), ErrorCode::IssueDateMustBeToday);
        assert_eq!(ErrorCode::from("463"), ErrorCode::PaymentOnReversedInvoice);
    }

    #[test]
    fn wire_code_is_trimmed_and_unknown_codes_are_preserved() {
        assert_eq!(
            ErrorCode::from(" 463 "),
            ErrorCode::PaymentOnReversedInvoice
        );
        assert_eq!(
            ErrorCode::from("FUTURE_CODE"),
            ErrorCode::Unknown("FUTURE_CODE".to_owned())
        );
        assert_eq!(ErrorCode::Unknown("999".to_owned()).code(), "999");
    }

    #[test]
    fn only_transient_codes_are_retryable() {
        for code in NAMED {
            let expected = matches!(
                code,
                ErrorCode::Maintenance | ErrorCode::EInvoiceSigningFailed
            );
            assert_eq!(code.is_retryable(), expected, "{code:?}");
        }
        assert!(!ErrorCode::Unknown("999".to_owned()).is_retryable());
    }
}
