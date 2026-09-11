//! Error types.
//!
//! szamlazz.hu reports domain errors in-band (numeric codes plus Hungarian
//! messages in `szlahu_*` response headers or the response XML). HTTP failures
//! remain possible and are handled separately. [`ErrorCode`] gives the
//! documented codes typed names with English documentation; the Hungarian
//! message is kept verbatim in [`ApiError`].
//!
//! Two questions are answered per error, and they are different questions:
//! [`ErrorCode::is_retryable`] (can the same *query* succeed later), and
//! [`ErrorCode::outcome_class`] (also on [`ResponseError`] and the client's
//! error): may a *document* have been created despite the error. A
//! caller uses the second together with the [operation recovery table](#recovery).
//! A refusal describes this exchange, not any earlier send of the logical operation.
//!
//! Which channel carries the error depends on the operation: invoice creation,
//! storno, and proforma deletion set `szlahu_error_code`/`szlahu_error`
//! headers *and* a `<hibakod>`/`<hibauzenet>` body, while the XML query (code
//! 7) and credit-entry registration (code 463) report in the body only. Every
//! parser in this crate therefore reads the body's `<hibakod>` as well as the
//! headers; see [`RawResponse::header_error`](crate::wire::RawResponse::header_error).

#![doc = include_str!("recovery.md")]

