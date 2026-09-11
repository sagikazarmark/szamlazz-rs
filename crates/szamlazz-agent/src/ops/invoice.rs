//! Invoice creation (`xmlszamla`): invoices, proformas, prepayment/final
//! invoices, corrective invoices, and delivery notes.

use jiff::civil::Date;
use rust_decimal::Decimal;

use super::envelope::{self, Reply};
use crate::credentials::Credentials;
use crate::error::{ParseError, RequestError, ResponseError};
use crate::item::LineItem;
use crate::types::{
    Currency, ExchangeRate, InvoiceNumber, InvoiceTemplate, Language, PaymentMethod, Pdf,
    SellerEmail, TaxpayerStatus,
};
use crate::wire::{AgentRequest, MultipartFile, RawResponse};
use crate::xml;

pub use super::envelope::CreatedInvoice;
pub use super::waybill::{Mpl, PickPackPoint, Sprinter, TransOFlex, Waybill};

/// What kind of document the invoice operation issues.
///
/// The wire uses independent boolean flags (`dijbekero`, `elolegszamla`, …)
/// and reference elements. This enum selects one document kind and attaches
/// the references exposed for that kind.
///
/// It exposes the proforma reference (`dijbekeroSzamlaszam`) on regular,
/// prepayment and final invoices; [`InvoiceKind::proforma_number`] reads it
/// uniformly. The XSD declares that reference independently of the kind
/// flags, rather than restricting it to these three kinds.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum InvoiceKind {
    /// A regular invoice (`számla`), optionally issued against a proforma.
    #[doc(alias = "számla")]
    Invoice {
        /// The proforma being invoiced (`dijbekeroSzamlaszam`), if any.
        proforma_number: Option<InvoiceNumber>,
    },
    /// A proforma / payment request (`díjbekérő`).
    #[doc(alias = "díjbekérő")]
    Proforma,
    /// A delivery note (`szállítólevél`), a non-financial document.
    #[doc(alias = "szállítólevél")]
    DeliveryNote,
    /// A prepayment (advance) invoice (`előlegszámla`), optionally issued
    /// against a proforma.
    ///
    /// szamlazz.hu also consumes a proforma that shares the prepayment
    /// invoice's order number when the reference is absent (verified); the
    /// reference makes the link explicit rather than leaving it to the order
    /// number. A prepayment invoice sent *with* the reference has not been
    /// exercised on the test account yet.
    #[doc(alias = "előlegszámla")]
    Prepayment {
        /// The proforma being invoiced (`dijbekeroSzamlaszam`), if any.
        proforma_number: Option<InvoiceNumber>,
    },
    /// A final invoice (`végszámla`) settling a prepayment invoice.
    ///
    /// szamlazz.hu links the prepayment invoice (by `prepayment_number` or
    /// by the shared order number), but does **not** net it into the final
    /// invoice's totals: a final invoice sent with the full performance as
    /// its only lines is issued for the full amount, and the buyer is billed
    /// the prepayment twice. A `végszámla` lists the full performance and
    /// deducts the prepayment as a **negative line item at the same VAT
    /// rate**; the caller supplies that line. Verified on the test account.
    /// Explicit `dijbekeroSzamlaszam` on a final invoice has not been
    /// exercised there; implicit linking does not verify that reference.
    #[doc(alias = "végszámla")]
    Final {
        /// The prepayment invoice being settled (`elolegSzamlaszam`), if
        /// referenced explicitly.
        prepayment_number: Option<InvoiceNumber>,
        /// The proforma being invoiced (`dijbekeroSzamlaszam`), if any.
        proforma_number: Option<InvoiceNumber>,
    },
    /// A corrective invoice (`helyesbítő számla`).
    #[doc(alias = "helyesbítő számla")]
    Corrective {
        /// The invoice being corrected (`helyesbitettSzamlaszam`).
        corrected_number: InvoiceNumber,
    },
}

impl InvoiceKind {
    /// A regular invoice with no proforma reference.
    #[must_use]
    pub fn invoice() -> Self {
        Self::Invoice {
            proforma_number: None,
        }
    }

    /// A prepayment invoice with no proforma reference.
    #[must_use]
    pub fn prepayment() -> Self {
        Self::Prepayment {
            proforma_number: None,
        }
    }

    /// The proforma reference (`dijbekeroSzamlaszam`) carried by this value:
    /// available on regular, prepayment and final invoice variants, and
    /// `None` for the other variants.
    #[must_use]
    pub fn proforma_number(&self) -> Option<&InvoiceNumber> {
        match self {
            Self::Invoice { proforma_number }
            | Self::Prepayment { proforma_number }
            | Self::Final {
                proforma_number, ..
            } => proforma_number.as_ref(),
            Self::Proforma | Self::DeliveryNote | Self::Corrective { .. } => None,
        }
    }
}

/// Buyer general-ledger metadata (`vevoFokonyv`).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct BuyerLedger {
    /// Accounting date (`konyvelesDatum`).
    pub accounting_date: Option<Date>,
    /// Buyer identifier (`vevoAzonosito`).
    pub buyer_id: Option<String>,
    /// Buyer general-ledger account (`vevoFokonyviSzam`).
    pub buyer_account: Option<String>,
    /// Continuous fulfillment (`folyamatosTelj`).
    pub continuous_fulfillment: Option<bool>,
    /// Settlement period start (`elszDatumTol`).
    pub settlement_from: Option<Date>,
    /// Settlement period end (`elszDatumIg`).
    pub settlement_to: Option<Date>,
}

/// Invoice header (`fejlec`): dates, payment terms, and identifiers.
#[doc(alias = "fejléc")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InvoiceHeader {
    /// Issue date (`keltDatum`). `None` lets szamlazz.hu use today.
    ///
    /// A request, not a guarantee: on the test account a create sent with
    /// yesterday's date was answered `sikeres=true` with the issued invoice's
    /// `<kelt>` set to **today**; the value is silently replaced, not
    /// rejected (a storno with a non-today `keltDatum` *is* rejected, with
    /// 352). Read the date back from the created document rather than
    /// assuming the one sent.
    #[doc(alias = "keltDatum")]
    pub issue_date: Option<Date>,
    /// Fulfillment date (`teljesitesDatum`).
    #[doc(alias = "teljesítés dátum")]
    pub fulfillment_date: Date,
    /// Payment due date (`fizetesiHataridoDatum`).
    #[doc(alias = "fizetési határidő")]
    pub due_date: Date,
    /// Payment method (`fizmod`).
    pub payment_method: PaymentMethod,
    /// Currency (`penznem`).
    pub currency: Currency,
    /// Document language (`szamlaNyelve`).
    pub language: Language,
    /// Free-text comment shown on the document (`megjegyzes`).
    pub comment: Option<String>,
    /// Exchange rate; required when the currency is not HUF.
    pub exchange_rate: Option<ExchangeRate>,
    /// Order number (`rendelesSzam`); also usable later as a query key.
    #[doc(alias = "rendelésszám")]
    pub order_number: Option<String>,
    /// Additional logo token (`logoExtra`) configured for the account.
    pub extra_logo: Option<String>,
    /// Invoice number prefix (`szamlaszamElotag`); must be pre-registered on
    /// the account (error 202 otherwise).
    #[doc(alias = "számlaszám előtag")]
    pub number_prefix: Option<String>,
    /// Adjustment to the payable total (`fizetendoKorrekcio`).
    #[serde(default, deserialize_with = "crate::number::de::optional")]
    pub payable_adjustment: Option<Decimal>,
    /// Per-document paid control (`fizetve`). `None` omits the element,
    /// leaving szamlazz.hu's payment-method/account defaults in effect;
    /// `Some(false)` and `Some(true)` send explicit false and true respectively.
    /// Omission and explicit false are not assumed to be equivalent.
    ///
    /// Migrating from the former `bool`: the constructor still omits the
    /// element, and missing or null JSON decodes as `None`. Existing serialized
    /// booleans decode as the corresponding `Some`: an old `"paid": false`
    /// now sends explicit false, whereas it previously omitted `fizetve`.
    /// Use `None` (missing or null in JSON) to retain that omission behavior.
    #[serde(default)]
    pub paid: Option<bool>,
    /// Apply margin-scheme VAT (`arresAfa`).
    pub margin_vat: Option<bool>,
    /// Indicates that the invoice contains no Hungarian VAT (`eusAfa`).
    /// When `Some(true)` is accepted, szamlazz.hu does not submit the invoice
    /// to NAV Online Invoice. The vendor permits this only for an
    /// OSS-registered seller or a seller with a non-Hungarian tax number.
    ///
    /// This does not replace the correct VAT code on each line item. The
    /// vendor states that retroactive submission is not possible if this
    /// setting was wrong. `None` omits the element; `Some(false)` sends false.
    /// See the [vendor VAT guidance](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/vat-rates).
    pub eu_vat: Option<bool>,
    /// Requested invoice PDF template (`szamlaSablon`). `None` leaves the
    /// element absent, except for [`InvoiceKind::DeliveryNote`]: the library
    /// always sends `SzlaFuvarlevelesAlap` for that kind, overriding this field.
    pub template: Option<InvoiceTemplate>,
    /// Return a preview PDF without issuing the document (`elonezetpdf`).
    pub preview_pdf: Option<bool>,
    /// Per-document simplified invoice image for tour operators (`simpleItems`).
    /// `None` omits the element; explicit false/true is sent as supplied.
    /// This changes the image, not the document kind or electronic/paper
    /// appearance. Full monetary line-item data is still sent to NAV.
    ///
    /// The [vendor's rules](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency)
    /// allow independent selection on regular invoices (including those from
    /// proformas), proformas and prepayment invoices. Final invoices inherit
    /// the prepayment's setting; stornos inherit the original's. Corrective
    /// invoices and delivery notes cannot use it, and simplified originals
    /// cannot be corrected even when the corrective omits this field.
    ///
    /// The seller must have OSS off and a Hungarian tax number. At most two
    /// items are allowed, except a final may have four (two negative and two
    /// new). Allowed VAT codes: `0`, `5`, `18`, `27`, `TAM`, `AAM`, `K.AFA`,
    /// `F.AFA`; a final's rates must match the prepayment's, though order,
    /// names and prices may differ. The server overrides the requested
    /// template with the simplified view. For `K.AFA`, the caller must put
    /// the margin-scheme information in the invoice comment.
    /// These content rules are answered by szamlazz.hu, not validated locally.
    ///
    /// The writer's tail is template → preview → simple items, following the
    /// download XSD and official PHP 2.12.4. Current EN/HU inline XSDs reverse
    /// the last two: combined-preview server acceptance remains unverified.
    #[doc(alias = "simpleItems")]
    #[serde(default)]
    pub simple_items: Option<bool>,
}

