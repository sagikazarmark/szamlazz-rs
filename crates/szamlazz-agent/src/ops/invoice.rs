//! Invoice creation (`xmlszamla`): invoices, proformas, prepayment/final
//! invoices, corrective invoices, and delivery notes.

use jiff::civil::Date;
use rust_decimal::Decimal;

use crate::credentials::Credentials;
use crate::error::{ApiError, ParseError, RequestError, ResponseError};
use crate::item::LineItem;
use crate::types::{Currency, InvoiceNumber, Language, PaymentMethod, Pdf, TaxpayerStatus};
use crate::wire::{AgentRequest, MultipartFile, RawResponse};
use crate::xml;

/// What kind of document the invoice operation issues.
///
/// The wire encodes these as independent boolean flags (`dijbekero`,
/// `elolegszamla`, …); this enum makes the meaningless combinations
/// unrepresentable and attaches the per-kind required references.
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
    /// A delivery note (`szállítólevél`) — a non-financial document.
    #[doc(alias = "szállítólevél")]
    DeliveryNote,
    /// A prepayment (advance) invoice (`előlegszámla`).
    #[doc(alias = "előlegszámla")]
    Prepayment,
    /// A final invoice (`végszámla`) settling a prepayment invoice.
    #[doc(alias = "végszámla")]
    Final {
        /// The prepayment invoice being settled (`elolegSzamlaszam`), if
        /// referenced explicitly.
        prepayment_number: Option<InvoiceNumber>,
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
}

/// Exchange rate information, required on non-HUF documents.
#[doc(alias = "árfolyam")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct ExchangeRate {
    /// The quoting bank (`arfolyamBank`), e.g. `MNB`.
    pub bank: String,
    /// The rate (`arfolyam`). May be omitted only for automatic current-rate
    /// MNB lookup.
    pub rate: Option<Decimal>,
}

impl ExchangeRate {
    /// An exchange rate quoted by `bank`.
    pub fn new(bank: impl Into<String>, rate: Decimal) -> Self {
        Self {
            bank: bank.into(),
            rate: Some(rate),
        }
    }

    /// Uses Számlázz.hu's automatic current MNB exchange-rate lookup.
    #[must_use]
    pub fn automatic_mnb() -> Self {
        Self {
            bank: "MNB".to_owned(),
            rate: None,
        }
    }
}

/// Invoice PDF template (`szamlaSablon`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum InvoiceTemplate {
    /// `SzlaMost`.
    Most,
    /// `SzlaAlap`.
    Default,
    /// `SzlaNoEnv`.
    NoEnvelope,
    /// `Szla8cm`.
    EightCentimeter,
    /// `SzlaTomb`.
    Continuous,
    /// `SzlaFuvarlevelesAlap`, the delivery-note invoice layout.
    DeliveryNote,
    /// A future or account-specific template token.
    Other(String),
}

impl InvoiceTemplate {
    /// The exact `szamlaSablon` wire token.
    #[must_use]
    pub fn as_wire(&self) -> &str {
        match self {
            Self::Most => "SzlaMost",
            Self::Default => "SzlaAlap",
            Self::NoEnvelope => "SzlaNoEnv",
            Self::EightCentimeter => "Szla8cm",
            Self::Continuous => "SzlaTomb",
            Self::DeliveryNote => "SzlaFuvarlevelesAlap",
            Self::Other(value) => value,
        }
    }
}

/// Buyer general-ledger metadata (`vevoFokonyv`).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
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

/// Trans-O-Flex carrier data (`fuvarlevel` / `tof`).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct TransOFlex {
    /// Carrier-provided five-digit identifier (`azonosito`).
    pub id: Option<String>,
    /// Shipment identifier (`shipmentID`).
    pub shipment_id: Option<String>,
    /// Number of parcels (`csomagszam`).
    pub parcel_count: Option<u32>,
    /// Destination country code (`countryCode`).
    pub country_code: Option<String>,
    /// Destination ZIP code (`zip`).
    pub zip: Option<String>,
    /// Service code (`service`).
    pub service: Option<String>,
}

/// Pick Pack Pont carrier data (`fuvarlevel` / `ppp`).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct PickPackPoint {
    /// Barcode prefix (`vonalkodPrefix`).
    pub barcode_prefix: Option<String>,
    /// Per-invoice barcode suffix (`vonalkodPostfix`).
    pub barcode_suffix: Option<String>,
}

/// Sprinter carrier data (`fuvarlevel` / `sprinter`).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct Sprinter {
    /// Agreed carrier identifier (`azonosito`).
    pub id: Option<String>,
    /// Sender code (`feladokod`).
    pub sender_code: Option<String>,
    /// Routing code (`iranykod`).
    pub routing_code: Option<String>,
    /// Number of parcels (`csomagszam`).
    pub parcel_count: Option<u32>,
    /// Per-invoice barcode suffix (`vonalkodPostfix`).
    pub barcode_suffix: Option<String>,
    /// Delivery-time text (`szallitasiIdo`).
    pub delivery_time: Option<String>,
}