/// A documented Számla Agent error code.
///
/// The set is open: codes not documented (or added later by szamlazz.hu) parse
/// as [`ErrorCode::Unknown`], and a failure reported without any code
/// (`sikeres=false` with no `hibakod`) is [`ErrorCode::Absent`]. Codes marked
/// *observed* are undocumented but were reproduced against a szamlazz.hu test
/// account; their Hungarian messages are quoted verbatim.
///
/// Parsed from the wire with [`FromStr`](std::str::FromStr) (infallible) or
/// `From<&str>` / `From<String>` / `From<u16>`; the text is trimmed, and a
/// numeric token is matched as a number, so `007` is code 7.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorCode {
    /// 1: system maintenance or internal error; retry in a few minutes.
    Maintenance,
    /// 3: authentication failed (invalid agent key or username/password).
    InvalidCredentials,
    /// 7: missing data (`Hiányzó adat`): a required field is absent from the
    /// request, or the referenced document was not found (an unknown invoice
    /// number, order number, or external identifier on PDF/XML queries).
    /// Receipt operations report an unknown receipt number as
    /// [`ErrorCode::ReceiptNotFound`] (339). On receipt send, 7 can instead
    /// mean a missing subject; the class alone does not identify the missing data.
    ///
    /// On queries the code is reported in the body only (no `szlahu_error_code`
    /// header). A proforma that has been converted into an invoice (by an
    /// explicit reference or by an invoice issued under the same order number)
    /// also returns 7 by number and by external identifier, exactly like a
    /// deleted one.
    MissingData,
    /// 14 (observed): the referenced document is itself a storno or credit
    /// invoice and cannot be reversed or credited: `Sztornó és jóváíró számlát
    /// nem lehet sem sztornózni, sem jóváírni.` Returned by the storno
    /// operation when [`StornoInvoice::invoice_number`](crate::ops::storno::StornoInvoice::invoice_number)
    /// names a storno invoice.
    StornoOfReversalInvoice,
    /// 53: the XML was not received as a proper multipart file field.
    XmlNotAFile,
    /// 54: e-invoice issuance not enabled; missing subscription permission or
    /// certificate.
    EInvoiceNotEnabled,
    /// 55: e-invoice signing failed; certificate expired or the timestamp
    /// server is unreachable. Signing failure does not prove issuance. Timestamp
    /// access may recover later; an expired certificate needs operator remediation.
    EInvoiceSigningFailed,
    /// 56: the invoice was issued, but its notification could not be
    /// delivered, when accompanied by its number. Invoice-issuing operations
    /// expose that as a non-fatal flag; without a number the outcome is unknown.
    /// Corroborated by first-party PHP 2.12.4 source, not observed on the test account.
    InvoiceNotificationDeliveryFailed,
    /// 57: malformed request XML.
    MalformedXml,
    /// 71: the order number already exists on another document.
    ///
    /// Fires only when the account setting *Rendelésszám ismétlődés tiltása*
    /// (disable order number repetition) is on. The check is scoped per
    /// document type (an invoice, a proforma, a prepayment invoice, a final
    /// invoice, and a delivery note may all carry the same order number), and
    /// storno and corrective invoices are exempt (a storno invoice inherits
    /// its original's order number; a corrective invoice may repeat it).
    /// The [documented replay rule](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number)
    /// additionally requires matching buyer, gross amount and three dates, and
    /// a document created within the last two days. A live document alone does
    /// not guarantee replay. Short-interval identical replay and order-number
    /// reuse after reversal were observed on the test account; those observations
    /// do not establish an indefinite replay window.
    ///
    /// Nothing new was created ([`OutcomeClass::DuplicateOrderNumber`]); a
    /// query by order number names the existing document. Treat the code as
    /// a settled refusal even when that query then finds nothing under the
    /// order: re-sending the create only repeats the answer.
    DuplicateOrderNumber,
    /// 73 (observed): the referenced prepayment invoice cannot be identified:
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
    /// 135: the user is logged into szamlazz.hu in a browser; log out to run
    /// the Agent.
    BrowserSessionActive,
    /// 136: authentication blocked (expired subscription, pending invoice, or
    /// payment delay); log in via the browser to resolve.
    LoginBlocked,
    /// 152: the order number already exists on another document; the message
    /// names the offending order number.
    ///
    /// Same rule as [`ErrorCode::DuplicateOrderNumber`]: requires the account
    /// setting *Rendelésszám ismétlődés tiltása*, is scoped per document type,
    /// exempts storno and corrective invoices, and a reversed invoice's order
    /// number becomes reusable. The message (`Már létező rendelésszám: ….
    /// Az ismétlődés engedélyezhető a Beállítások oldalon.`) names the order
    /// number (whitespace-trimmed), but never the existing invoice number;
    /// recovering that requires a query by order number. Like 71, a settled
    /// refusal: nothing new was created, and re-sending only repeats it.
    DuplicateOrderNumberNamed,
    /// 164: the user has access to multiple accounts; the Agent requires
    /// single-account access (use an agent key).
    MultipleAccounts,
    /// 202: the invoice number prefix (`szamlaszamElotag`) is not registered.
    UnregisteredPrefix,
    /// 221 (observed): the invoice has a corrective invoice and cannot be
    /// reversed: `Ez a számla nem sztornózható (van helyesbítő számlája).`
    /// Returned by the storno operation; the corrective invoice remains the
    /// only way to change such an invoice.
    HasCorrectiveInvoice,
    /// 259: line item net value must equal unit price × quantity.
    NetValueMismatch,
    /// 260: line item VAT value must equal net × rate / 100.
    VatValueMismatch,
    /// 261: line item gross value must equal net + VAT.
    GrossValueMismatch,
    /// 262: line item net value error; the offending row is named in the
    /// message.
    NetValueInvalid,
    /// 263: line item VAT value error; the offending row is named in the
    /// message.
    VatValueInvalid,
    /// 264: line item gross value error; the offending row is named in the
    /// message.
    GrossValueInvalid,
    /// 335: proforma not found (or already deleted).
    ProformaNotFound,
    /// 336: the receipt prefix is already used for invoices.
    ReceiptPrefixUsedForInvoices,
    /// 337: invalid receipt prefix; only capital letters and numbers are allowed.
    /// A test-account reply on 2026-09-11 additionally specified at most five characters.
    InvalidReceiptPrefix,
    /// 338: a receipt call identifier has already been used; no duplicate
    /// receipt is issued and the prior success is not replayed.
    DuplicateReceiptCallId,
    /// 339: the referenced receipt number does not exist.
    ReceiptNotFound,
    /// 340: receipt tender amounts do not sum to its gross total.
    ReceiptPaymentMismatch,
    /// 352 (observed): the issue date (`keltDatum`) may only be today:
    /// `A számla kelte csak a mai nap lehet: ….` Observed on a storno request
    /// carrying an earlier `keltDatum`, reversing a paper invoice with a paper
    /// storno, so not a rule of e-invoices only; omit
    /// [`StornoInvoice::issue_date`](crate::ops::storno::StornoInvoice::issue_date)
    /// to let the server date the storno invoice.
    IssueDateMustBeToday,
    /// 363: a HUF/Ft receipt item's gross value must be a whole number.
    ReceiptGrossNotWhole,
    /// 364: a HUF/Ft receipt item's net value may have at most two decimal places.
    ReceiptNetPrecision,
    /// 365: a HUF/Ft receipt item's VAT value may have at most two decimal places.
    ReceiptVatPrecision,
    /// 463 (observed): a credit entry was registered against a reversed or
    /// reversing invoice: `Sztornózó vagy sztornózott számlához nem tartozhat
    /// kifizetettségi információ.` Reported in the body only (no
    /// `szlahu_error_code` header). Reversal also removes the original
    /// invoice's recorded payments from its queried XML.
    PaymentOnReversedInvoice,
    /// 537: an item reached the maximum of 400 data erasure codes.
    ErasureCodeLimit,
    /// 538: data erasure codes are unavailable on demo/test accounts.
    ErasureCodesUnavailable,
    /// 539: data erasure codes are disabled in the account settings.
    ErasureCodesDisabled,
    /// 551: simplified invoice image is incompatible with OSS enabled or a
    /// non-Hungarian seller tax number, including an inherited final invoice.
    SimplifiedImageAccountIncompatible,
    /// 552: simplified invoice image permits at most two items (four on a final).
    SimplifiedImageItemLimit,
    /// 553: a simplified invoice image item uses a disallowed VAT token.
    SimplifiedImageVatInvalid,
    /// 554: a simplified-image original cannot be corrected, even when the
    /// corrective request omits `simpleItems`.
    SimplifiedImageCannotCorrect,
    /// 555: simplified final invoice VAT rates differ from the prepayment's.
    SimplifiedImagePrepaymentVatMismatch,
    /// 556: simplified invoice image is forbidden on correctives and delivery notes.
    SimplifiedImageDocumentForbidden,
    /// Any code without documented meaning, preserved exactly from the wire.
    Unknown(String),
    /// szamlazz.hu reported a failure (`sikeres=false`, or NAV's `funcCode`
    /// other than `OK`) and sent no code with it: no `hibakod`, or an empty
    /// one. Nothing is invented in its place; [`code`](Self::code) is the
    /// empty string and the [`Display`](std::fmt::Display) reads `absent`.
    Absent,
}