impl InvoiceHeader {
    /// A header with the required fields. `Option` fields default to `None`,
    /// including `paid`, so the writer omits `fizetve` by default.
    #[must_use]
    pub fn new(
        fulfillment_date: Date,
        due_date: Date,
        payment_method: PaymentMethod,
        currency: Currency,
        language: Language,
    ) -> Self {
        Self {
            issue_date: None,
            fulfillment_date,
            due_date,
            payment_method,
            currency,
            language,
            comment: None,
            exchange_rate: None,
            order_number: None,
            extra_logo: None,
            number_prefix: None,
            payable_adjustment: None,
            paid: None,
            margin_vat: None,
            eu_vat: None,
            template: None,
            preview_pdf: None,
            simple_items: None,
        }
    }
}

/// Seller (`elado`) details. Everything is optional: the account's own data
/// is used where fields are absent.
#[doc(alias = "eladó")]
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Seller {
    /// Bank name (`bank`).
    pub bank: Option<String>,
    /// Bank account number (`bankszamlaszam`).
    pub bank_account: Option<String>,
    /// Notification email settings for the buyer email.
    pub email: Option<SellerEmail>,
    /// Name of the signer shown on the document (`alairoNeve`).
    pub signer_name: Option<String>,
}

/// Postal/delivery address of the buyer (`postazasi*` fields).
#[doc(alias = "postázási cím")]
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct PostalAddress {
    /// Recipient name (`postazasiNev`).
    pub name: Option<String>,
    /// Country (`postazasiOrszag`).
    pub country: Option<String>,
    /// ZIP code (`postazasiIrsz`).
    pub zip: Option<String>,
    /// City (`postazasiTelepules`).
    pub city: Option<String>,
    /// Street address (`postazasiCim`).
    pub address: Option<String>,
}

/// Buyer (`vevo`) details.
#[doc(alias = "vevő")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Buyer {
    /// Name (`nev`).
    pub name: String,
    /// Country (`orszag`).
    pub country: Option<String>,
    /// ZIP code (`irsz`).
    pub zip: String,
    /// City (`telepules`).
    pub city: String,
    /// Street address (`cim`).
    pub address: String,
    /// Email address (`email`); notification is sent when present unless
    /// [`Buyer::send_email`] is `Some(false)`. Multiple recipients may be
    /// comma-separated.
    pub email: Option<String>,
    /// Whether szamlazz.hu should email the document to the buyer
    /// (`sendEmail`). `None` omits the element and leaves server defaults in
    /// effect.
    #[serde(default)]
    pub send_email: Option<bool>,
    /// Taxpayer status reported to NAV (`adoalany`).
    pub taxpayer_status: Option<TaxpayerStatus>,
    /// Hungarian tax number (`adoszam`).
    #[doc(alias = "adószám")]
    pub tax_number: Option<String>,
    /// VAT-group identifier (`csoportazonosito`).
    pub group_id: Option<String>,
    /// EU tax number (`adoszamEU`).
    pub eu_tax_number: Option<String>,
    /// Postal address, when it differs from the billing address.
    pub postal_address: Option<PostalAddress>,
    /// Buyer general-ledger metadata (`vevoFokonyv`).
    pub ledger: Option<BuyerLedger>,
    /// Partner identifier in the billing account's partner records
    /// (`azonosito`). Use an identifier for only one partner within that
    /// account; do not reuse one assigned to another buyer.
    ///
    /// When szamlazz.hu recognizes the identifier, it updates that partner
    /// with the billing data supplied in this request. Possession of the
    /// customer account link gives access to that customer account's
    /// documents, so sharing an identifier can expose another buyer's
    /// documents. This differs from the internal numeric
    /// [`BuyerInfo::id`](crate::ops::query_xml::BuyerInfo::id) returned by a query.
    /// See the [vendor's buyer annotations](https://docs.szamlazz.hu/hu/agent/generating_invoice/xml).
    pub id: Option<String>,
    /// Name of the signer on the buyer side (`alairoNeve`).
    pub signer_name: Option<String>,
    /// Phone number (`telefonszam`).
    pub phone: Option<String>,
    /// Comment (`megjegyzes`).
    pub comment: Option<String>,
}

impl Buyer {
    /// A buyer with the required fields; optional fields default to absent.
    pub fn new(
        name: impl Into<String>,
        zip: impl Into<String>,
        city: impl Into<String>,
        address: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            country: None,
            zip: zip.into(),
            city: city.into(),
            address: address.into(),
            email: None,
            send_email: None,
            taxpayer_status: None,
            tax_number: None,
            group_id: None,
            eu_tax_number: None,
            postal_address: None,
            ledger: None,
            id: None,
            signer_name: None,
            phone: None,
            comment: None,
        }
    }
}

/// A file attached to the buyer email sent for a created invoice.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EmailAttachment {
    /// Filename shown to the recipient.
    pub filename: String,
    /// Raw file bytes.
    pub content: Vec<u8>,
    /// MIME content type, for example `application/pdf`.
    pub content_type: String,
}

impl EmailAttachment {
    /// Creates an email attachment.
    pub fn new(
        filename: impl Into<String>,
        content: impl Into<Vec<u8>>,
        content_type: impl Into<String>,
    ) -> Self {
        Self {
            filename: filename.into(),
            content: content.into(),
            content_type: content_type.into(),
        }
    }
}

/// The per-attachment size limit. The docs say "2 MB"; the decimal reading
/// keeps client validation under either interpretation.
const MAX_ATTACHMENT_BYTES: usize = 2_000_000;

/// A bounded collection of at most five invoice email attachments.
///
/// Dereferences to the slice of attachments and iterates over them, so the
/// collection idioms (`len`, `is_empty`, `iter`, `for`) read as on a `Vec`;
/// growth goes through [`InvoiceAttachments::push`], which keeps the bound.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
#[serde(transparent)]
pub struct InvoiceAttachments(Vec<EmailAttachment>);

impl InvoiceAttachments {
    /// An empty attachment collection.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends an attachment, rejecting a sixth file.
    ///
    /// # Errors
    ///
    /// Returns an error for a sixth attachment or a file larger than 2 MB.
    pub fn push(&mut self, attachment: EmailAttachment) -> Result<(), AttachmentError> {
        if self.0.len() == 5 {
            return Err(AttachmentError::TooMany);
        }
        if attachment.content.len() > MAX_ATTACHMENT_BYTES {
            return Err(AttachmentError::TooLarge);
        }
        self.0.push(attachment);
        Ok(())
    }