/// MPL carrier data (`fuvarlevel` / `mpl`).
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct Mpl {
    /// MPL customer code (`vevokod`).
    pub customer_code: String,
    /// Barcode source (`vonalkod`).
    pub barcode: String,
    /// Parcel weight (`tomeg`).
    pub weight: String,
    /// Extra-service icon configuration (`kulonszolgaltatasok`).
    pub extra_services: Option<String>,
    /// Declared value (`erteknyilvanitas`).
    pub declared_value: Option<Decimal>,
}

impl Mpl {
    /// Creates MPL data with the three XSD-required fields.
    pub fn new(
        customer_code: impl Into<String>,
        barcode: impl Into<String>,
        weight: impl Into<String>,
    ) -> Self {
        Self {
            customer_code: customer_code.into(),
            barcode: barcode.into(),
            weight: weight.into(),
            extra_services: None,
            declared_value: None,
        }
    }
}

/// Optional carrier waybill block (`fuvarlevel`).
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct Waybill {
    /// Legacy destination (`uticel`).
    pub destination: Option<String>,
    /// Carrier service token (`futarSzolgalat`).
    pub carrier: Option<String>,
    /// General barcode (`vonalkod`).
    pub barcode: Option<String>,
    /// Waybill comment (`megjegyzes`).
    pub comment: Option<String>,
    /// Trans-O-Flex details (`tof`).
    pub trans_o_flex: Option<TransOFlex>,
    /// Pick Pack Pont details (`ppp`).
    pub pick_pack_point: Option<PickPackPoint>,
    /// Sprinter details (`sprinter`).
    pub sprinter: Option<Sprinter>,
    /// MPL details (`mpl`).
    pub mpl: Option<Mpl>,
}

/// Invoice header (`fejlec`): dates, payment terms, and identifiers.
#[doc(alias = "fejléc")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct InvoiceHeader {
    /// Issue date (`keltDatum`). `None` lets szamlazz.hu use today.
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
    pub payable_adjustment: Option<Decimal>,
    /// Marks the invoice as already paid (`fizetve`).
    #[serde(default)]
    pub paid: bool,
    /// Apply margin-scheme VAT (`arresAfa`).
    pub margin_vat: Option<bool>,
    /// VAT belongs to another EU member state (`eusAfa`).
    pub eu_vat: Option<bool>,
    /// Invoice PDF template (`szamlaSablon`).
    pub template: Option<InvoiceTemplate>,
    /// Return a preview PDF without issuing the document (`elonezetpdf`).
    pub preview_pdf: Option<bool>,
}

impl InvoiceHeader {
    /// A header with the required fields; optional fields default to absent.
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
            paid: false,
            margin_vat: None,
            eu_vat: None,
            template: None,
            preview_pdf: None,
        }
    }
}

/// Seller (`elado`) details. Everything is optional: the account's own data
/// is used where fields are absent.
#[doc(alias = "eladó")]
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
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

/// Settings for the notification email szamlazz.hu sends to the buyer.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct SellerEmail {
    /// Reply-to address (`emailReplyto`).
    pub reply_to: Option<String>,
    /// Subject (`emailTargy`).
    pub subject: Option<String>,
    /// Body (`emailSzoveg`); supports `BBCode` (`[b]`, `[i]`, `[h1]`…).
    pub body: Option<String>,
}

/// Postal/delivery address of the buyer (`postazasi*` fields).
#[doc(alias = "postázási cím")]
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
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
#[non_exhaustive]
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
    /// Partner identifier from the account's partner database (`azonosito`).
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
#[non_exhaustive]
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

    /// Whether no files are attached.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
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
/// is always requested in structured form (response version 2); the PDF, when
/// [`CreateInvoice::download_pdf`] is set, arrives decoded in
/// [`InvoiceCreationResult::pdf`].
#[doc(alias = "xmlszamla")]
#[doc(alias = "számla készítés")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct CreateInvoice {
    /// The document kind to issue.
    pub kind: InvoiceKind,
    /// Issue an e-invoice (`eszamla`); requires the subscription feature.
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
    /// Optional waybill/carrier data (`fuvarlevel`).
    pub waybill: Option<Waybill>,
    /// Line items; at least one is required.
    pub items: Vec<LineItem>,
    /// Files attached to the buyer email (`attachfile1`…`attachfile5`).
    #[serde(default)]
    pub attachments: InvoiceAttachments,
}