impl ErrorCode {
    /// The wire code; the empty string for [`ErrorCode::Absent`], which has
    /// none.
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
            Self::ReceiptPrefixUsedForInvoices => "336",
            Self::InvalidReceiptPrefix => "337",
            Self::DuplicateReceiptCallId => "338",
            Self::ReceiptNotFound => "339",
            Self::ReceiptPaymentMismatch => "340",
            Self::IssueDateMustBeToday => "352",
            Self::ReceiptGrossNotWhole => "363",
            Self::ReceiptNetPrecision => "364",
            Self::ReceiptVatPrecision => "365",
            Self::PaymentOnReversedInvoice => "463",
            Self::ErasureCodeLimit => "537",
            Self::ErasureCodesUnavailable => "538",
            Self::ErasureCodesDisabled => "539",
            Self::SimplifiedImageAccountIncompatible => "551",
            Self::SimplifiedImageItemLimit => "552",
            Self::SimplifiedImageVatInvalid => "553",
            Self::SimplifiedImageCannotCorrect => "554",
            Self::SimplifiedImagePrepaymentVatMismatch => "555",
            Self::SimplifiedImageDocumentForbidden => "556",
            Self::Unknown(code) => code,
            Self::Absent => "",
        }
    }

    /// The named variant of a numeric wire code, or `None` when the crate
    /// does not know it. The one table the string and numeric parsers share.
    fn known(code: u16) -> Option<Self> {
        Some(match code {
            1 => Self::Maintenance,
            3 => Self::InvalidCredentials,
            7 => Self::MissingData,
            14 => Self::StornoOfReversalInvoice,
            53 => Self::XmlNotAFile,
            54 => Self::EInvoiceNotEnabled,
            55 => Self::EInvoiceSigningFailed,
            56 => Self::InvoiceNotificationDeliveryFailed,
            57 => Self::MalformedXml,
            71 => Self::DuplicateOrderNumber,
            73 => Self::PrepaymentInvoiceNotIdentifiable,
            135 => Self::BrowserSessionActive,
            136 => Self::LoginBlocked,
            152 => Self::DuplicateOrderNumberNamed,
            164 => Self::MultipleAccounts,
            202 => Self::UnregisteredPrefix,
            221 => Self::HasCorrectiveInvoice,
            259 => Self::NetValueMismatch,
            260 => Self::VatValueMismatch,
            261 => Self::GrossValueMismatch,
            262 => Self::NetValueInvalid,
            263 => Self::VatValueInvalid,
            264 => Self::GrossValueInvalid,
            335 => Self::ProformaNotFound,
            336 => Self::ReceiptPrefixUsedForInvoices,
            337 => Self::InvalidReceiptPrefix,
            338 => Self::DuplicateReceiptCallId,
            339 => Self::ReceiptNotFound,
            340 => Self::ReceiptPaymentMismatch,
            352 => Self::IssueDateMustBeToday,
            363 => Self::ReceiptGrossNotWhole,
            364 => Self::ReceiptNetPrecision,
            365 => Self::ReceiptVatPrecision,
            463 => Self::PaymentOnReversedInvoice,
            537 => Self::ErasureCodeLimit,
            538 => Self::ErasureCodesUnavailable,
            539 => Self::ErasureCodesDisabled,
            551 => Self::SimplifiedImageAccountIncompatible,
            552 => Self::SimplifiedImageItemLimit,
            553 => Self::SimplifiedImageVatInvalid,
            554 => Self::SimplifiedImageCannotCorrect,
            555 => Self::SimplifiedImagePrepaymentVatMismatch,
            556 => Self::SimplifiedImageDocumentForbidden,
            _ => return None,
        })
    }

    /// Whether the same request is potentially transient: `true` for 1
    /// (maintenance) and 55 (signing failed). This is a retry hint suitable for
    /// reads, not permission to repeat a write. Code 55 does not prove issuance:
    /// timestamp access may recover, but an expired certificate needs remediation.
    ///
    /// The vendor permits at most **five total sends of the same request,
    /// including the initial send**, then stop for operator intervention; never
    /// retry in a tight loop. Its same-request wording does not specify an exact
    /// combined budget for a write and its reconciliation queries.
    /// See the [operation recovery table](crate::error#recovery) before any repeat.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Maintenance | Self::EInvoiceSigningFailed)
    }

    /// Whether the code is about the agent credentials rather than the
    /// request: 3 (invalid credentials), 135 (a browser session is active),
    /// 136 (login blocked) or 164 (multiple accounts). Their documented meanings
    /// are authentication/access refusals, which this crate interprets as a
    /// subset of [`OutcomeClass::Rejected`] for this exchange. Exact server
    /// processing order has not been established; a refusal does not settle an
    /// earlier lost send. Fixing access permits another evaluation of the
    /// request, not guaranteed success. An integration pages an operator on
    /// these codes rather than reporting a refusal of the document.
    #[must_use]
    pub fn is_credential_error(&self) -> bool {
        matches!(
            self,
            Self::InvalidCredentials
                | Self::BrowserSessionActive
                | Self::LoginBlocked
                | Self::MultipleAccounts
        )
    }

    /// What this code says about the document the request asked for: may one
    /// exist despite the error? See [`OutcomeClass`] for the caller's action
    /// per class and the [operation recovery table](crate::error#recovery).
    ///
    /// The table combines documented codes, first-party PHP source (56), and
    /// test-account observations (only the variants explicitly marked observed):
    ///
    /// | Class | Codes |
    /// |---|---|
    /// | [`Unknown`](OutcomeClass::Unknown) | 1, 55, 56, every code this crate does not know ([`ErrorCode::Unknown`]) and a failure without a code ([`ErrorCode::Absent`]) |
    /// | [`DuplicateOrderNumber`](OutcomeClass::DuplicateOrderNumber) | 71, 152 |
    /// | [`NotFound`](OutcomeClass::NotFound) | 7 (operation-dependent missing data), 339 (receipt not found) |
    /// | [`Rejected`](OutcomeClass::Rejected) | everything else, the credential codes 3, 135, 136 and 164 included |
    ///
    /// 56 surfaces as an error only when the response carries no document
    /// number (with one, the parsers report success with
    /// `notification_delivery_failed` set), so as an error it always leaves
    /// the outcome open. An unknown code is classified conservatively: it may
    /// be a refusal, or a new "issued, but…" code like numbered 56. Neither 55
    /// nor the thirteen receipt/simplified-image additions were observed on the account.
    #[must_use]
    pub fn outcome_class(&self) -> OutcomeClass {
        match self {
            Self::Maintenance
            | Self::EInvoiceSigningFailed
            | Self::InvoiceNotificationDeliveryFailed
            | Self::Unknown(_)
            | Self::Absent => OutcomeClass::Unknown,
            Self::DuplicateOrderNumber | Self::DuplicateOrderNumberNamed => {
                OutcomeClass::DuplicateOrderNumber
            }
            Self::MissingData | Self::ReceiptNotFound => OutcomeClass::NotFound,
            Self::InvalidCredentials
            | Self::StornoOfReversalInvoice
            | Self::XmlNotAFile
            | Self::EInvoiceNotEnabled
            | Self::MalformedXml
            | Self::PrepaymentInvoiceNotIdentifiable
            | Self::BrowserSessionActive
            | Self::LoginBlocked
            | Self::MultipleAccounts
            | Self::UnregisteredPrefix
            | Self::HasCorrectiveInvoice
            | Self::NetValueMismatch
            | Self::VatValueMismatch
            | Self::GrossValueMismatch
            | Self::NetValueInvalid
            | Self::VatValueInvalid
            | Self::GrossValueInvalid
            | Self::ProformaNotFound
            | Self::ReceiptPrefixUsedForInvoices
            | Self::InvalidReceiptPrefix
            | Self::DuplicateReceiptCallId
            | Self::ReceiptPaymentMismatch
            | Self::IssueDateMustBeToday
            | Self::ReceiptGrossNotWhole
            | Self::ReceiptNetPrecision
            | Self::ReceiptVatPrecision
            | Self::PaymentOnReversedInvoice
            | Self::ErasureCodeLimit
            | Self::ErasureCodesUnavailable
            | Self::ErasureCodesDisabled
            | Self::SimplifiedImageAccountIncompatible
            | Self::SimplifiedImageItemLimit
            | Self::SimplifiedImageVatInvalid
            | Self::SimplifiedImageCannotCorrect
            | Self::SimplifiedImagePrepaymentVatMismatch
            | Self::SimplifiedImageDocumentForbidden => OutcomeClass::Rejected,
        }
    }
}