    /// The attached files.
    #[must_use]
    pub fn as_slice(&self) -> &[EmailAttachment] {
        &self.0
    }
}

impl std::ops::Deref for InvoiceAttachments {
    type Target = [EmailAttachment];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<[EmailAttachment]> for InvoiceAttachments {
    fn as_ref(&self) -> &[EmailAttachment] {
        &self.0
    }
}

impl IntoIterator for InvoiceAttachments {
    type Item = EmailAttachment;
    type IntoIter = std::vec::IntoIter<EmailAttachment>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a InvoiceAttachments {
    type Item = &'a EmailAttachment;
    type IntoIter = std::slice::Iter<'a, EmailAttachment>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl TryFrom<Vec<EmailAttachment>> for InvoiceAttachments {
    type Error = AttachmentError;

    fn try_from(attachments: Vec<EmailAttachment>) -> Result<Self, Self::Error> {
        if attachments.len() > 5 {
            Err(AttachmentError::TooMany)
        } else if attachments
            .iter()
            .any(|attachment| attachment.content.len() > MAX_ATTACHMENT_BYTES)
        {
            Err(AttachmentError::TooLarge)
        } else {
            Ok(Self(attachments))
        }
    }
}

impl<'de> serde::Deserialize<'de> for InvoiceAttachments {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let attachments = Vec::<EmailAttachment>::deserialize(deserializer)?;
        Self::try_from(attachments).map_err(serde::de::Error::custom)
    }
}

/// Invalid invoice email attachment collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AttachmentError {
    /// Számla Agent accepts at most five files (`attachfile1`…`attachfile5`).
    #[error("an invoice email can contain at most five attachments")]
    TooMany,
    /// Számla Agent accepts at most 2 MB per attachment. The docs give the
    /// limit as "2 MB" without a byte count; this crate assumes the decimal
    /// reading (2,000,000 bytes) to stay under either interpretation.
    #[error("an invoice email attachment can contain at most 2 MB")]
    TooLarge,
}

/// The invoice-creation operation (`xmlszamla`, `action-xmlagentxmlfile`).
///
/// Issues the document kind selected by [`CreateInvoice::kind`]. The response
/// is always requested in structured form (response version 2) and is a
/// [`CreationOutcome`]: the issued document, or the preview PDF when
/// [`InvoiceHeader::preview_pdf`] asked for one. The PDF of an issued
/// document, when [`CreateInvoice::download_pdf`] is set, arrives decoded in
/// [`CreatedInvoice::pdf`].
#[doc(alias = "xmlszamla")]
#[doc(alias = "számla készítés")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateInvoice {
    /// The document kind to issue.
    pub kind: InvoiceKind,
    /// Issue an e-invoice (`eszamla`); requires the subscription feature.
    ///
    /// The issued document reports the result as its queried
    /// [`InvoiceAppearance`](crate::ops::query_xml::InvoiceAppearance): `1`
    /// (paper) for `false`, an e-invoice code (`3` observed) for `true`.
    #[doc(alias = "e-számla")]
    #[serde(default)]
    pub e_invoice: bool,
    /// Return the PDF in the response (`szamlaLetoltes`).
    #[serde(default)]
    pub download_pdf: bool,
    /// Number of copies in the downloaded PDF (`szamlaLetoltesPld`).
    ///
    /// Deprecated by szamlazz.hu: the element remains schema-valid but the
    /// server ignores it, so it no longer affects the returned PDF.
    pub download_copies: Option<u8>,
    /// Aggregator identifier (`aggregator`) for contracted integrations.
    pub aggregator: Option<String>,
    /// Guardian processing flag (`guardian`) for contracted integrations.
    pub guardian: Option<bool>,
    /// Show line-item identifiers on the invoice (`cikkazoninvoice`).
    pub item_identifiers_on_invoice: Option<bool>,
    /// External identifier for later queries by third-party systems
    /// (`szamlaKulsoAzon`).
    pub external_id: Option<String>,
    /// Header block.
    pub header: InvoiceHeader,
    /// Seller block.
    #[serde(default)]
    pub seller: Seller,
    /// Buyer block.
    pub buyer: Buyer,
    /// Optional waybill/carrier data (`fuvarlevel`), also usable on invoices
    /// with a layout that can display it; see [`Waybill`].
    pub waybill: Option<Waybill>,
    /// Line items; at least one is required.
    pub items: Vec<LineItem>,
    /// Files attached to the buyer email (`attachfile1`…`attachfile5`).
    #[serde(default)]
    pub attachments: InvoiceAttachments,
}

impl CreateInvoice {
    /// An invoice-creation request with the supplied blocks and line items.
    /// `e_invoice` and `download_pdf` default to false and are sent explicitly.
    /// Optional settings default to `None`; seller fields and attachments
    /// start empty. The seller XML container is still emitted.
    /// Set fields on the returned value, or use functional update:
    ///
    /// ```
    /// # use szamlazz_agent::ops::invoice::{Buyer, CreateInvoice, InvoiceHeader, InvoiceKind};
    /// # use szamlazz_agent::{Currency, Language, PaymentMethod};
    /// # let header = InvoiceHeader::new(
    /// #     "2026-07-04".parse().expect("date"), "2026-07-12".parse().expect("date"),
    /// #     PaymentMethod::Transfer, Currency::HUF, Language::Hungarian,
    /// # );
    /// # let buyer = Buyer::new("Example Kft.", "1111", "Budapest", "Example utca 1.");
    /// let request = CreateInvoice {
    ///     external_id: Some("shop:ORD-1:invoice".to_owned()),
    ///     ..CreateInvoice::new(InvoiceKind::invoice(), header, buyer, Vec::new())
    /// };
    /// # assert_eq!(request.external_id.as_deref(), Some("shop:ORD-1:invoice"));
    /// ```
    #[must_use]
    pub fn new(
        kind: InvoiceKind,
        header: InvoiceHeader,
        buyer: Buyer,
        items: Vec<LineItem>,
    ) -> Self {
        Self {
            kind,
            e_invoice: false,
            download_pdf: false,
            download_copies: None,
            aggregator: None,
            guardian: None,
            item_identifiers_on_invoice: None,
            external_id: None,
            header,
            seller: Seller::default(),
            buyer,
            waybill: None,
            items,
            attachments: InvoiceAttachments::new(),
        }
    }
}

/// What a successful [`CreateInvoice`] answered: a numbered document, or the
/// preview a request with [`InvoiceHeader::preview_pdf`] asked for, which
/// issues nothing.
///
/// The two shapes share no field whose meaning depends on the other: an
/// issued document always has its number (its totals and PDF are optional,
/// as the reply reports them), a preview only its PDF.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum CreationOutcome {
    /// A document was issued.
    Issued(CreatedInvoice),
    /// A preview was rendered; no document exists.
    Preview(InvoicePreview),
}

impl CreationOutcome {
    /// The issued document, or `None` for a preview.
    #[must_use]
    pub fn issued(&self) -> Option<&CreatedInvoice> {
        match self {
            Self::Issued(created) => Some(created),
            Self::Preview(_) => None,
        }
    }

    /// The issued document by value, or `None` for a preview.
    #[must_use]
    pub fn into_issued(self) -> Option<CreatedInvoice> {
        match self {
            Self::Issued(created) => Some(created),
            Self::Preview(_) => None,
        }
    }

    /// The PDF the reply carried: the preview's, or the issued document's
    /// when [`CreateInvoice::download_pdf`] asked for it.
    #[must_use]
    pub fn pdf(&self) -> Option<&Pdf> {
        match self {
            Self::Issued(created) => created.pdf.as_ref(),
            Self::Preview(preview) => Some(&preview.pdf),
        }
    }
}

/// The preview a create with [`InvoiceHeader::preview_pdf`] renders: the PDF
/// of the document as it would be issued, and nothing else, since nothing
/// was.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct InvoicePreview {
    /// The rendered PDF.
    pub pdf: Pdf,
}

impl AgentRequest for CreateInvoice {
    const ACTION: &'static str = "action-xmlagentxmlfile";
    type Response = CreationOutcome;