impl CreateInvoice {
    /// An invoice-creation request with the required blocks; optional fields
    /// (`e_invoice`, `download_pdf`, `external_id`, `seller`) default to
    /// absent and can be set on the returned value.
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

/// A successful invoice-creation result, including PDF previews that do not
/// issue a numbered document.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct InvoiceCreationResult {
    /// The assigned invoice number (`szamlaszam`). Absent for PDF previews,
    /// which do not issue a document.
    pub invoice_number: Option<InvoiceNumber>,
    /// szamlazz.hu's internal document identifier (`szlahu_id` header).
    ///
    /// The same value the XML query returns as
    /// [`InvoiceInfo::id`](crate::ops::query_xml::InvoiceInfo::id); it is a
    /// document identifier, not an account or supplier identifier, and is
    /// distinct for every issued document. `None` when the header is absent
    /// (as on PDF previews) or not a number.
    pub document_id: Option<u64>,
    /// Net total (`szamlanetto`).
    pub net_total: Option<Decimal>,
    /// Gross total (`szamlabrutto`).
    pub gross_total: Option<Decimal>,
    /// Outstanding amount (`kintlevoseg`).
    #[doc(alias = "kintlévőség")]
    pub outstanding: Option<Decimal>,
    /// Buyer-facing account/payment URL (`vevoifiokurl`).
    pub customer_account_url: Option<String>,
    /// The document PDF, when requested.
    pub pdf: Option<Pdf>,
    /// Whether the invoice was issued but Számlázz.hu could not deliver its
    /// notification (`56`). The issued invoice must not be retried.
    pub notification_delivery_failed: bool,
}

/// A successfully issued numbered invoice, such as a storno invoice.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct CreatedInvoice {
    /// The assigned invoice number (`szamlaszam`).
    pub invoice_number: InvoiceNumber,
    /// szamlazz.hu's internal document identifier (`szlahu_id` header).
    ///
    /// The same value the XML query returns as
    /// [`InvoiceInfo::id`](crate::ops::query_xml::InvoiceInfo::id); for a
    /// storno invoice, the original's identifier reappears as the storno's
    /// [`economic_event_id`](crate::ops::query_xml::InvoiceInfo::economic_event_id).
    /// A document identifier, not an account or supplier identifier. `None`
    /// when the header is absent or not a number.
    pub document_id: Option<u64>,
    /// Net total (`szamlanetto`).
    pub net_total: Option<Decimal>,
    /// Gross total (`szamlabrutto`).
    pub gross_total: Option<Decimal>,
    /// Outstanding amount (`kintlevoseg`).
    #[doc(alias = "kintlévőség")]
    pub outstanding: Option<Decimal>,
    /// Buyer-facing account/payment URL (`vevoifiokurl`).
    pub customer_account_url: Option<String>,
    /// The document PDF, when requested.
    pub pdf: Option<Pdf>,
    /// Whether the invoice was issued but Számlázz.hu could not deliver its
    /// notification (`56`). The issued invoice must not be retried.
    pub notification_delivery_failed: bool,
}

impl CreatedInvoice {
    /// Whether this document is a reversal of `original`: a *different*
    /// invoice number with a negative gross total.
    ///
    /// The check every caller must make after a
    /// [`StornoInvoice`](crate::ops::storno::StornoInvoice): szamlazz.hu
    /// answers a storno request for a proforma or a delivery note with a
    /// success-shaped response that merely echoes the requested document
    /// (same number, positive totals) and reverses nothing. A repeat storno of
    /// an already reversed invoice also passes this check — it echoes the
    /// existing storno invoice, which is a genuine reversal.
    ///
    /// Returns `false` when the gross total is unknown.
    #[must_use]
    pub fn reverses(&self, original: &InvoiceNumber) -> bool {
        self.invoice_number != *original
            && self
                .gross_total
                .is_some_and(|gross| gross.is_sign_negative())
    }
}

impl AgentRequest for CreateInvoice {
    const ACTION: &'static str = "action-xmlagentxmlfile";
    type Response = InvoiceCreationResult;