/// What a szamlazz.hu error says about the document the request asked for:
/// whether one may exist despite the error.
///
/// Answers the question a document-issuing integration must ask before it
/// retries (*may a document have been created?*), which
/// [`ErrorCode::is_retryable`] does not: invoice creation has no idempotency
/// key, so re-sending a create after a code that left the outcome open can
/// issue a duplicate legal document. Use the [operation recovery table](crate::error#recovery):
/// a document's existence cannot settle a credit-entry mutation or receipt email.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutcomeClass {
    /// szamlazz.hu refused this request before acting. Fix the request (or,
    /// for credential codes, the account). This does not prove an earlier
    /// send failed; notably 338 refuses a duplicate without recovering its result.
    Rejected,
    /// A document may or may not have been created. Reconcile before anything
    /// else: use the [operation recovery table](crate::error#recovery). An immediate
    /// empty query does not rule out an earlier send still in flight.
    Unknown,
    /// Another document already carries the order number (71/152); nothing
    /// new was created. Query by order number to find it: the message names
    /// the order number, never the existing document.
    DuplicateOrderNumber,
    /// The referenced receipt is absent (339), or data is missing (7): on an
    /// invoice query the selector matched nothing; on a write a required field
    /// or referenced document is missing. Interpret 7 for the operation, and
    /// do not infer the outcome of an earlier send from this exchange.
    NotFound,
}