    fn validate(&self) -> Result<(), RequestError> {
        xml::validate_dates([
            ("header.issue_date", self.header.issue_date),
            (
                "header.fulfillment_date",
                Some(self.header.fulfillment_date),
            ),
            ("header.due_date", Some(self.header.due_date)),
        ])?;
        if let Some(ledger) = &self.buyer.ledger {
            xml::validate_dates([
                ("buyer.ledger.accounting_date", ledger.accounting_date),
                ("buyer.ledger.settlement_from", ledger.settlement_from),
                ("buyer.ledger.settlement_to", ledger.settlement_to),
            ])?;
        }
        for ledger in self.items.iter().filter_map(|item| item.ledger.as_ref()) {
            xml::validate_dates([
                ("items.ledger.settlement_from", ledger.settlement_from),
                ("items.ledger.settlement_to", ledger.settlement_to),
            ])?;
        }
        if self.items.is_empty() {
            return Err(RequestError::MissingLineItems);
        }
        if let InvoiceKind::Final {
            prepayment_number, ..
        } = &self.kind
        {
            let has_prepayment_number = prepayment_number
                .as_ref()
                .is_some_and(|number| !number.as_str().trim().is_empty());
            let has_order_number = self
                .header
                .order_number
                .as_deref()
                .is_some_and(|number| !number.trim().is_empty());

            if !has_prepayment_number && !has_order_number {
                return Err(RequestError::MissingPrepaymentReference);
            }
        }
        if let Some(count) = self
            .items
            .iter()
            .filter_map(|item| item.erasure_code_count)
            .find(|&count| count > crate::item::MAX_ERASURE_CODE_COUNT)
        {
            return Err(RequestError::ErasureCodeCountOutOfRange(count));
        }
        if let Some(count) = self
            .waybill
            .iter()
            .flat_map(Waybill::parcel_counts)
            .find(|&count| count > i32::MAX as u32)
        {
            return Err(RequestError::ParcelCountOutOfRange(count));
        }
        if !self.header.currency.is_huf() {
            let rate = self
                .header
                .exchange_rate
                .as_ref()
                .ok_or(RequestError::MissingExchangeRate)?;
            let bank = rate.bank.trim();

            if bank.is_empty() || bank != rate.bank || (rate.rate.is_none() && bank != "MNB") {
                return Err(RequestError::InvalidExchangeRate);
            }
        }

        Ok(())
    }

    // Preserving XSD order in one serializer function is clearer than splitting it.
    #[allow(clippy::too_many_lines)]
    fn write_xml(&self, credentials: &Credentials) -> Vec<u8> {
        xml::document("xmlszamla", "http://www.szamlazz.hu/xmlszamla", |root| {
            root.node("beallitasok", |s| {
                s.credentials(credentials);
                s.bool("eszamla", self.e_invoice);
                s.bool("szamlaLetoltes", self.download_pdf);
                if let Some(copies) = self.download_copies {
                    s.text("szamlaLetoltesPld", &copies.to_string());
                }
                s.text("valaszVerzio", super::RESPONSE_VERSION);
                s.text_opt("aggregator", self.aggregator.as_deref());
                if let Some(guardian) = self.guardian {
                    s.bool("guardian", guardian);
                }
                if let Some(show) = self.item_identifiers_on_invoice {
                    s.bool("cikkazoninvoice", show);
                }
                s.text_opt("szamlaKulsoAzon", self.external_id.as_deref());
            });
            root.node("fejlec", |f| {
                let h = &self.header;
                f.date_opt("keltDatum", h.issue_date);
                f.date("teljesitesDatum", h.fulfillment_date);
                f.date("fizetesiHataridoDatum", h.due_date);
                f.text("fizmod", h.payment_method.as_wire());
                f.text("penznem", h.currency.as_str());
                f.text("szamlaNyelve", h.language.as_wire());
                f.text_opt("megjegyzes", h.comment.as_deref());
                if let Some(rate) = &h.exchange_rate {
                    f.text("arfolyamBank", &rate.bank);
                    if let Some(rate) = rate.rate {
                        f.decimal("arfolyam", rate);
                    }
                }
                f.text_opt("rendelesSzam", h.order_number.as_deref());
                // `dijbekeroSzamlaszam` precedes every kind flag in the XSD
                // and is one element whichever kind carries it.
                f.text_opt(
                    "dijbekeroSzamlaszam",
                    self.kind.proforma_number().map(InvoiceNumber::as_str),
                );
                match &self.kind {
                    InvoiceKind::Invoice { .. } => {}
                    InvoiceKind::Prepayment { .. } => f.bool("elolegszamla", true),
                    InvoiceKind::Final {
                        prepayment_number, ..
                    } => {
                        f.bool("vegszamla", true);
                        f.text_opt(
                            "elolegSzamlaszam",
                            prepayment_number.as_ref().map(InvoiceNumber::as_str),
                        );
                    }
                    InvoiceKind::Corrective { corrected_number } => {
                        f.bool("helyesbitoszamla", true);
                        f.text("helyesbitettSzamlaszam", corrected_number.as_str());
                    }
                    InvoiceKind::Proforma => f.bool("dijbekero", true),
                    InvoiceKind::DeliveryNote => f.bool("szallitolevel", true),
                }
                f.text_opt("logoExtra", h.extra_logo.as_deref());
                f.text_opt("szamlaszamElotag", h.number_prefix.as_deref());
                if let Some(adjustment) = h.payable_adjustment {
                    f.decimal("fizetendoKorrekcio", adjustment);
                }
                if let Some(paid) = h.paid {
                    f.bool("fizetve", paid);
                }
                if let Some(enabled) = h.margin_vat {
                    f.bool("arresAfa", enabled);
                }
                if let Some(enabled) = h.eu_vat {
                    f.bool("eusAfa", enabled);
                }
                let template = match self.kind {
                    InvoiceKind::DeliveryNote => Some(&InvoiceTemplate::DeliveryNote),
                    _ => h.template.as_ref(),
                };

                if let Some(template) = template {
                    f.text("szamlaSablon", template.as_wire());
                }
                if let Some(preview) = h.preview_pdf {
                    f.bool("elonezetpdf", preview);
                }
                // Deliberate policy: PHP 2.12.4/download order; the current
                // inline XSDs disagree when both optional fields are present.
                if let Some(simple) = h.simple_items {
                    f.bool("simpleItems", simple);
                }
            });
            root.node("elado", |e| {
                e.text_opt("bank", self.seller.bank.as_deref());
                e.text_opt("bankszamlaszam", self.seller.bank_account.as_deref());
                if let Some(email) = &self.seller.email {
                    e.text_opt("emailReplyto", email.reply_to.as_deref());
                    e.text_opt("emailTargy", email.subject.as_deref());
                    e.text_opt("emailSzoveg", email.body.as_deref());
                }
                e.text_opt("alairoNeve", self.seller.signer_name.as_deref());
            });
            root.node("vevo", |v| {
                let b = &self.buyer;
                v.text("nev", &b.name);
                v.text_opt("orszag", b.country.as_deref());
                v.text("irsz", &b.zip);
                v.text("telepules", &b.city);
                v.text("cim", &b.address);
                v.text_opt("email", b.email.as_deref());
                if let Some(send) = b.send_email {
                    v.bool("sendEmail", send);
                }
                if let Some(status) = b.taxpayer_status {
                    v.text("adoalany", status.as_wire());
                }
                v.text_opt("adoszam", b.tax_number.as_deref());
                v.text_opt("csoportazonosito", b.group_id.as_deref());
                v.text_opt("adoszamEU", b.eu_tax_number.as_deref());
                if let Some(postal) = &b.postal_address {
                    v.text_opt("postazasiNev", postal.name.as_deref());
                    v.text_opt("postazasiOrszag", postal.country.as_deref());
                    v.text_opt("postazasiIrsz", postal.zip.as_deref());
                    v.text_opt("postazasiTelepules", postal.city.as_deref());
                    v.text_opt("postazasiCim", postal.address.as_deref());
                }
                if let Some(ledger) = &b.ledger {
                    v.node("vevoFokonyv", |l| {
                        l.date_opt("konyvelesDatum", ledger.accounting_date);
                        l.text_opt("vevoAzonosito", ledger.buyer_id.as_deref());
                        l.text_opt("vevoFokonyviSzam", ledger.buyer_account.as_deref());
                        if let Some(continuous) = ledger.continuous_fulfillment {
                            l.bool("folyamatosTelj", continuous);
                        }
                        l.date_opt("elszDatumTol", ledger.settlement_from);
                        l.date_opt("elszDatumIg", ledger.settlement_to);
                    });
                }
                v.text_opt("azonosito", b.id.as_deref());
                v.text_opt("alairoNeve", b.signer_name.as_deref());
                v.text_opt("telefonszam", b.phone.as_deref());
                v.text_opt("megjegyzes", b.comment.as_deref());
            });
            if let Some(waybill) = &self.waybill {
                root.node("fuvarlevel", |w| waybill.write(w));
            }
            root.node("tetelek", |t| {
                for item in &self.items {
                    t.node("tetel", |i| {
                        i.text("megnevezes", &item.name);
                        i.text_opt("azonosito", item.id.as_deref());
                        i.decimal("mennyiseg", item.quantity);
                        i.text("mennyisegiEgyseg", &item.unit);
                        i.decimal("nettoEgysegar", item.unit_price);
                        i.text("afakulcs", &item.vat_rate.as_wire());
                        if let Some(base) = item.margin_vat_base {
                            i.decimal("arresAfaAlap", base);
                        }
                        i.decimal("nettoErtek", item.net_value);
                        i.decimal("afaErtek", item.vat_value);
                        i.decimal("bruttoErtek", item.gross_value);
                        i.text_opt("megjegyzes", item.comment.as_deref());
                        if let Some(ledger) = &item.ledger {
                            i.node("tetelFokonyv", |l| {
                                l.text_opt("gazdasagiEsem", ledger.economic_event.as_deref());
                                l.text_opt(
                                    "gazdasagiEsemAfa",
                                    ledger.vat_economic_event.as_deref(),
                                );
                                l.text_opt(
                                    "arbevetelFokonyviSzam",
                                    ledger.revenue_account.as_deref(),
                                );
                                l.text_opt("afaFokonyviSzam", ledger.vat_account.as_deref());
                                l.date_opt("elszDatumTol", ledger.settlement_from);
                                l.date_opt("elszDatumIg", ledger.settlement_to);
                            });
                        }
                        if let Some(count) = item.erasure_code_count {
                            i.text("torloKod", &count.to_string());
                        }
                    });
                }
            });
        })
    }