    fn validate(&self) -> Result<(), RequestError> {
        if self.items.is_empty() {
            return Err(RequestError::MissingLineItems);
        }
        if let InvoiceKind::Final { prepayment_number } = &self.kind {
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
        if let Some(waybill) = &self.waybill {
            for count in [
                waybill
                    .trans_o_flex
                    .as_ref()
                    .and_then(|carrier| carrier.parcel_count),
                waybill
                    .sprinter
                    .as_ref()
                    .and_then(|carrier| carrier.parcel_count),
            ]
            .into_iter()
            .flatten()
            {
                if count > i32::MAX as u32 {
                    return Err(RequestError::ParcelCountOutOfRange(count));
                }
            }
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
                s.text("valaszVerzio", "2");
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
                match &self.kind {
                    InvoiceKind::Invoice { proforma_number } => {
                        f.text_opt(
                            "dijbekeroSzamlaszam",
                            proforma_number.as_ref().map(InvoiceNumber::as_str),
                        );
                    }
                    InvoiceKind::Prepayment => f.bool("elolegszamla", true),
                    InvoiceKind::Final { prepayment_number } => {
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
                if h.paid {
                    f.bool("fizetve", true);
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
                root.node("fuvarlevel", |w| {
                    w.text_opt("uticel", waybill.destination.as_deref());
                    w.text_opt("futarSzolgalat", waybill.carrier.as_deref());
                    w.text_opt("vonalkod", waybill.barcode.as_deref());
                    w.text_opt("megjegyzes", waybill.comment.as_deref());
                    if let Some(tof) = &waybill.trans_o_flex {
                        w.node("tof", |c| {
                            c.text_opt("azonosito", tof.id.as_deref());
                            c.text_opt("shipmentID", tof.shipment_id.as_deref());
                            if let Some(count) = tof.parcel_count {
                                c.text("csomagszam", &count.to_string());
                            }
                            c.text_opt("countryCode", tof.country_code.as_deref());
                            c.text_opt("zip", tof.zip.as_deref());
                            c.text_opt("service", tof.service.as_deref());
                        });
                    }
                    if let Some(ppp) = &waybill.pick_pack_point {
                        w.node("ppp", |c| {
                            c.text_opt("vonalkodPrefix", ppp.barcode_prefix.as_deref());
                            c.text_opt("vonalkodPostfix", ppp.barcode_suffix.as_deref());
                        });
                    }
                    if let Some(sprinter) = &waybill.sprinter {
                        w.node("sprinter", |c| {
                            c.text_opt("azonosito", sprinter.id.as_deref());
                            c.text_opt("feladokod", sprinter.sender_code.as_deref());
                            c.text_opt("iranykod", sprinter.routing_code.as_deref());
                            if let Some(count) = sprinter.parcel_count {
                                c.text("csomagszam", &count.to_string());
                            }
                            c.text_opt("vonalkodPostfix", sprinter.barcode_suffix.as_deref());
                            c.text_opt("szallitasiIdo", sprinter.delivery_time.as_deref());
                        });
                    }
                    if let Some(mpl) = &waybill.mpl {
                        w.node("mpl", |c| {
                            c.text("vevokod", &mpl.customer_code);
                            c.text("vonalkod", &mpl.barcode);
                            c.text("tomeg", &mpl.weight);
                            c.text_opt("kulonszolgaltatasok", mpl.extra_services.as_deref());
                            if let Some(value) = mpl.declared_value {
                                c.decimal("erteknyilvanitas", value);
                            }
                        });
                    }
                });
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

    fn parse(&self, response: &RawResponse) -> Result<Self::Response, ResponseError> {
        let result = parse_creation_result(response)?;

        if self.header.preview_pdf != Some(true) && result.invoice_number.is_none() {
            return Err(ParseError::Missing("szamlaszam").into());
        }

        Ok(result)
    }

    fn multipart_files(&self) -> Vec<MultipartFile<'_>> {
        self.attachments
            .as_slice()
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

/// Parses the common `xmlszamlavalasz` (response version 2) body, preserving
/// the optional invoice number used by preview responses.
pub(crate) fn parse_creation_result(
    response: &RawResponse,
) -> Result<InvoiceCreationResult, ResponseError> {
    response.check_available()?;
    let header_error = response.header_error();

    if let Some(error) = &header_error
        && error.code != crate::ErrorCode::InvoiceNotificationDeliveryFailed
    {
        return Err(error.clone().into());
    }

    let valasz = match InvoiceResponse::from_body(response.body()) {
        Ok(valasz) => valasz,
        Err(parse_error) => {
            return parse_notification_fallback(response, header_error.as_ref(), parse_error);
        }
    };
    let body_error = (!valasz.sikeres).then(|| valasz.api_error());

    if let Some(error) = &body_error
        && error.code != crate::ErrorCode::InvoiceNotificationDeliveryFailed
    {
        return Err(error.clone().into());
    }

    let notification_delivery_failed = header_error.is_some() || body_error.is_some();
    let invoice_number = valasz
        .szamlaszam
        .as_deref()
        .and_then(nonblank_invoice_number)
        .or_else(|| {
            response
                .szlahu("szlahu_szamlaszam")
                .as_deref()
                .and_then(nonblank_invoice_number)
        });

    if notification_delivery_failed && invoice_number.is_none() {
        if let Some(error) = header_error.or(body_error) {
            return Err(error.into());
        }
        return Err(ParseError::Missing("notification error").into());
    }

    let net_total = decimal_body_or_header(valasz.szamlanetto, response, "szlahu_nettovegosszeg");
    let gross_total =
        decimal_body_or_header(valasz.szamlabrutto, response, "szlahu_bruttovegosszeg");
    let outstanding = decimal_body_or_header(valasz.kintlevoseg, response, "szlahu_kintlevoseg");
    let pdf = valasz
        .pdf
        .filter(|value| !value.is_empty())
        .map(|encoded| Pdf::from_base64(&encoded))
        .transpose();

    Ok(InvoiceCreationResult {
        invoice_number,
        document_id: parse_document_id_header(response),
        net_total: if notification_delivery_failed {
            net_total.unwrap_or(None)
        } else {
            net_total?
        },
        gross_total: if notification_delivery_failed {
            gross_total.unwrap_or(None)
        } else {
            gross_total?
        },
        outstanding: if notification_delivery_failed {
            outstanding.unwrap_or(None)
        } else {
            outstanding?
        },
        customer_account_url: valasz.vevoifiokurl.filter(|s| !s.is_empty()).or_else(|| {
            response
                .szlahu("szlahu_vevoifiokurl")
                .filter(|s| !s.is_empty())
        }),
        pdf: if notification_delivery_failed {
            pdf.unwrap_or(None)
        } else {
            pdf?
        },
        notification_delivery_failed,
    })
}

fn parse_notification_fallback(
    response: &RawResponse,
    header_error: Option<&ApiError>,
    parse_error: ParseError,
) -> Result<InvoiceCreationResult, ResponseError> {
    let body_notification = MinimalInvoiceResponse::from_body(response.body()).ok();
    let notification_error = header_error
        .filter(|error| error.code == crate::ErrorCode::InvoiceNotificationDeliveryFailed)
        .cloned()
        .or_else(|| {
            body_notification
                .as_ref()
                .filter(|body| {
                    body.hibakod.as_deref().map(crate::ErrorCode::from)
                        == Some(crate::ErrorCode::InvoiceNotificationDeliveryFailed)
                })
                .map(MinimalInvoiceResponse::api_error)
        });
    let Some(notification_error) = notification_error else {
        return Err(parse_error.into());
    };
    let invoice_number = response
        .szlahu("szlahu_szamlaszam")
        .as_deref()
        .and_then(nonblank_invoice_number)
        .or_else(|| {
            body_notification
                .as_ref()
                .and_then(|body| body.szamlaszam.as_deref())
                .and_then(nonblank_invoice_number)
        });
    let Some(invoice_number) = invoice_number else {
        return Err(notification_error.into());
    };

    Ok(InvoiceCreationResult {
        invoice_number: Some(invoice_number),
        document_id: parse_document_id_header(response),
        net_total: parse_decimal_header(response, "szlahu_nettovegosszeg").unwrap_or(None),
        gross_total: parse_decimal_header(response, "szlahu_bruttovegosszeg").unwrap_or(None),
        outstanding: parse_decimal_header(response, "szlahu_kintlevoseg").unwrap_or(None),
        customer_account_url: response
            .szlahu("szlahu_vevoifiokurl")
            .filter(|url| !url.is_empty()),
        pdf: None,
        notification_delivery_failed: true,
    })
}

fn nonblank_invoice_number(value: &str) -> Option<InvoiceNumber> {
    let value = value.trim();
    (!value.is_empty()).then(|| InvoiceNumber::new(value))
}

/// The document identifier from the `szlahu_id` header.
///
/// Lenient on purpose: the identifier is auxiliary, and a successful issuance
/// must never be reported as a parse failure because of it. An absent, blank,
/// or non-numeric header is `None`.
fn parse_document_id_header(response: &RawResponse) -> Option<u64> {
    response
        .header("szlahu_id")
        .and_then(|value| value.trim().parse().ok())
}

fn parse_decimal_header(
    response: &RawResponse,
    name: &'static str,
) -> Result<Option<Decimal>, ParseError> {
    response
        .header(name)
        .map(|value| {
            value
                .trim()
                .parse()
                .map_err(|error: rust_decimal::Error| ParseError::Invalid {
                    field: name,
                    message: error.to_string(),
                })
        })
        .transpose()
}

pub(crate) fn decimal_body_or_header(
    body_value: Option<Decimal>,
    response: &RawResponse,
    header: &'static str,
) -> Result<Option<Decimal>, ParseError> {
    match body_value {
        Some(value) => Ok(Some(value)),
        None => parse_decimal_header(response, header),
    }
}

/// Parses an operation that must issue a numbered document, such as storno.
pub(crate) fn parse_issued(response: &RawResponse) -> Result<CreatedInvoice, ResponseError> {
    let result = parse_creation_result(response)?;

    Ok(CreatedInvoice {
        invoice_number: result
            .invoice_number
            .ok_or(ParseError::Missing("szamlaszam"))?,
        document_id: result.document_id,
        net_total: result.net_total,
        gross_total: result.gross_total,
        outstanding: result.outstanding,
        customer_account_url: result.customer_account_url,
        pdf: result.pdf,
        notification_delivery_failed: result.notification_delivery_failed,
    })
}

/// The `xmlszamlavalasz` response document (response version 2), shared with
/// the PDF-query operation.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct InvoiceResponse {
    #[serde(deserialize_with = "xml::de::flexible_bool")]
    pub(crate) sikeres: bool,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    pub hibakod: Option<String>,
    #[serde(default)]
    pub hibauzenet: Option<String>,
    #[serde(default)]
    pub szamlaszam: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    pub szamlanetto: Option<Decimal>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    pub szamlabrutto: Option<Decimal>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    pub kintlevoseg: Option<Decimal>,
    #[serde(default)]
    pub vevoifiokurl: Option<String>,
    #[serde(default)]
    pub pdf: Option<String>,
}

#[derive(serde::Deserialize)]
struct MinimalInvoiceResponse {
    #[serde(default)]
    hibakod: Option<String>,
    #[serde(default)]
    hibauzenet: Option<String>,
    #[serde(default)]
    szamlaszam: Option<String>,
}

impl MinimalInvoiceResponse {
    fn from_body(body: &[u8]) -> Result<Self, ParseError> {
        let text = xml::response_text(
            body,
            "xmlszamlavalasz",
            "http://www.szamlazz.hu/xmlszamlavalasz",
        )?;

        Ok(quick_xml::de::from_str(text)?)
    }

    fn api_error(&self) -> ApiError {
        ApiError {
            code: self
                .hibakod
                .as_deref()
                .map_or_else(|| crate::ErrorCode::Unknown("0".to_owned()), Into::into),
            message: self.hibauzenet.clone().unwrap_or_default(),
        }
    }
}

impl InvoiceResponse {
    pub(crate) fn from_body(body: &[u8]) -> Result<Self, ParseError> {
        let text = xml::response_text(
            body,
            "xmlszamlavalasz",
            "http://www.szamlazz.hu/xmlszamlavalasz",
        )?;

        Ok(quick_xml::de::from_str(text)?)
    }

    /// Converts a `sikeres=false` response into the reported [`ApiError`].
    pub(crate) fn into_success(self) -> Result<Self, ResponseError> {
        if self.sikeres {
            Ok(self)
        } else {
            Err(ApiError {
                code: self
                    .hibakod
                    .map_or_else(|| crate::ErrorCode::Unknown("0".to_owned()), Into::into),
                message: self.hibauzenet.unwrap_or_default(),
            }
            .into())
        }
    }

    fn api_error(&self) -> ApiError {
        ApiError {
            code: self
                .hibakod
                .as_deref()
                .map_or_else(|| crate::ErrorCode::Unknown("0".to_owned()), Into::into),
            message: self.hibauzenet.clone().unwrap_or_default(),
        }
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
            items: vec![LineItem::calculated_for_currency(
                "Eladó izé",
                dec!(1),
                "db",
                dec!(10000),
                VatRate::percent(27),
                &Currency::HUF,
            )],
            attachments: InvoiceAttachments::new(),
        }
    }

    #[test]
    fn writes_canonical_invoice_xml() {
        let xml = sample().write_xml(&Credentials::agent_key("key"));
        let expected = include_str!("../../tests/golden/xmlszamla.xml").trim_end();
        assert_eq!(String::from_utf8(xml).expect("utf-8"), expected);
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

    #[test]
    fn writes_current_optional_blocks_in_xsd_order() {
        let mut invoice = sample();
        invoice.download_copies = Some(2);
        invoice.aggregator = Some("AGG".into());
        invoice.guardian = Some(true);
        invoice.item_identifiers_on_invoice = Some(false);
        invoice.header.extra_logo = Some("LOGO".into());
        invoice.header.payable_adjustment = Some(dec!(1.5));
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
        assert!(xml.contains("<logoExtra>LOGO</logoExtra><fizetendoKorrekcio>1.5</fizetendoKorrekcio><arresAfa>true</arresAfa><eusAfa>false</eusAfa><szamlaSablon>SzlaNoEnv</szamlaSablon><elonezetpdf>true</elonezetpdf>"));
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
        let created = sample().parse(&response).expect("success");
        assert_eq!(
            created.invoice_number.as_ref().map(InvoiceNumber::as_str),
            Some("E-TST-2026-3")
        );
        assert_eq!(created.document_id, None);
        assert_eq!(created.net_total, Some(dec!(30000)));
        assert_eq!(created.gross_total, Some(dec!(38100)));
        assert!(created.pdf.is_none());
        assert!(!created.notification_delivery_failed);
    }

    /// The creation result is journal-safe: it round-trips through JSON with
    /// the PDF as base64.
    #[test]
    fn creation_result_round_trips_through_json() {
        let body = br#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><szamlaszam>E-TST-2026-3</szamlaszam><szamlanetto>30000</szamlanetto><szamlabrutto>38100</szamlabrutto><kintlevoseg>38100</kintlevoseg><vevoifiokurl>https://example.test/acct</vevoifiokurl><pdf>JVBERi0=</pdf></xmlszamlavalasz>"#;
        let response = RawResponse::new([("szlahu_id", "924307402")], body.to_vec());
        let created = sample().parse(&response).expect("success");
        assert_eq!(created.pdf.as_ref().map(Pdf::as_bytes), Some(&b"%PDF-"[..]));
        assert_eq!(created.document_id, Some(924_307_402));

        let json = serde_json::to_value(&created).expect("serialize");
        assert_eq!(json["invoice_number"], "E-TST-2026-3");
        assert_eq!(json["gross_total"], "38100");
        assert_eq!(json["pdf"], "JVBERi0=");

        let restored: InvoiceCreationResult = serde_json::from_value(json).expect("deserialize");
        assert_eq!(restored, created);
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
        let created = sample().parse(&response).expect("success");
        assert_eq!(created.document_id, Some(924_307_402));

        // The identifier is auxiliary: a blank or malformed header never
        // turns a successful issuance into a parse failure.
        for value in ["", "not-a-number", "-1"] {
            let response = RawResponse::new([("szlahu_id", value)], body.to_vec());
            let created = sample().parse(&response).expect("success");
            assert_eq!(created.document_id, None, "header {value:?}");
        }
    }

    #[test]
    fn notification_failure_keeps_document_id() {
        let response = RawResponse::new(
            [
                ("szlahu_error_code", "56"),
                ("szlahu_error", "notification failed"),
                ("szlahu_szamlaszam", "E-2026-123"),
                ("szlahu_id", "924307402"),
            ],
            b"notification failed".to_vec(),
        );
        let created = sample().parse(&response).expect("invoice was issued");
        assert_eq!(created.document_id, Some(924_307_402));
        assert!(created.notification_delivery_failed);

        let body = br#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod>56</hibakod><hibauzenet>notification failed</hibauzenet><szamlaszam>E-2026-123</szamlaszam></xmlszamlavalasz>"#;
        let response = RawResponse::new([("szlahu_id", "924307402")], body.to_vec());
        let created = sample().parse(&response).expect("invoice was issued");
        assert_eq!(created.document_id, Some(924_307_402));
        assert!(created.notification_delivery_failed);
    }

    fn created(number: &str, gross: Option<Decimal>) -> CreatedInvoice {
        CreatedInvoice {
            invoice_number: InvoiceNumber::new(number),
            document_id: None,
            net_total: None,
            gross_total: gross,
            outstanding: None,
            customer_account_url: None,
            pdf: None,
            notification_delivery_failed: false,
        }
    }

    #[test]
    fn reverses_requires_a_new_number_with_a_negative_gross() {
        let original = InvoiceNumber::new("CTEST-2026-40");

        // A genuine storno invoice (also what a repeat storno echoes).
        assert!(created("CTEST-2026-42", Some(dec!(-1270))).reverses(&original));

        // Storno of a proforma or delivery note: the requested document is
        // echoed unchanged.
        assert!(!created("CTEST-2026-40", Some(dec!(1270))).reverses(&original));
        // A different number with positive totals reversed nothing either.
        assert!(!created("CTEST-2026-41", Some(dec!(1270))).reverses(&original));
        // Same number, negative gross (not observed) is not a reversal.
        assert!(!created("CTEST-2026-40", Some(dec!(-1270))).reverses(&original));
        // Unknown totals cannot prove a reversal.
        assert!(!created("CTEST-2026-42", None).reverses(&original));
        assert!(!created("CTEST-2026-42", Some(dec!(0))).reverses(&original));
    }

    #[test]
    fn notification_failure_preserves_successful_issuance() {
        let body = br#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod>56</hibakod><hibauzenet>notification failed</hibauzenet></xmlszamlavalasz>"#;
        let response = RawResponse::new(
            [
                ("szlahu_error_code", "56"),
                ("szlahu_error", "Az+%C3%A9rtes%C3%ADt%C3%A9s+sikertelen"),
                ("szlahu_szamlaszam", "E-2026-123"),
                ("szlahu_nettovegosszeg", "30000"),
                ("szlahu_bruttovegosszeg", "38100"),
            ],
            body.to_vec(),
        );
        let created = sample().parse(&response).expect("invoice was issued");
        assert_eq!(
            created.invoice_number.as_ref().map(InvoiceNumber::as_str),
            Some("E-2026-123")
        );
        assert_eq!(created.gross_total, Some(dec!(38100)));
        assert!(created.notification_delivery_failed);
    }

    #[test]
    fn notification_failure_with_non_xml_body_preserves_successful_issuance() {
        let response = RawResponse::new(
            [
                ("szlahu_error_code", "56"),
                ("szlahu_error", "Az+%C3%A9rtes%C3%ADt%C3%A9s+sikertelen"),
                ("szlahu_szamlaszam", "E-2026-123"),
                ("szlahu_nettovegosszeg", "30000"),
                ("szlahu_bruttovegosszeg", "38100"),
                ("szlahu_kintlevoseg", "38100"),
                ("szlahu_vevoifiokurl", "https%3A%2F%2Fexample.com%2Finvoice"),
            ],
            b"notification failed".to_vec(),
        );

        let created = sample().parse(&response).expect("invoice was issued");
        assert_eq!(
            created.invoice_number.as_ref().map(InvoiceNumber::as_str),
            Some("E-2026-123")
        );
        assert_eq!(created.net_total, Some(dec!(30000)));
        assert_eq!(created.gross_total, Some(dec!(38100)));
        assert_eq!(created.outstanding, Some(dec!(38100)));
        assert_eq!(
            created.customer_account_url.as_deref(),
            Some("https://example.com/invoice")
        );
        assert!(created.pdf.is_none());
        assert!(created.notification_delivery_failed);
    }

    #[test]
    fn notification_failure_ignores_malformed_optional_headers_after_issuance() {
        let response = RawResponse::new(
            [
                ("szlahu_error_code", "56"),
                ("szlahu_error", "notification failed"),
                ("szlahu_szamlaszam", "E-2026-123"),
                ("szlahu_nettovegosszeg", "not-a-number"),
                ("szlahu_bruttovegosszeg", ""),
            ],
            b"notification failed".to_vec(),
        );

        let created = sample().parse(&response).expect("invoice was issued");
        assert_eq!(
            created.invoice_number.as_ref().map(InvoiceNumber::as_str),
            Some("E-2026-123")
        );
        assert_eq!(created.net_total, None);
        assert_eq!(created.gross_total, None);
        assert!(created.notification_delivery_failed);
    }

    #[test]
    fn notification_failure_with_xml_ignores_malformed_optional_metadata() {
        let body = br#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod>56</hibakod><hibauzenet>notification failed</hibauzenet><pdf>not-base64</pdf></xmlszamlavalasz>"#;
        let response = RawResponse::new(
            [
                ("szlahu_error_code", "56"),
                ("szlahu_error", "notification failed"),
                ("szlahu_szamlaszam", "E-2026-123"),
                ("szlahu_nettovegosszeg", "not-a-number"),
            ],
            body.to_vec(),
        );

        let created = sample().parse(&response).expect("invoice was issued");
        assert_eq!(
            created.invoice_number.as_ref().map(InvoiceNumber::as_str),
            Some("E-2026-123")
        );
        assert_eq!(created.net_total, None);
        assert!(created.pdf.is_none());
        assert!(created.notification_delivery_failed);
    }

    #[test]
    fn notification_failure_recovers_issuance_from_body_only() {
        let body = br#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod> 56 </hibakod><hibauzenet>notification failed</hibauzenet><szamlaszam>E-2026-123</szamlaszam><szamlanetto>not-a-number</szamlanetto></xmlszamlavalasz>"#;
        let response = RawResponse::new::<&str, &str>([], body.to_vec());

        let created = sample().parse(&response).expect("invoice was issued");
        assert_eq!(
            created.invoice_number.as_ref().map(InvoiceNumber::as_str),
            Some("E-2026-123")
        );
        assert_eq!(created.net_total, None);
        assert!(created.notification_delivery_failed);
    }

    #[test]
    fn notification_failure_without_invoice_number_is_an_error() {
        let body = br#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod>56</hibakod><hibauzenet>notification failed</hibauzenet></xmlszamlavalasz>"#;
        let response = RawResponse::new([("szlahu_szamlaszam", "%20")], body.to_vec());
        assert!(matches!(
            sample().parse(&response),
            Err(ResponseError::Api(api))
                if api.code == crate::ErrorCode::InvoiceNotificationDeliveryFailed
        ));
    }

    #[test]
    fn parses_preview_without_invoice_number() {
        let body = br#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><pdf>JVBERi0=</pdf></xmlszamlavalasz>"#;
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let mut request = sample();
        request.header.preview_pdf = Some(true);
        let preview = request.parse(&response).expect("preview");
        assert_eq!(preview.invoice_number, None);
        assert_eq!(preview.pdf.expect("PDF").as_bytes(), b"%PDF-");
    }

    #[test]
    fn non_preview_success_requires_invoice_number() {
        let body = br#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><pdf>JVBERi0=</pdf></xmlszamlavalasz>"#;
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        assert!(matches!(
            sample().parse(&response),
            Err(ResponseError::Parse(ParseError::Missing("szamlaszam")))
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