/// Parses a wire code: the text is trimmed, a numeric token is matched as a
/// number (`007` is code 7), an empty token is [`ErrorCode::Absent`] and
/// anything else is [`ErrorCode::Unknown`] with the trimmed text.
impl From<&str> for ErrorCode {
    fn from(code: &str) -> Self {
        let code = code.trim();

        if code.is_empty() {
            return Self::Absent;
        }
        code.parse::<u16>()
            .ok()
            .and_then(Self::known)
            .unwrap_or_else(|| Self::Unknown(code.to_owned()))
    }
}

impl From<String> for ErrorCode {
    fn from(code: String) -> Self {
        Self::from(code.as_str())
    }
}

/// Matches the number against the known codes without formatting it; an
/// unknown number is [`ErrorCode::Unknown`] with its decimal text.
impl From<u16> for ErrorCode {
    fn from(code: u16) -> Self {
        Self::known(code).unwrap_or_else(|| Self::Unknown(code.to_string()))
    }
}

/// Parses a wire code; never fails, since an unknown code is
/// [`ErrorCode::Unknown`] and an empty one [`ErrorCode::Absent`].
impl std::str::FromStr for ErrorCode {
    type Err = std::convert::Infallible;

    fn from_str(code: &str) -> Result<Self, Self::Err> {
        Ok(Self::from(code))
    }
}

/// The wire code, or `absent` for [`ErrorCode::Absent`].
impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Absent => f.write_str("absent"),
            code => f.write_str(code.code()),
        }
    }
}

/// A derived line-item value that does not fit a [`Decimal`](rust_decimal::Decimal).
///
/// Returned by [`LineItem::try_calculated`](crate::LineItem::try_calculated).
/// Each variant names the step of the arithmetic szamlazz.hu verifies server-side
/// (net = unit price × quantity, VAT = net × rate / 100, gross = net + VAT).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ArithmeticError {
    /// A numeric VAT token cannot be represented exactly by a decimal.
    #[error("line item numeric VAT rate cannot be represented exactly by a decimal")]
    UnrepresentableVatRate,
    /// `unit_price × quantity` cannot fit exactly (overflow or precision loss).
    #[error("line item net value (unit price × quantity) cannot fit exactly in a decimal")]
    NetOverflow,
    /// An intermediate or result of `net × rate / 100` cannot fit exactly.
    #[error("line item VAT value (net × rate / 100) cannot fit exactly in a decimal")]
    VatOverflow,
    /// `net + VAT` cannot fit exactly (overflow or precision loss).
    #[error("line item gross value (net + VAT) cannot fit exactly in a decimal")]
    GrossOverflow,
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
    /// An unfinished replacing registration has no entries. The schema permits
    /// empty replacement, but it must be requested explicitly with
    /// [`ClearCreditEntries`](crate::ops::credit_entry::ClearCreditEntries).
    #[error(
        "a replacing credit-entry request needs at least one entry: with none it would clear the invoice's payments"
    )]
    EmptyCreditEntryReplace,
    /// Foreign-currency documents require the quoting bank and exchange rate.
    ///
    /// Raised for every currency but the forint (`HUF` / `Ft`, any letter
    /// case) when no `ExchangeRate` is set, on every document kind, since
    /// whether szamlazz.hu itself accepts a foreign-currency proforma or
    /// delivery note without one is unverified. A caller without a rate to
    /// quote can ask for szamlazz.hu's automatic current MNB rate with
    /// `ExchangeRate::automatic_mnb()`.
    #[error("foreign-currency documents require an exchange rate")]
    MissingExchangeRate,
    /// Exchange-rate details contain no bank, or request automatic lookup from
    /// a bank other than MNB.
    #[error("invalid foreign-currency exchange-rate details")]
    InvalidExchangeRate,
    /// A line item requests more data erasure codes than szamlazz.hu's
    /// documented per-item maximum
    /// ([`MAX_ERASURE_CODE_COUNT`](crate::MAX_ERASURE_CODE_COUNT); rejected
    /// server-side as error 537).
    #[error(
        "line item requests {0} data erasure codes; the maximum is {max}",
        max = crate::item::MAX_ERASURE_CODE_COUNT
    )]
    ErasureCodeCountOutOfRange(u32),
    /// A waybill parcel count exceeds the nonnegative XML Schema `int` range.
    #[error("waybill parcel count {0} exceeds {max}", max = i32::MAX)]
    ParcelCountOutOfRange(u32),
    /// An outbound date has a nonpositive year. Requests support years 1–9999:
    /// year zero and Jiff's negative-year spelling are not XSD 1.0 dates.
    #[error("request date `{field}` has year {year}; supported years are 1–9999")]
    InvalidDateYear {
        /// Request field path, independent of its value.
        field: &'static str,
        /// The unsupported civil year.
        year: i16,
    },
    /// A receipt line item carries a field the receipt row has no element
    /// for: `margin_vat_base`, or the ledger's `economic_event`,
    /// `vat_economic_event`, `settlement_from` or `settlement_to`, which are
    /// invoice-only. Named as the `LineItem` field path. Refused rather than
    /// silently dropped, so a value the caller set never vanishes on the
    /// wire.
    #[error("receipt line items cannot carry `{0}`: the field is invoice-only")]
    UnsupportedOnReceipt(&'static str),
    /// An operation produced bytes that are not UTF-8 XML.
    #[error("request XML is not valid UTF-8")]
    InvalidXmlEncoding,
    /// An operation contains a character forbidden by XML 1.0, given as its
    /// code point.
    ///
    /// The document is scanned once, after it is written, so the offending
    /// field is not known here: a text field of the request carries the
    /// character (a `NUL` from a truncated database column is the usual
    /// case), and the caller finds it by searching its own values for the
    /// code point.
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
    ///
    /// The body is quoted as a [bounded excerpt](body_excerpt), never whole.
    #[error("unexpected response body: {0}")]
    UnexpectedBody(String),
}