    /// A numbered reply is the issued document. A success without a number is
    /// the preview this request asked for (`preview_pdf`), which must carry
    /// its PDF; without the request asking, it is a reply missing its
    /// `szamlaszam`.
    fn parse(&self, response: &RawResponse) -> Result<Self::Response, ResponseError> {
        match envelope::parse_reply(response)? {
            Reply::Issued(created) => Ok(CreationOutcome::Issued(created)),
            Reply::Unnumbered(reply) if self.header.preview_pdf == Some(true) => {
                let pdf = reply.pdf.ok_or(ParseError::Missing("pdf"))?;
                Ok(CreationOutcome::Preview(InvoicePreview { pdf }))
            }
            Reply::Unnumbered(_) => Err(ParseError::Missing("szamlaszam").into()),
        }
    }

    fn multipart_files(&self) -> Vec<MultipartFile<'_>> {
        self.attachments
            .iter()
            .enumerate()
            .map(|(index, attachment)| MultipartFile {
                name: format!("attachfile{}", index + 1),
                filename: &attachment.filename,
                content_type: &attachment.content_type,
                content: &attachment.content,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use rust_decimal::dec;

    use super::*;
    use crate::types::VatRate;

    fn sample() -> CreateInvoice {
        CreateInvoice {
            kind: InvoiceKind::invoice(),
            e_invoice: false,
            download_pdf: true,
            download_copies: None,
            aggregator: None,
            guardian: None,
            item_identifiers_on_invoice: None,
            external_id: None,
            header: InvoiceHeader::new(
                date(2026, 7, 4),
                date(2026, 7, 12),
                PaymentMethod::Transfer,
                Currency::HUF,
                Language::Hungarian,
            ),
            seller: Seller::default(),
            buyer: Buyer::new("Kovács Bt.", "2030", "Érd", "Tárnoki út 23."),
            waybill: None,
            items: vec![
                LineItem::try_calculated(
                    "Eladó izé",
                    dec!(1),
                    "db",
                    dec!(10000),
                    VatRate::percent(27),
                    crate::Rounding::minor_unit(&Currency::HUF),
                )
                .expect("fits"),
            ],
            attachments: InvoiceAttachments::new(),
        }
    }

    /// The issued document of a create's outcome; a preview is a test failure.
    fn issued(outcome: CreationOutcome) -> CreatedInvoice {
        outcome.into_issued().expect("an issued document")
    }

    #[test]
    fn writes_canonical_invoice_xml() {
        let xml = sample().write_xml(&Credentials::agent_key("key"));
        let expected = include_str!("../../tests/golden/xmlszamla.xml").trim_end();
        assert_eq!(String::from_utf8(xml).expect("utf-8"), expected);
    }

    #[test]
    fn paid_json_preserves_omission_and_explicit_boolean_wire_intent() {
        use serde_json::{Value, json};

        for method in [PaymentMethod::Transfer, PaymentMethod::Cash] {
            let mut invoice = sample();
            invoice.header.payment_method = method;
            assert_eq!(invoice.header.paid, None);
            let omitted = String::from_utf8(invoice.write_xml(&Credentials::agent_key("key")))
                .expect("utf-8");
            assert!(!omitted.contains("fizetve"));
            let mut header = serde_json::to_value(&invoice.header).expect("serialize header");
            header
                .as_object_mut()
                .expect("header object")
                .remove("paid");

            for (input, paid, element) in [
                (None, None, ""),
                (Some(Value::Null), None, ""),
                (Some(json!(false)), Some(false), "<fizetve>false</fizetve>"),
                (Some(json!(true)), Some(true), "<fizetve>true</fizetve>"),
            ] {
                let mut value = header.clone();
                if let Some(input) = input {
                    value["paid"] = input;
                }
                invoice.header = serde_json::from_value(value).expect("deserialize header");
                assert_eq!(invoice.header.paid, paid);
                let serialized = serde_json::to_value(&invoice.header).expect("serialize header");
                assert_eq!(serialized["paid"], json!(paid));
                assert_eq!(
                    serde_json::from_value::<InvoiceHeader>(serialized).expect("round-trip header"),
                    invoice.header
                );
                let xml = String::from_utf8(invoice.write_xml(&Credentials::agent_key("key")))
                    .expect("utf-8");
                assert_eq!(
                    xml,
                    omitted.replace("</fejlec>", &format!("{element}</fejlec>"))
                );
            }
        }
    }

    #[test]
    fn corrective_requires_reference_by_construction() {
        let mut invoice = sample();
        invoice.kind = InvoiceKind::Corrective {
            corrected_number: InvoiceNumber::new("E-2026-42"),
        };
        let xml =
            String::from_utf8(invoice.write_xml(&Credentials::agent_key("key"))).expect("utf-8");
        assert!(xml.contains("<helyesbitoszamla>true</helyesbitoszamla>"));
        assert!(xml.contains("<helyesbitettSzamlaszam>E-2026-42</helyesbitettSzamlaszam>"));
        assert!(!xml.contains("<dijbekero>"));
    }

    /// `dijbekeroSzamlaszam` sits between `rendelesSzam` and `elolegszamla`
    /// in the XSD (`fixtures/upstream/agent/xsd/xmlszamla.xsd`, `fejlecTipus`),
    /// so a prepayment invoice converting a proforma writes the reference
    /// before its own flag.
    #[test]
    fn prepayment_writes_the_proforma_reference_before_its_flag() {
        let mut invoice = sample();
        invoice.header.order_number = Some("ORD-1".to_owned());
        invoice.kind = InvoiceKind::Prepayment {
            proforma_number: Some(InvoiceNumber::new("D-2026-7")),
        };
        let xml =
            String::from_utf8(invoice.write_xml(&Credentials::agent_key("key"))).expect("utf-8");
        assert!(
            xml.contains(
                "<rendelesSzam>ORD-1</rendelesSzam><dijbekeroSzamlaszam>D-2026-7</dijbekeroSzamlaszam><elolegszamla>true</elolegszamla></fejlec>"
            ),
            "{xml}"
        );
    }

    #[test]
    fn prepayment_without_a_proforma_writes_the_flag_alone() {
        let mut invoice = sample();
        invoice.kind = InvoiceKind::prepayment();
        assert_eq!(invoice.kind.proforma_number(), None);
        let xml =
            String::from_utf8(invoice.write_xml(&Credentials::agent_key("key"))).expect("utf-8");
        assert!(xml.contains("<elolegszamla>true</elolegszamla>"), "{xml}");
        assert!(!xml.contains("<dijbekeroSzamlaszam>"), "{xml}");
    }

    /// The final invoice's three optional header elements in XSD order:
    /// `dijbekeroSzamlaszam`, `vegszamla`, `elolegSzamlaszam`.
    #[test]
    fn final_invoice_writes_the_proforma_reference_before_its_flag() {
        let mut invoice = sample();
        invoice.kind = InvoiceKind::Final {
            prepayment_number: Some(InvoiceNumber::new("E-2026-3")),
            proforma_number: Some(InvoiceNumber::new("D-2026-7")),
        };
        assert_eq!(
            invoice.kind.proforma_number(),
            Some(&InvoiceNumber::new("D-2026-7"))
        );
        let xml =
            String::from_utf8(invoice.write_xml(&Credentials::agent_key("key"))).expect("utf-8");
        assert!(
            xml.contains(
                "<dijbekeroSzamlaszam>D-2026-7</dijbekeroSzamlaszam><vegszamla>true</vegszamla><elolegSzamlaszam>E-2026-3</elolegSzamlaszam></fejlec>"
            ),
            "{xml}"
        );
    }

    /// Only the three kinds that can carry the reference expose one.
    #[test]
    fn proforma_number_is_none_on_the_kinds_that_cannot_carry_one() {
        for kind in [
            InvoiceKind::Proforma,
            InvoiceKind::DeliveryNote,
            InvoiceKind::Corrective {
                corrected_number: InvoiceNumber::new("E-2026-42"),
            },
        ] {
            assert_eq!(kind.proforma_number(), None, "{kind:?}");
        }
        assert_eq!(
            InvoiceKind::Invoice {
                proforma_number: Some(InvoiceNumber::new("D-1")),
            }
            .proforma_number(),
            Some(&InvoiceNumber::new("D-1"))
        );
    }

    /// The JSON shape a caller building the request from JSON (the CLI) sends:
    /// every kind that carries a proforma reference is an object with
    /// `proforma_number` (the prepayment invoice included; earlier releases
    /// wrote it as the bare string `"prepayment"`), and a `final` written by
    /// an earlier release, without `proforma_number`, still decodes.
    #[test]
    fn invoice_kind_json_shape() {
        use serde_json::json;

        for (kind, json) in [
            (
                InvoiceKind::invoice(),
                json!({"invoice": {"proforma_number": null}}),
            ),
            (
                InvoiceKind::prepayment(),
                json!({"prepayment": {"proforma_number": null}}),
            ),
            (
                InvoiceKind::Prepayment {
                    proforma_number: Some(InvoiceNumber::new("D-1")),
                },
                json!({"prepayment": {"proforma_number": "D-1"}}),
            ),
            (
                InvoiceKind::Final {
                    prepayment_number: Some(InvoiceNumber::new("E-1")),
                    proforma_number: Some(InvoiceNumber::new("D-1")),
                },
                json!({"final": {"prepayment_number": "E-1", "proforma_number": "D-1"}}),
            ),
            (InvoiceKind::Proforma, json!("proforma")),
        ] {
            assert_eq!(serde_json::to_value(&kind).expect("serialize"), json);
            assert_eq!(
                serde_json::from_value::<InvoiceKind>(json).expect("deserialize"),
                kind
            );
        }

        let legacy_final: InvoiceKind =
            serde_json::from_value(json!({"final": {"prepayment_number": "E-1"}}))
                .expect("a final without proforma_number decodes");
        assert_eq!(
            legacy_final,
            InvoiceKind::Final {
                prepayment_number: Some(InvoiceNumber::new("E-1")),
                proforma_number: None,
            }
        );
        assert!(
            serde_json::from_value::<InvoiceKind>(json!("prepayment")).is_err(),
            "the pre-#69 bare string is not accepted"
        );
    }

    #[test]
    fn writes_current_optional_blocks_in_xsd_order() {
        let mut invoice = sample();
        invoice.download_copies = Some(2);
        invoice.aggregator = Some("AGG".into());
        invoice.guardian = Some(true);
        invoice.item_identifiers_on_invoice = Some(false);
        invoice.header.extra_logo = Some("LOGO".into());
        invoice.header.payable_adjustment = Some(dec!(1.5));
        invoice.header.paid = Some(false);
        invoice.header.margin_vat = Some(true);
        invoice.header.eu_vat = Some(false);
        invoice.header.template = Some(InvoiceTemplate::NoEnvelope);
        invoice.header.preview_pdf = Some(true);
        invoice.buyer.send_email = Some(false);
        invoice.buyer.group_id = Some("GROUP-1".into());
        invoice.buyer.ledger = Some(BuyerLedger {
            accounting_date: Some(date(2026, 7, 5)),
            buyer_id: Some("BUYER-1".into()),
            buyer_account: Some("311".into()),
            continuous_fulfillment: Some(true),
            settlement_from: Some(date(2026, 7, 1)),
            settlement_to: Some(date(2026, 7, 31)),
        });
        let item = &mut invoice.items[0];
        item.id = Some("ITEM-1".into());
        item.margin_vat_base = Some(dec!(9000));
        item.comment = Some("row".into());
        item.ledger = Some(crate::LineItemLedger {
            economic_event: Some("SALE".into()),
            vat_economic_event: Some("VAT".into()),
            revenue_account: Some("911".into()),
            vat_account: Some("467".into()),
            settlement_from: Some(date(2026, 7, 1)),
            settlement_to: Some(date(2026, 7, 31)),
        });
        item.erasure_code_count = Some(123);
        invoice.waybill = Some(Waybill {
            destination: Some("Depot".into()),
            carrier: Some("MPL".into()),
            barcode: Some("BAR".into()),
            comment: Some("Handle".into()),
            trans_o_flex: Some(TransOFlex {
                id: Some("12345".into()),
                shipment_id: Some("SHIP".into()),
                parcel_count: Some(2),
                country_code: Some("HU".into()),
                zip: Some("1111".into()),
                service: Some("EXP".into()),
            }),
            pick_pack_point: Some(PickPackPoint {
                barcode_prefix: Some("PPP".into()),
                barcode_suffix: Some("42".into()),
            }),
            sprinter: Some(Sprinter {
                id: Some("SPR".into()),
                sender_code: Some("1234567890".into()),
                routing_code: Some("106".into()),
                parcel_count: Some(1),
                barcode_suffix: Some("7654321".into()),
                delivery_time: Some("1 day".into()),
            }),
            mpl: Some(Mpl {
                customer_code: "MPL-C".into(),
                barcode: "MPL-B".into(),
                weight: "1.5".into(),
                extra_services: Some("A".into()),
                declared_value: Some(dec!(10000)),
            }),
        });

        let xml =
            String::from_utf8(invoice.write_xml(&Credentials::agent_key("key"))).expect("utf-8");
        assert!(xml.contains("<szamlaLetoltes>true</szamlaLetoltes><szamlaLetoltesPld>2</szamlaLetoltesPld><valaszVerzio>2</valaszVerzio><aggregator>AGG</aggregator><guardian>true</guardian><cikkazoninvoice>false</cikkazoninvoice>"));
        assert!(xml.contains("<logoExtra>LOGO</logoExtra><fizetendoKorrekcio>1.5</fizetendoKorrekcio><fizetve>false</fizetve><arresAfa>true</arresAfa><eusAfa>false</eusAfa><szamlaSablon>SzlaNoEnv</szamlaSablon><elonezetpdf>true</elonezetpdf>"));
        assert!(!xml.contains("<email></email>"));
        assert!(xml.contains("<sendEmail>false</sendEmail>"));
        assert!(xml.contains("<csoportazonosito>GROUP-1</csoportazonosito>"));
        assert!(xml.contains("<vevoFokonyv><konyvelesDatum>2026-07-05</konyvelesDatum><vevoAzonosito>BUYER-1</vevoAzonosito><vevoFokonyviSzam>311</vevoFokonyviSzam><folyamatosTelj>true</folyamatosTelj><elszDatumTol>2026-07-01</elszDatumTol><elszDatumIg>2026-07-31</elszDatumIg></vevoFokonyv>"));
        assert!(xml.contains("<fuvarlevel><uticel>Depot</uticel><futarSzolgalat>MPL</futarSzolgalat><vonalkod>BAR</vonalkod><megjegyzes>Handle</megjegyzes><tof><azonosito>12345</azonosito><shipmentID>SHIP</shipmentID><csomagszam>2</csomagszam><countryCode>HU</countryCode><zip>1111</zip><service>EXP</service></tof><ppp><vonalkodPrefix>PPP</vonalkodPrefix><vonalkodPostfix>42</vonalkodPostfix></ppp><sprinter><azonosito>SPR</azonosito><feladokod>1234567890</feladokod><iranykod>106</iranykod><csomagszam>1</csomagszam><vonalkodPostfix>7654321</vonalkodPostfix><szallitasiIdo>1 day</szallitasiIdo></sprinter><mpl><vevokod>MPL-C</vevokod><vonalkod>MPL-B</vonalkod><tomeg>1.5</tomeg><kulonszolgaltatasok>A</kulonszolgaltatasok><erteknyilvanitas>10000</erteknyilvanitas></mpl></fuvarlevel>"));
        assert!(xml.contains("<megnevezes>Eladó izé</megnevezes><azonosito>ITEM-1</azonosito><mennyiseg>1</mennyiseg>"));
        assert!(xml.contains(
            "<afakulcs>27</afakulcs><arresAfaAlap>9000</arresAfaAlap><nettoErtek>10000</nettoErtek>"
        ));
        assert!(xml.contains("<megjegyzes>row</megjegyzes><tetelFokonyv><gazdasagiEsem>SALE</gazdasagiEsem><gazdasagiEsemAfa>VAT</gazdasagiEsemAfa><arbevetelFokonyviSzam>911</arbevetelFokonyviSzam><afaFokonyviSzam>467</afaFokonyviSzam><elszDatumTol>2026-07-01</elszDatumTol><elszDatumIg>2026-07-31</elszDatumIg></tetelFokonyv><torloKod>123</torloKod>"));
    }

    #[test]
    fn delivery_note_uses_current_template_and_schema_flag() {
        let mut invoice = sample();
        invoice.kind = InvoiceKind::DeliveryNote;
        let xml =
            String::from_utf8(invoice.write_xml(&Credentials::agent_key("key"))).expect("utf-8");
        assert!(xml.contains("<szallitolevel>true</szallitolevel>"));
        assert!(xml.contains("<szamlaSablon>SzlaFuvarlevelesAlap</szamlaSablon>"));
    }

    #[test]
    fn invoice_attachments_are_exact_multipart_file_parts() {
        let mut invoice = sample();
        invoice
            .attachments
            .push(EmailAttachment::new(
                "terms.txt",
                b"one".to_vec(),
                "text/plain",
            ))
            .expect("first attachment");
        invoice
            .attachments
            .push(EmailAttachment::new(
                "data.bin",
                b"two".to_vec(),
                "application/octet-stream",
            ))
            .expect("second attachment");
        let xml = invoice.write_xml(&Credentials::agent_key("key"));
        let wire = invoice
            .to_wire(&Credentials::agent_key("key"))
            .expect("valid request");
        let boundary = "----szamlazz-agent-4f7d1a2b9c3e";
        let mut expected = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"action-xmlagentxmlfile\"; filename=\"action-xmlagentxmlfile\"\r\nContent-Type: text/xml\r\n\r\n"
        )
        .into_bytes();
        expected.extend_from_slice(&xml);
        expected.extend_from_slice(format!("\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"attachfile1\"; filename=\"terms.txt\"\r\nContent-Type: text/plain\r\n\r\none").as_bytes());
        expected.extend_from_slice(format!("\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"attachfile2\"; filename=\"data.bin\"\r\nContent-Type: application/octet-stream\r\n\r\ntwo").as_bytes());
        expected.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        assert_eq!(wire.body, expected);
    }

    /// The bounded collection reads like a slice: length, emptiness,
    /// iteration by reference and by value.
    #[test]
    fn invoice_attachments_have_the_collection_idioms() {
        let attachments = InvoiceAttachments::try_from(vec![
            EmailAttachment::new("a.txt", b"one".to_vec(), "text/plain"),
            EmailAttachment::new("b.txt", b"two".to_vec(), "text/plain"),
        ])
        .expect("two attachments");
        assert_eq!(attachments.len(), 2);
        assert!(!attachments.is_empty());
        assert_eq!(attachments.iter().count(), 2);
        assert_eq!((&attachments).into_iter().count(), 2);
        assert_eq!(attachments.as_ref().len(), 2);
        assert_eq!(attachments.as_slice()[1].filename, "b.txt");
        let names: Vec<String> = attachments
            .into_iter()
            .map(|attachment| attachment.filename)
            .collect();
        assert_eq!(names, ["a.txt", "b.txt"]);
        assert!(InvoiceAttachments::new().is_empty());
    }

    #[test]
    fn sixth_invoice_attachment_is_rejected() {
        let attachments = (0..6)
            .map(|index| EmailAttachment::new(format!("{index}.txt"), Vec::new(), "text/plain"))
            .collect::<Vec<_>>();
        assert_eq!(
            InvoiceAttachments::try_from(attachments).expect_err("too many"),
            AttachmentError::TooMany
        );
    }

    #[test]
    fn oversized_invoice_attachment_is_rejected() {
        let attachment = EmailAttachment::new(
            "large.bin",
            vec![0; MAX_ATTACHMENT_BYTES + 1],
            "application/octet-stream",
        );
        assert_eq!(
            InvoiceAttachments::try_from(vec![attachment]).expect_err("too large"),
            AttachmentError::TooLarge
        );
    }

    #[test]
    fn parses_success_response() {
        let body = include_bytes!("../../tests/synthetic/xmlszamlavalasz.xml");
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let created = issued(sample().parse(&response).expect("success"));
        assert_eq!(created.invoice_number.as_str(), "E-TST-2026-3");
        assert_eq!(created.document_id, None);
        assert_eq!(created.net_total, Some(dec!(30000)));
        assert_eq!(created.gross_total, Some(dec!(38100)));
        assert!(created.pdf.is_none());
        assert!(!created.notification_delivery_failed);
    }

    /// The creation outcome is journal-safe: it round-trips through JSON as a
    /// tagged enum with the PDF as base64.
    #[test]
    fn creation_outcome_round_trips_through_json() {
        let body = br#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><szamlaszam>E-TST-2026-3</szamlaszam><szamlanetto>30000</szamlanetto><szamlabrutto>38100</szamlabrutto><kintlevoseg>38100</kintlevoseg><vevoifiokurl>https://example.test/acct</vevoifiokurl><pdf>JVBERi0=</pdf></xmlszamlavalasz>"#;
        let response = RawResponse::new([("szlahu_id", "924307402")], body.to_vec());
        let outcome = sample().parse(&response).expect("success");
        assert_eq!(outcome.pdf().map(Pdf::as_bytes), Some(&b"%PDF-"[..]));
        let created = outcome.issued().expect("issued");
        assert_eq!(created.document_id, Some(924_307_402));

        let json = serde_json::to_value(&outcome).expect("serialize");
        assert_eq!(json["issued"]["invoice_number"], "E-TST-2026-3");
        assert_eq!(json["issued"]["gross_total"], "38100");
        assert_eq!(json["issued"]["pdf"], "JVBERi0=");

        let restored: CreationOutcome = serde_json::from_value(json).expect("deserialize");
        assert_eq!(restored, outcome);
    }

    #[test]
    fn document_id_comes_from_the_szlahu_id_header() {
        let body = include_bytes!("../../tests/synthetic/xmlszamlavalasz.xml");
        let response = RawResponse::new(
            [
                ("szlahu_szamlaszam", "E-TST-2026-3"),
                ("szlahu_id", " 924307402 "),
            ],
            body.to_vec(),
        );
        let created = issued(sample().parse(&response).expect("success"));
        assert_eq!(created.document_id, Some(924_307_402));

        // The identifier is auxiliary: a blank or malformed header never
        // turns a successful issuance into a parse failure.
        for value in ["", "not-a-number", "-1"] {
            let response = RawResponse::new([("szlahu_id", value)], body.to_vec());
            let created = issued(sample().parse(&response).expect("success"));
            assert_eq!(created.document_id, None, "header {value:?}");
        }
    }

    /// The one error a create tolerates is 56 (issued, notification not
    /// delivered), which the envelope parser reads from either channel; the
    /// create reports the issued document with the flag set. The full table
    /// is the envelope's own (`ops::envelope`).
    #[test]
    fn notification_failure_preserves_successful_issuance() {
        let response = RawResponse::new(
            [
                ("szlahu_error_code", "56"),
                ("szlahu_error", "Az+%C3%A9rtes%C3%ADt%C3%A9s+sikertelen"),
                ("szlahu_szamlaszam", "E-2026-123"),
                ("szlahu_bruttovegosszeg", "38100"),
                ("szlahu_id", "924307402"),
            ],
            b"notification failed".to_vec(),
        );
        let created = issued(sample().parse(&response).expect("invoice was issued"));
        assert_eq!(created.invoice_number.as_str(), "E-2026-123");
        assert_eq!(created.gross_total, Some(dec!(38100)));
        assert_eq!(created.document_id, Some(924_307_402));
        assert!(created.notification_delivery_failed);

        // A proxy's 502 with no szamlazz.hu header is refused by status on
        // this path too, while a 56 answered with a 500 is szamlazz.hu's.
        let proxy =
            RawResponse::new([("content-type", "text/html")], b"<html/>".to_vec()).with_status(502);
        assert!(matches!(
            sample().parse(&proxy),
            Err(ResponseError::HttpStatus { status: 502, .. })
        ));
        let answered = response.with_status(500);
        assert!(
            issued(sample().parse(&answered).expect("szamlazz.hu answered"))
                .notification_delivery_failed
        );
    }

    /// A success without a number is the preview the request asked for, with
    /// its PDF; a request that asked for none is missing its `szamlaszam`,
    /// and a preview request answered without a PDF is missing that.
    #[test]
    fn a_preview_is_the_pdf_and_no_document() {
        let body = br#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><pdf>JVBERi0=</pdf></xmlszamlavalasz>"#;
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let mut request = sample();
        request.header.preview_pdf = Some(true);
        let outcome = request.parse(&response).expect("preview");
        assert_eq!(outcome.issued(), None);
        assert_eq!(outcome.pdf().map(Pdf::as_bytes), Some(&b"%PDF-"[..]));
        match &outcome {
            CreationOutcome::Preview(preview) => assert_eq!(preview.pdf.as_bytes(), b"%PDF-"),
            other => panic!("expected a preview, got {other:?}"),
        }
        let json = serde_json::to_value(&outcome).expect("serialize");
        assert_eq!(json["preview"]["pdf"], "JVBERi0=");
        assert_eq!(
            serde_json::from_value::<CreationOutcome>(json).expect("deserialize"),
            outcome
        );

        assert!(matches!(
            sample().parse(&response),
            Err(ResponseError::Parse(ParseError::Missing("szamlaszam")))
        ));

        let no_pdf = br#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres></xmlszamlavalasz>"#;
        let response = RawResponse::new::<&str, &str>([], no_pdf.to_vec());
        assert!(matches!(
            request.parse(&response),
            Err(ResponseError::Parse(ParseError::Missing("pdf")))
        ));
    }

    #[test]
    fn rejects_requests_without_items() {
        let mut invoice = sample();
        invoice.items.clear();
        assert!(matches!(
            invoice.to_wire(&Credentials::agent_key("key")),
            Err(RequestError::MissingLineItems)
        ));
    }

    #[test]
    fn final_invoice_requires_a_prepayment_reference() {
        let mut invoice = sample();
        invoice.kind = InvoiceKind::Final {
            prepayment_number: None,
            proforma_number: None,
        };
        assert!(matches!(
            invoice.to_wire(&Credentials::agent_key("key")),
            Err(RequestError::MissingPrepaymentReference)
        ));

        invoice.header.order_number = Some("ORDER-1".to_owned());
        invoice
            .to_wire(&Credentials::agent_key("key"))
            .expect("order number identifies the prepayment");

        invoice.header.order_number = None;
        invoice.kind = InvoiceKind::Final {
            prepayment_number: Some(InvoiceNumber::from("E-2026-1")),
            proforma_number: None,
        };
        invoice
            .to_wire(&Credentials::agent_key("key"))
            .expect("invoice number identifies the prepayment");
    }

    #[test]
    fn rejects_xml_10_forbidden_text() {
        let mut invoice = sample();
        invoice.buyer.name = "invalid\0name".to_owned();
        assert!(matches!(
            invoice.to_wire(&Credentials::agent_key("key")),
            Err(RequestError::InvalidXmlCharacter(0))
        ));
    }

    #[test]
    fn rejects_more_than_400_erasure_codes() {
        let mut invoice = sample();
        invoice.items[0].erasure_code_count = Some(401);
        assert!(matches!(
            invoice.to_wire(&Credentials::agent_key("key")),
            Err(RequestError::ErasureCodeCountOutOfRange(401))
        ));
    }

    #[test]
    fn rejects_waybill_parcel_count_outside_xsd_int_range() {
        for trans_o_flex in [true, false] {
            let mut invoice = sample();
            invoice.waybill = Some(Waybill::default());
            if trans_o_flex {
                invoice.waybill.as_mut().expect("waybill").trans_o_flex = Some(TransOFlex {
                    parcel_count: Some(i32::MAX as u32 + 1),
                    ..TransOFlex::default()
                });
            } else {
                invoice.waybill.as_mut().expect("waybill").sprinter = Some(Sprinter {
                    parcel_count: Some(i32::MAX as u32 + 1),
                    ..Sprinter::default()
                });
            }
            assert!(matches!(
                invoice.to_wire(&Credentials::agent_key("key")),
                Err(RequestError::ParcelCountOutOfRange(_))
            ));
        }
    }

    #[test]
    fn accepts_waybill_parcel_count_at_xsd_int_maximum() {
        let mut invoice = sample();
        invoice.waybill = Some(Waybill {
            trans_o_flex: Some(TransOFlex {
                parcel_count: Some(i32::MAX as u32),
                ..TransOFlex::default()
            }),
            sprinter: Some(Sprinter {
                parcel_count: Some(i32::MAX as u32),
                ..Sprinter::default()
            }),
            ..Waybill::default()
        });
        invoice
            .to_wire(&Credentials::agent_key("key"))
            .expect("XSD int maximum is valid");
    }

    #[test]
    fn preserves_nonnumeric_invoice_error_code() {
        let body = br#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod>FUTURE_CODE</hibakod><hibauzenet>future</hibauzenet></xmlszamlavalasz>"#;
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let error = sample().parse(&response).expect_err("error");
        match error {
            ResponseError::Api(api) => {
                assert_eq!(
                    api.code,
                    crate::ErrorCode::Unknown("FUTURE_CODE".to_owned())
                );
            }
            other => panic!("expected api error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_foreign_currency_without_exchange_rate() {
        let mut invoice = sample();
        invoice.header.currency = Currency::EUR;
        assert!(matches!(
            invoice.to_wire(&Credentials::agent_key("key")),
            Err(RequestError::MissingExchangeRate)
        ));
    }

    /// The forint in any letter case is not a foreign currency: no exchange
    /// rate is demanded, and the code is sent as the caller spelled it.
    #[test]
    fn lower_case_huf_needs_no_exchange_rate() {
        let mut invoice = sample();
        invoice.header.currency = Currency::new("huf");
        let wire = invoice
            .to_wire(&Credentials::agent_key("key"))
            .expect("the forint needs no exchange rate");
        let body = String::from_utf8(wire.body).expect("UTF-8 multipart");
        assert!(body.contains("<penznem>huf</penznem>"), "sent as given");
        assert!(!body.contains("<arfolyamBank>"));
    }

    #[test]
    fn automatic_mnb_rate_omits_explicit_invoice_rate() {
        let mut invoice = sample();
        invoice.header.currency = Currency::EUR;
        invoice.header.exchange_rate = Some(ExchangeRate::automatic_mnb());
        let wire = invoice
            .to_wire(&Credentials::agent_key("key"))
            .expect("valid automatic MNB rate");
        let body = String::from_utf8(wire.body).expect("UTF-8 multipart");
        assert!(body.contains("<arfolyamBank>MNB</arfolyamBank>"));
        assert!(!body.contains("<arfolyam>"));
    }

    #[test]
    fn rejects_exchange_rate_without_bank() {
        let mut invoice = sample();
        invoice.header.currency = Currency::EUR;
        invoice.header.exchange_rate = Some(ExchangeRate::new(" ", dec!(400)));
        assert!(matches!(
            invoice.to_wire(&Credentials::agent_key("key")),
            Err(RequestError::InvalidExchangeRate)
        ));

        invoice.header.exchange_rate = Some(ExchangeRate {
            bank: " MNB ".to_owned(),
            rate: None,
        });
        assert!(matches!(
            invoice.to_wire(&Credentials::agent_key("key")),
            Err(RequestError::InvalidExchangeRate)
        ));
    }

    #[test]
    fn parses_error_response() {
        let body = include_bytes!("../../tests/synthetic/xmlszamlavalasz_error.xml");
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let error = sample().parse(&response).expect_err("error");
        match error {
            ResponseError::Api(api) => {
                assert_eq!(api.code, crate::ErrorCode::InvalidCredentials);
                assert!(api.message.contains("Synthetic login error"));
            }
            other => panic!("expected api error, got {other:?}"),
        }
    }

    #[test]
    fn header_error_takes_precedence() {
        let response = RawResponse::new(
            [("szlahu_error_code", "202"), ("szlahu_error", "prefix")],
            Vec::new(),
        );
        let error = sample().parse(&response).expect_err("error");
        match error {
            ResponseError::Api(api) => assert_eq!(api.code, crate::ErrorCode::UnregisteredPrefix),
            other => panic!("expected api error, got {other:?}"),
        }
    }
}