/// The most of a response body an error message quotes.
///
/// An upstream body that is not szamlazz.hu's answer (a proxy's HTML page,
/// a stack trace) ends up in error displays, and from there in a consumer's
/// logs, faults or journal. A bounded prefix keeps those readable and
/// bounded; the length is noted so the truncation is visible.
pub const BODY_EXCERPT_LEN: usize = 256;

/// A bounded, lossy-UTF-8 excerpt of a response body for an error message:
/// the whole body when it fits in [`BODY_EXCERPT_LEN`] bytes, otherwise a
/// prefix on a character boundary with the total length noted. A blank body
/// reads as `empty response`.
///
/// Every excerpt this crate's errors quote ([`ParseError::UnexpectedBody`],
/// [`ResponseError::HttpStatus`]) goes through here. Public so that an
/// integration with its own HTTP client, logging a
/// [`RawResponse`](crate::wire::RawResponse) body of its own (a status its
/// parsers never saw, a body it rejected before parsing), quotes it under the
/// same bound rather than whole.
#[must_use]
pub fn body_excerpt(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    let text = text.trim();

    if text.is_empty() {
        return "empty response".to_owned();
    }
    if text.len() <= BODY_EXCERPT_LEN {
        return text.to_owned();
    }
    let mut cut = BODY_EXCERPT_LEN;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }

    format!("{}… [truncated: {} bytes]", &text[..cut], body.len())
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
    /// A non-2xx status, checked before the body when neither a nonblank
    /// `szlahu_down` nor an error-code header took precedence. A success-number
    /// or unrelated `szlahu_*` header does not bypass this check. The status
    /// does not identify whether szamlazz.hu or an intermediary answered.
    ///
    /// Raised only when the client supplied the status
    /// ([`RawResponse::with_status`](crate::wire::RawResponse::with_status));
    /// szamlazz.hu's own in-band answer (`szlahu_error_code`, `szlahu_down`)
    /// is read first whatever the status. Its [outcome
    /// class](Self::outcome_class) is `Unknown`: a gateway timeout may have
    /// cut a request the server went on to act on.
    #[error("HTTP {status} before body interpretation: {body}")]
    HttpStatus {
        /// The HTTP status.
        status: u16,
        /// A [bounded excerpt](body_excerpt) of the body.
        body: String,
    },
    /// The response could not be parsed.
    #[error(transparent)]
    Parse(#[from] ParseError),
}

impl ResponseError {
    /// What this failure says about the document the request asked for: may
    /// one exist despite the error? See [`OutcomeClass`].
    ///
    /// An API error's class is its [`ErrorCode::outcome_class`]. Unavailability
    /// (`szlahu_down`), a non-2xx status reached before body interpretation
    /// and an unparseable response are [`OutcomeClass::Unknown`]: szamlazz.hu
    /// produced no answer the caller can conclude from, so a document may
    /// have been issued. See the [operation recovery table](crate::error#recovery)
    /// for the distinct receipt, mutation, deletion and email cases.
    #[must_use]
    pub fn outcome_class(&self) -> OutcomeClass {
        match self {
            Self::Api(api) => api.code.outcome_class(),
            Self::ServiceUnavailable(_) | Self::HttpStatus { .. } | Self::Parse(_) => {
                OutcomeClass::Unknown
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The original catalogue; the thirteen #195 additions have a source-derived
    /// public-parser table in `tests/error_classification.rs`.
    const ORIGINAL_NAMED: [ErrorCode; 30] = [
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
        for code in ORIGINAL_NAMED {
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
        for code in ORIGINAL_NAMED {
            let numeric: u16 = code.code().parse().expect("named codes are numeric");
            assert_eq!(ErrorCode::from(numeric), code, "{code:?}");
            assert_eq!(
                code.code().parse::<ErrorCode>(),
                Ok(code.clone()),
                "{code:?}"
            );
        }
        assert_eq!(
            ErrorCode::from(999_u16),
            ErrorCode::Unknown("999".to_owned())
        );
        assert_eq!(
            ErrorCode::from("007"),
            ErrorCode::MissingData,
            "a number is a number"
        );
        assert_eq!(
            ErrorCode::from("70000"),
            ErrorCode::Unknown("70000".to_owned())
        );
    }

    /// A failure without a code is `Absent`: parsed from an empty token, the
    /// empty string as its wire code, `absent` in a display, and the outcome
    /// left open like any code the crate cannot read.
    #[test]
    fn an_absent_code_is_honest_about_itself() {
        assert_eq!(ErrorCode::from(""), ErrorCode::Absent);
        assert_eq!(ErrorCode::from("  "), ErrorCode::Absent);
        assert_eq!("".parse::<ErrorCode>(), Ok(ErrorCode::Absent));
        assert_eq!(ErrorCode::Absent.code(), "");
        assert_eq!(ErrorCode::Absent.to_string(), "absent");
        assert_eq!(ErrorCode::Absent.outcome_class(), OutcomeClass::Unknown);
        assert!(!ErrorCode::Absent.is_retryable());
        assert!(!ErrorCode::Absent.is_credential_error());
        assert!(!ORIGINAL_NAMED.contains(&ErrorCode::Absent));
        let error = ApiError {
            code: ErrorCode::Absent,
            message: "Hiba".to_owned(),
        };
        assert_eq!(error.to_string(), "szamlazz.hu error absent: Hiba");
    }

    /// The excerpt keeps a short body whole, cuts a long one on a character
    /// boundary (a multi-byte character straddling the limit is dropped, not
    /// split), and notes the total length.
    #[test]
    fn body_excerpt_is_bounded_and_char_safe() {
        assert_eq!(body_excerpt(b"  short  "), "short");
        assert_eq!(
            body_excerpt(&[0xff, b'x']),
            "\u{FFFD}x",
            "lossy, never a panic"
        );

        let ascii = "a".repeat(BODY_EXCERPT_LEN);
        assert_eq!(
            body_excerpt(ascii.as_bytes()),
            ascii,
            "exactly the limit fits"
        );

        // 255 ASCII bytes then a 2-byte `é` straddling byte 256.
        let straddling = format!("{}é tail", "a".repeat(BODY_EXCERPT_LEN - 1));
        let excerpt = body_excerpt(straddling.as_bytes());
        assert!(
            excerpt.starts_with(&"a".repeat(BODY_EXCERPT_LEN - 1)),
            "{excerpt}"
        );
        assert!(!excerpt.contains('é'), "{excerpt}");
        assert!(
            excerpt.ends_with(&format!("… [truncated: {} bytes]", straddling.len())),
            "{excerpt}"
        );
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
        for code in ORIGINAL_NAMED {
            let expected = matches!(
                code,
                ErrorCode::Maintenance | ErrorCode::EInvoiceSigningFailed
            );
            assert_eq!(code.is_retryable(), expected, "{code:?}");
        }
        assert!(!ErrorCode::Unknown("999".to_owned()).is_retryable());
    }

    /// The original catalogue's classes, from documentation and observations:
    /// after which a document may exist are 1, 55 and 56 (the latter surfaces
    /// as an error only without a number); 71/152 name an existing document;
    /// 7 is "not on the query surface"; every other code refuses before acting.
    #[test]
    fn original_catalogue_has_an_outcome_class() {
        let table: [(ErrorCode, OutcomeClass); 30] = [
            (ErrorCode::Maintenance, OutcomeClass::Unknown),
            (ErrorCode::InvalidCredentials, OutcomeClass::Rejected),
            (ErrorCode::MissingData, OutcomeClass::NotFound),
            (ErrorCode::StornoOfReversalInvoice, OutcomeClass::Rejected),
            (ErrorCode::XmlNotAFile, OutcomeClass::Rejected),
            (ErrorCode::EInvoiceNotEnabled, OutcomeClass::Rejected),
            (ErrorCode::EInvoiceSigningFailed, OutcomeClass::Unknown),
            (
                ErrorCode::InvoiceNotificationDeliveryFailed,
                OutcomeClass::Unknown,
            ),
            (ErrorCode::MalformedXml, OutcomeClass::Rejected),
            (
                ErrorCode::DuplicateOrderNumber,
                OutcomeClass::DuplicateOrderNumber,
            ),
            (
                ErrorCode::PrepaymentInvoiceNotIdentifiable,
                OutcomeClass::Rejected,
            ),
            (ErrorCode::BrowserSessionActive, OutcomeClass::Rejected),
            (ErrorCode::LoginBlocked, OutcomeClass::Rejected),
            (
                ErrorCode::DuplicateOrderNumberNamed,
                OutcomeClass::DuplicateOrderNumber,
            ),
            (ErrorCode::MultipleAccounts, OutcomeClass::Rejected),
            (ErrorCode::UnregisteredPrefix, OutcomeClass::Rejected),
            (ErrorCode::HasCorrectiveInvoice, OutcomeClass::Rejected),
            (ErrorCode::NetValueMismatch, OutcomeClass::Rejected),
            (ErrorCode::VatValueMismatch, OutcomeClass::Rejected),
            (ErrorCode::GrossValueMismatch, OutcomeClass::Rejected),
            (ErrorCode::NetValueInvalid, OutcomeClass::Rejected),
            (ErrorCode::VatValueInvalid, OutcomeClass::Rejected),
            (ErrorCode::GrossValueInvalid, OutcomeClass::Rejected),
            (ErrorCode::ProformaNotFound, OutcomeClass::Rejected),
            (ErrorCode::DuplicateReceiptCallId, OutcomeClass::Rejected),
            (ErrorCode::IssueDateMustBeToday, OutcomeClass::Rejected),
            (ErrorCode::PaymentOnReversedInvoice, OutcomeClass::Rejected),
            (ErrorCode::ErasureCodeLimit, OutcomeClass::Rejected),
            (ErrorCode::ErasureCodesUnavailable, OutcomeClass::Rejected),
            (ErrorCode::ErasureCodesDisabled, OutcomeClass::Rejected),
        ];
        assert_eq!(
            table.len(),
            ORIGINAL_NAMED.len(),
            "the table covers the original catalogue"
        );
        for (code, expected) in table {
            assert!(
                ORIGINAL_NAMED.contains(&code),
                "{code:?} is in the original catalogue"
            );
            assert_eq!(code.outcome_class(), expected, "{code:?}");
        }
    }

    /// The credential codes are exactly 3, 135, 136 and 164: what szamlazz.hu
    /// answers about the agent key before it looks at the request. Every
    /// other named code, and an unknown one, is about the request; the
    /// credential codes are a subset of the rejected class.
    #[test]
    fn credential_codes_are_the_four_login_codes() {
        let credential = [
            ErrorCode::InvalidCredentials,
            ErrorCode::BrowserSessionActive,
            ErrorCode::LoginBlocked,
            ErrorCode::MultipleAccounts,
        ];
        for code in ORIGINAL_NAMED {
            assert_eq!(
                code.is_credential_error(),
                credential.contains(&code),
                "{code:?}"
            );
        }
        for code in &credential {
            assert_eq!(code.outcome_class(), OutcomeClass::Rejected, "{code:?}");
        }
        assert_eq!(
            credential.map(|code| code.code().to_owned()),
            ["3", "135", "136", "164"]
        );
        // Read off the wire, the code is the same variant.
        assert!(ErrorCode::from("135").is_credential_error());
        assert!(!ErrorCode::Unknown("3x".to_owned()).is_credential_error());
    }

    /// A code the crate does not know may be a refusal or a new "issued, but…"
    /// code: the conservative answer is that a document may exist.
    #[test]
    fn unknown_codes_classify_as_unknown() {
        assert_eq!(
            ErrorCode::Unknown("999".to_owned()).outcome_class(),
            OutcomeClass::Unknown
        );
        assert_eq!(
            ErrorCode::from("FUTURE_CODE").outcome_class(),
            OutcomeClass::Unknown
        );
    }

    /// A response error's class is its code's when szamlazz.hu answered, and
    /// `Unknown` when it did not: `szlahu_down` and an unparseable body both
    /// leave the outcome open.
    #[test]
    fn response_errors_without_an_answer_leave_the_outcome_open() {
        let rejected = ResponseError::Api(ApiError {
            code: ErrorCode::NetValueMismatch,
            message: "net".to_owned(),
        });
        assert_eq!(rejected.outcome_class(), OutcomeClass::Rejected);

        let open = ResponseError::Api(ApiError {
            code: ErrorCode::EInvoiceSigningFailed,
            message: "signing".to_owned(),
        });
        assert_eq!(open.outcome_class(), OutcomeClass::Unknown);

        let down = ResponseError::ServiceUnavailable("maintenance".to_owned());
        assert_eq!(down.outcome_class(), OutcomeClass::Unknown);

        let parse = ResponseError::Parse(ParseError::Missing("szamlaszam"));
        assert_eq!(parse.outcome_class(), OutcomeClass::Unknown);

        let proxy = ResponseError::HttpStatus {
            status: 502,
            body: "<html/>".to_owned(),
        };
        assert_eq!(proxy.outcome_class(), OutcomeClass::Unknown);
        assert!(proxy.to_string().starts_with("HTTP 502"));
    }
}
