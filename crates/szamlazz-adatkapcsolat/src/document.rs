//! The pushed document types, deserialized leniently: unknown elements are
//! ignored and empty elements read as absent, so schema evolution on the
//! szamlazz.hu side does not break receivers. Date fields are the
//! business-level civil [`Date`] type; XML Schema's optional `xs:date`
//! timezone suffix (`Z`/`±hh:mm`) is accepted on the wire and discarded, so
//! a schema-valid push can never fail the whole delivery over an offset a
//! civil date cannot represent.

use std::fmt;
use std::sync::Arc;

use jiff::civil::Date;
use quick_xml::Reader;
use quick_xml::events::Event;
use quick_xml::name::{Namespace, ResolveResult};
use quick_xml::reader::NsReader;
use rust_decimal::Decimal;

use crate::error::ParseError;

/// One pushed document, identified by the XML root element.
#[derive(Debug)]
#[non_exhaustive]
pub enum Document {
    /// An outgoing invoice (`<szamla>`), pushed within ~15 minutes of issue.
    #[doc(alias = "kimenő számla")]
    OutgoingInvoice(InvoiceDocument),
    /// An incoming (received) invoice (`<szamlabe>`).
    #[doc(alias = "bejövő számla")]
    IncomingInvoice(InvoiceDocument),
    /// A bank transaction (`<banktranz>`), pushed in periodic batches — one
    /// transaction per request.
    #[doc(alias = "banki tranzakció")]
    BankTransaction(BankTransaction),
    /// A daily receipt batch (`<xmlnyugtaarchiv>`).
    #[doc(alias = "nyugta")]
    Receipts(ReceiptBatch),
}

impl Document {
    /// Parses a pushed request body, dispatching on the root element.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid UTF-8 or XML, an unknown root or wrong
    /// namespace, invalid embedded data, or missing required structure.
    pub fn parse(body: &[u8]) -> Result<Self, ParseError> {
        let root = Self::preflight(body)?;
        Self::parse_preflighted(body, root)
    }

    /// Validates UTF-8, XML well-formedness, the root, and every element's
    /// namespace without deserializing embedded payloads such as PDFs.
    pub(crate) fn preflight(body: &[u8]) -> Result<RootKind, ParseError> {
        let text = std::str::from_utf8(body)?;
        let root = root_kind(text)?;
        validate_element_namespaces(text, root)?;

        Ok(root)
    }

    pub(crate) fn parse_preflighted(body: &[u8], root: RootKind) -> Result<Self, ParseError> {
        let text = std::str::from_utf8(body)?;
        let raw_xml: Arc<str> = Arc::from(text);

        match root {
            RootKind::OutgoingInvoice => {
                let mut invoice: InvoiceDocument = quick_xml::de::from_str(text)?;
                invoice.validate(InvoiceKind::Outgoing)?;
                invoice.raw_xml = Some(raw_xml);
                Ok(Self::OutgoingInvoice(invoice))
            }
            RootKind::IncomingInvoice => {
                let mut invoice: InvoiceDocument = quick_xml::de::from_str(text)?;
                invoice.validate(InvoiceKind::Incoming)?;
                invoice.raw_xml = Some(raw_xml);
                Ok(Self::IncomingInvoice(invoice))
            }
            RootKind::BankTransaction => {
                let mut transaction: BankTransaction = quick_xml::de::from_str(text)?;
                transaction.raw_xml = Some(raw_xml);
                Ok(Self::BankTransaction(transaction))
            }
            RootKind::Receipts => {
                let mut batch: ReceiptBatch = quick_xml::de::from_str(text)?;
                batch.validate()?;
                batch.raw_xml = Some(raw_xml);
                Ok(Self::Receipts(batch))
            }
        }
    }
}

fn validate_element_namespaces(text: &str, kind: RootKind) -> Result<(), ParseError> {
    let mut reader = NsReader::from_str(text);

    loop {
        let (namespace, event) = reader
            .read_resolved_event()
            .map_err(quick_xml::DeError::from)?;

        match event {
            Event::Start(element) | Event::Empty(element) => {
                let expected = Namespace(kind.namespace().as_bytes());

                if namespace != ResolveResult::Bound(expected) {
                    let actual = match namespace {
                        ResolveResult::Bound(namespace) => {
                            String::from_utf8_lossy(namespace.as_ref()).into_owned()
                        }
                        ResolveResult::Unbound => String::new(),
                        ResolveResult::Unknown(prefix) => {
                            format!(
                                "unbound prefix {}",
                                String::from_utf8_lossy(prefix.as_ref())
                            )
                        }
                    };
                    return Err(ParseError::WrongNamespace {
                        root: String::from_utf8_lossy(element.local_name().as_ref()).into_owned(),
                        expected: kind.namespace(),
                        actual,
                    });
                }
            }
            Event::Eof => return Ok(()),
            _ => {}
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum RootKind {
    OutgoingInvoice,
    IncomingInvoice,
    BankTransaction,
    Receipts,
}

impl RootKind {
    fn name(self) -> &'static str {
        match self {
            Self::OutgoingInvoice => "szamla",
            Self::IncomingInvoice => "szamlabe",
            Self::BankTransaction => "banktranz",
            Self::Receipts => "xmlnyugtaarchiv",
        }
    }

    fn namespace(self) -> &'static str {
        match self {
            Self::OutgoingInvoice => "http://www.szamlazz.hu/szamla",
            Self::IncomingInvoice => "http://www.szamlazz.hu/szamlabe",
            Self::BankTransaction => "http://www.szamlazz.hu/banktranz",
            Self::Receipts => "http://www.szamlazz.hu/xmlnyugtaarchiv",
        }
    }
}

/// Identifies a known root and verifies its exact official namespace.
pub(crate) fn root_kind(text: &str) -> Result<RootKind, ParseError> {
    let mut reader = Reader::from_str(text);

    loop {
        match reader.read_event().map_err(quick_xml::DeError::from)? {
            Event::Start(start) | Event::Empty(start) => {
                let local = start.local_name();
                let local = std::str::from_utf8(local.as_ref())?;
                let kind = match local {
                    "szamla" => RootKind::OutgoingInvoice,
                    "szamlabe" => RootKind::IncomingInvoice,
                    "banktranz" => RootKind::BankTransaction,
                    "xmlnyugtaarchiv" => RootKind::Receipts,
                    other => return Err(ParseError::UnknownRoot(other.to_owned())),
                };
                let qualified_name = start.name();
                let qualified = std::str::from_utf8(qualified_name.as_ref())?;
                let prefix = qualified
                    .strip_suffix(local)
                    .and_then(|prefix| prefix.strip_suffix(':'));
                let namespace_attribute =
                    prefix.map_or_else(|| "xmlns".to_owned(), |prefix| format!("xmlns:{prefix}"));
                let mut namespace = None;

                for attribute in start.attributes().with_checks(false) {
                    let attribute = attribute.map_err(quick_xml::DeError::from)?;
                    if attribute.key.as_ref() == namespace_attribute.as_bytes() {
                        namespace = Some(std::str::from_utf8(attribute.value.as_ref())?.to_owned());
                        break;
                    }
                }

                if namespace.as_deref() != Some(kind.namespace()) {
                    return Err(ParseError::WrongNamespace {
                        root: kind.name().to_owned(),
                        expected: kind.namespace(),
                        actual: namespace.unwrap_or_default(),
                    });
                }
                return Ok(kind);
            }
            Event::Eof => return Err(ParseError::Empty),
            _ => {}
        }
    }
}

/// The documented integer value of `<eszamla>`.
///
/// Values `2` and `3` both mean e-invoice. Unknown values are retained so a
/// future protocol extension can be archived and inspected without data loss.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum InvoiceAppearance {
    /// `0`: the document is not an invoice (for example a proforma).
    NotInvoice,
    /// `1`: paper invoice.
    Paper,
    /// `2` or `3`: e-invoice, retaining the sender's exact code.
    Electronic(i64),
    /// Any future integer code.
    Unknown(i64),
}

impl InvoiceAppearance {
    /// Returns the exact integer received from szamlazz.hu.
    #[must_use]
    pub fn code(self) -> i64 {
        match self {
            Self::NotInvoice => 0,
            Self::Paper => 1,
            Self::Electronic(code) | Self::Unknown(code) => code,
        }
    }

    /// Whether this is one of the documented e-invoice values (`2` or `3`).
    #[must_use]
    pub fn is_e_invoice(self) -> bool {
        matches!(self, Self::Electronic(2 | 3))
    }
}

/// Creates the semantic value while retaining `code` exactly.
impl From<i64> for InvoiceAppearance {
    fn from(code: i64) -> Self {
        match code {
            0 => Self::NotInvoice,
            1 => Self::Paper,
            2 | 3 => Self::Electronic(code),
            other => Self::Unknown(other),
        }
    }
}

impl serde::Serialize for InvoiceAppearance {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.code())
    }
}

/// Open VAT semantics shared by invoice and receipt lines and VAT totals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum VatRate<'a> {
    /// A NAV-defined special VAT category. This takes semantic precedence
    /// whenever `<afatipus>` is present.
    Special(&'a str),
    /// The numeric percentage from required `<afakulcs>`.
    Percentage(Decimal),
}

/// A PDF pushed inside a document, already base64-decoded.
#[derive(Clone)]
pub struct Pdf(Vec<u8>);

impl Pdf {
    /// The PDF bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Writes the PDF to a file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be created or written.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_to(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        std::fs::write(path, &self.0)
    }
}

impl fmt::Debug for Pdf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Pdf({} bytes)", self.0.len())
    }
}

impl AsRef<[u8]> for Pdf {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Unwraps the raw PDF bytes.
impl From<Pdf> for Vec<u8> {
    fn from(pdf: Pdf) -> Self {
        pdf.0
    }
}

/// Serializes as a base64 string — the wire representation.
impl serde::Serialize for Pdf {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use base64::Engine as _;
        serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(&self.0))
    }
}

/// An address block (`cim` / `postacim`).
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct Address {
    /// Recipient name (`nev`; postal addresses only).
    #[serde(default, rename(deserialize = "nev"))]
    pub name: Option<String>,
    /// Country (`orszag`).
    #[serde(default, rename(deserialize = "orszag"))]
    pub country: Option<String>,
    /// ZIP code (`irsz`).
    #[serde(default, rename(deserialize = "irsz"))]
    pub zip: Option<String>,
    /// City (`telepules`).
    #[serde(default, rename(deserialize = "telepules"))]
    pub city: Option<String>,
    /// Street address (`cim`).
    #[serde(default, rename(deserialize = "cim"))]
    pub address: Option<String>,
}

/// A party on an invoice: the supplier (`szallito`) or the buyer (`vevo`).
///
/// The two wire shapes are near-identical, so one type covers both; fields
/// that only one side carries are optional.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct Party {
    /// szamlazz.hu-internal identifier (`id`).
    #[serde(default, deserialize_with = "de::empty_as_none")]
    pub id: Option<i64>,
    /// Name (`nev`).
    #[serde(default, rename(deserialize = "nev"))]
    pub name: Option<String>,
    /// Partner identifier from the account's partner database (`azonosito`).
    #[serde(default, rename(deserialize = "azonosito"))]
    pub partner_id: Option<String>,
    /// Billing address (`cim`).
    #[serde(default, rename(deserialize = "cim"))]
    pub address: Option<Address>,
    /// Postal address (`postacim`).
    #[serde(default, rename(deserialize = "postacim"))]
    pub postal_address: Option<Address>,
    /// Email address (`email`).
    #[serde(default)]
    pub email: Option<String>,
    /// Hungarian tax number (`adoszam`).
    #[doc(alias = "adószám")]
    #[serde(default, rename(deserialize = "adoszam"))]
    pub tax_number: Option<String>,
    /// VAT-group identifier (`csoportazonosito`).
    #[serde(default, rename(deserialize = "csoportazonosito"))]
    pub group_id: Option<String>,
    /// EU tax number (`adoszameu`).
    #[serde(default, rename(deserialize = "adoszameu"))]
    pub eu_tax_number: Option<String>,
    /// Bank details (`bank`; supplier only).
    #[serde(default)]
    pub bank: Option<Bank>,
    /// Buyer location code (`lokacio`): domestic, EU, third country, or
    /// unknown. Present on buyers only.
    #[serde(
        default,
        rename(deserialize = "lokacio"),
        deserialize_with = "de::empty_as_none"
    )]
    pub location: Option<i32>,
    /// Whether the buyer is a private individual (`privatePersonIndicator`).
    #[serde(
        default,
        rename(deserialize = "privatePersonIndicator"),
        deserialize_with = "de::opt_flexible_bool"
    )]
    pub private_person: Option<bool>,
    /// Outgoing-invoice buyer ledger data (`fokonyv`).
    #[serde(default, rename(deserialize = "fokonyv"))]
    pub buyer_ledger: Option<BuyerLedger>,
}

/// Accounting data attached to the buyer of an outgoing invoice (`fokonyv`).
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct BuyerLedger {
    /// Customer ledger number (`vevo`).
    #[serde(default, rename(deserialize = "vevo"))]
    pub customer: Option<String>,
    /// Customer ledger identifier (`vevoazon`).
    #[serde(default, rename(deserialize = "vevoazon"))]
    pub customer_id: Option<String>,
    /// Accounting date (`datum`).
    #[serde(
        default,
        rename(deserialize = "datum"),
        deserialize_with = "de::opt_xs_date"
    )]
    pub date: Option<Date>,
    /// Continuous fulfillment (`folyamatostelj`).
    #[serde(
        default,
        rename(deserialize = "folyamatostelj"),
        deserialize_with = "de::opt_flexible_bool"
    )]
    pub continuous_fulfillment: Option<bool>,
    /// Settlement period start (`elszDatTol`).
    #[serde(
        default,
        rename(deserialize = "elszDatTol"),
        deserialize_with = "de::opt_xs_date"
    )]
    pub settlement_start: Option<Date>,
    /// Settlement period end (`elszDatIg`).
    #[serde(
        default,
        rename(deserialize = "elszDatIg"),
        deserialize_with = "de::opt_xs_date"
    )]
    pub settlement_end: Option<Date>,
}

/// Bank details of a party (`bank`).
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct Bank {
    /// Bank name (`nev`).
    #[serde(default, rename(deserialize = "nev"))]
    pub name: Option<String>,
    /// Account number (`bankszamla`).
    #[serde(default, rename(deserialize = "bankszamla"))]
    pub account: Option<String>,
}

/// Identity and metadata of a pushed invoice (`alap`).
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct InvoiceInfo {
    /// The document id (`id`) — this is the value an [`InvoiceAck`] must
    /// echo.
    ///
    /// [`InvoiceAck`]: crate::InvoiceAck
    pub id: i32,
    /// Invoice number (`szamlaszam`).
    #[doc(alias = "számlaszám")]
    #[serde(default, rename(deserialize = "szamlaszam"))]
    pub invoice_number: Option<String>,
    /// Economic event identifier (`gazdEsemAzon`).
    #[serde(
        default,
        rename(deserialize = "gazdEsemAzon"),
        deserialize_with = "de::empty_as_none"
    )]
    pub economic_event_id: Option<i64>,
    /// Source system code (`forras`) for invoices not issued by szamlazz.hu.
    #[serde(
        default,
        rename(deserialize = "forras"),
        deserialize_with = "de::empty_as_none"
    )]
    pub source: Option<i64>,
    /// Registration number assigned by a receiver system (`iktatoszam`).
    #[doc(alias = "iktatószám")]
    #[serde(
        default,
        rename(deserialize = "iktatoszam"),
        deserialize_with = "de::empty_string_as_none"
    )]
    pub registration_number: Option<String>,
    /// Document type code (`tipus`), e.g. `SZ`, `SS`, `D`.
    #[serde(default, rename(deserialize = "tipus"))]
    pub kind: Option<String>,
    /// Document appearance (`eszamla`): `0` not an invoice, `1` paper, and
    /// `2`/`3` e-invoice. Unknown integer values are preserved.
    #[doc(alias = "e-számla")]
    #[serde(
        default,
        rename(deserialize = "eszamla"),
        deserialize_with = "de::opt_invoice_appearance"
    )]
    pub e_invoice: Option<InvoiceAppearance>,
    /// The invoice this one reverses or corrects (`hivszamlaszam`).
    #[serde(default, rename(deserialize = "hivszamlaszam"))]
    pub referenced_invoice_number: Option<String>,
    /// The proforma this invoice was issued from (`hivdijbekszam`).
    #[serde(default, rename(deserialize = "hivdijbekszam"))]
    pub referenced_proforma_number: Option<String>,
    /// Issue date (`kelt`).
    #[serde(
        default,
        rename(deserialize = "kelt"),
        deserialize_with = "de::opt_xs_date"
    )]
    pub issue_date: Option<Date>,
    /// Fulfillment date (`telj`).
    #[serde(
        default,
        rename(deserialize = "telj"),
        deserialize_with = "de::opt_xs_date"
    )]
    pub fulfillment_date: Option<Date>,
    /// Incoming-invoice continuous fulfillment flag (`folyamatostelj`).
    #[serde(
        default,
        rename(deserialize = "folyamatostelj"),
        deserialize_with = "de::opt_flexible_bool"
    )]
    pub continuous_fulfillment: Option<bool>,
    /// Incoming-invoice settlement period start (`elszDatTol`).
    #[serde(
        default,
        rename(deserialize = "elszDatTol"),
        deserialize_with = "de::opt_xs_date"
    )]
    pub settlement_start: Option<Date>,
    /// Incoming-invoice settlement period end (`elszDatIg`).
    #[serde(
        default,
        rename(deserialize = "elszDatIg"),
        deserialize_with = "de::opt_xs_date"
    )]
    pub settlement_end: Option<Date>,
    /// Payment due date (`fizh`).
    #[serde(
        default,
        rename(deserialize = "fizh"),
        deserialize_with = "de::opt_xs_date"
    )]
    pub due_date: Option<Date>,
    /// Payment method as displayed (`fizmod`).
    #[serde(default, rename(deserialize = "fizmod"))]
    pub payment_method: Option<String>,
    /// Normalized payment method (`fizmodunified`).
    #[serde(default, rename(deserialize = "fizmodunified"))]
    pub payment_method_unified: Option<String>,
    /// Cash invoice flag (`keszpenz`).
    #[serde(
        default,
        rename(deserialize = "keszpenz"),
        deserialize_with = "de::opt_flexible_bool"
    )]
    pub cash: Option<bool>,
    /// Order number (`rendelesszam`).
    #[doc(alias = "rendelésszám")]
    #[serde(
        default,
        rename(deserialize = "rendelesszam"),
        deserialize_with = "de::empty_string_as_none"
    )]
    pub order_number: Option<String>,
    /// Document language (`nyelv`).
    #[serde(default, rename(deserialize = "nyelv"))]
    pub language: Option<String>,
    /// Currency (`devizanem`).
    #[serde(default, rename(deserialize = "devizanem"))]
    pub currency: Option<String>,
    /// Quoting bank for the exchange rate (`devizabank`).
    #[serde(default, rename(deserialize = "devizabank"))]
    pub exchange_rate_bank: Option<String>,
    /// Exchange rate (`devizaarf`).
    #[serde(
        default,
        rename(deserialize = "devizaarf"),
        deserialize_with = "de::empty_as_none"
    )]
    pub exchange_rate: Option<Decimal>,
    /// Comment (`megjegyzes`).
    #[serde(default, rename(deserialize = "megjegyzes"))]
    pub comment: Option<String>,
    /// Invoice-level VAT type (`afatipus`), used when VAT belongs to another
    /// EU member state.
    #[serde(default, rename(deserialize = "afatipus"))]
    pub vat_type: Option<String>,
    /// Cash-accounting scheme flag (`penzforg`).
    #[serde(
        default,
        rename(deserialize = "penzforg"),
        deserialize_with = "de::opt_flexible_bool"
    )]
    pub cash_accounting: Option<bool>,
    /// KATA taxpayer flag (`kata`).
    #[serde(default, deserialize_with = "de::opt_flexible_bool")]
    pub kata: Option<bool>,
    /// Whether accounting should treat the invoice under KATA
    /// (`katafokonyv`).
    #[serde(
        default,
        rename(deserialize = "katafokonyv"),
        deserialize_with = "de::opt_flexible_bool"
    )]
    pub kata_ledger: Option<bool>,
    /// Email address associated with the invoice (`email`).
    #[serde(default)]
    pub email: Option<String>,
    /// Issued by a test account (`teszt`).
    #[serde(
        default,
        rename(deserialize = "teszt"),
        deserialize_with = "de::opt_flexible_bool"
    )]
    pub test: Option<bool>,
    /// Incoming-invoice deletion marker (`dobdel`).
    #[serde(
        default,
        rename(deserialize = "dobdel"),
        deserialize_with = "de::opt_flexible_bool"
    )]
    pub deleted: Option<bool>,
    /// Whether the invoice has been reversed (`sztornozott`).
    #[doc(alias = "sztornózott")]
    #[serde(
        default,
        rename(deserialize = "sztornozott"),
        deserialize_with = "de::opt_flexible_bool"
    )]
    pub reversed: Option<bool>,
}

/// One line item of a pushed invoice (`tetel`).
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct InvoiceItem {
    /// Item name (`nev`).
    #[serde(default, rename(deserialize = "nev"))]
    pub name: Option<String>,
    /// Item identifier (`azonosito`).
    #[serde(default, rename(deserialize = "azonosito"))]
    pub id: Option<String>,
    /// Quantity (`mennyiseg`).
    #[serde(
        default,
        rename(deserialize = "mennyiseg"),
        deserialize_with = "de::empty_as_none"
    )]
    pub quantity: Option<Decimal>,
    /// Unit of measure (`mennyisegiegyseg`).
    #[serde(default, rename(deserialize = "mennyisegiegyseg"))]
    pub unit: Option<String>,
    /// Net unit price (`nettoegysegar`).
    #[serde(
        default,
        rename(deserialize = "nettoegysegar"),
        deserialize_with = "de::empty_as_none"
    )]
    pub unit_price: Option<Decimal>,
    /// Optional NAV special VAT category (`afatipus`).
    #[serde(default, rename(deserialize = "afatipus"))]
    pub vat_type: Option<String>,
    /// Required numeric VAT percentage (`afakulcs`).
    #[doc(alias = "áfakulcs")]
    #[serde(
        default,
        rename(deserialize = "afakulcs"),
        deserialize_with = "de::empty_as_none"
    )]
    pub vat_rate: Option<Decimal>,
    /// Net value (`netto`).
    #[serde(
        default,
        rename(deserialize = "netto"),
        deserialize_with = "de::empty_as_none"
    )]
    pub net_value: Option<Decimal>,
    /// Margin-scheme VAT base (`arresafaalap`).
    #[serde(
        default,
        rename(deserialize = "arresafaalap"),
        deserialize_with = "de::empty_as_none"
    )]
    pub margin_vat_base: Option<Decimal>,
    /// VAT value (`afa`).
    #[serde(
        default,
        rename(deserialize = "afa"),
        deserialize_with = "de::empty_as_none"
    )]
    pub vat_value: Option<Decimal>,
    /// Gross value (`brutto`).
    #[serde(
        default,
        rename(deserialize = "brutto"),
        deserialize_with = "de::empty_as_none"
    )]
    pub gross_value: Option<Decimal>,
    /// Comment (`megjegyzes`).
    #[serde(default, rename(deserialize = "megjegyzes"))]
    pub comment: Option<String>,
    /// Item order on the invoice (`sztetordering`).
    #[serde(
        default,
        rename(deserialize = "sztetordering"),
        deserialize_with = "de::empty_as_none"
    )]
    pub ordering: Option<i64>,
    /// Item accounting data (`fokonyv`).
    #[serde(default, rename(deserialize = "fokonyv"))]
    pub ledger: Option<InvoiceItemLedger>,
}

impl InvoiceItem {
    /// Returns effective VAT semantics. A special `<afatipus>` takes
    /// precedence while the numeric `<afakulcs>` remains available separately.
    #[must_use]
    pub fn effective_vat(&self) -> Option<VatRate<'_>> {
        effective_vat(self.vat_type.as_deref(), self.vat_rate)
    }
}

/// Accounting data attached to an invoice line (`fokonyv`).
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct InvoiceItemLedger {
    /// Revenue ledger number (`arbevetel`).
    #[serde(default, rename(deserialize = "arbevetel"))]
    pub revenue: Option<String>,
    /// VAT ledger number (`afa`).
    #[serde(default, rename(deserialize = "afa"))]
    pub vat: Option<String>,
    /// Economic-event ledger number (`gazdasagiesemeny`).
    #[serde(default, rename(deserialize = "gazdasagiesemeny"))]
    pub economic_event: Option<String>,
    /// VAT economic-event ledger number (`gazdasagiesemenyafa`).
    #[serde(default, rename(deserialize = "gazdasagiesemenyafa"))]
    pub economic_event_vat: Option<String>,
    /// Item settlement period start (`elszdattol`).
    #[serde(
        default,
        rename(deserialize = "elszdattol"),
        deserialize_with = "de::opt_xs_date"
    )]
    pub settlement_start: Option<Date>,
    /// Item settlement period end (`elszdatig`).
    #[serde(
        default,
        rename(deserialize = "elszdatig"),
        deserialize_with = "de::opt_xs_date"
    )]
    pub settlement_end: Option<Date>,
}

/// Per-VAT-rate totals (`afakulcsossz`).
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct VatTotal {
    /// Optional NAV special VAT category (`afatipus`).
    #[serde(default, rename(deserialize = "afatipus"))]
    pub vat_type: Option<String>,
    /// Required numeric VAT percentage (`afakulcs`). Absent only for the grand
    /// total, whose schema does not contain a VAT key.
    #[serde(
        default,
        rename(deserialize = "afakulcs"),
        deserialize_with = "de::empty_as_none"
    )]
    pub vat_rate: Option<Decimal>,
    /// Net total (`netto`).
    #[serde(
        default,
        rename(deserialize = "netto"),
        deserialize_with = "de::empty_as_none"
    )]
    pub net: Option<Decimal>,
    /// VAT total (`afa`).
    #[serde(
        default,
        rename(deserialize = "afa"),
        deserialize_with = "de::empty_as_none"
    )]
    pub vat: Option<Decimal>,
    /// Gross total (`brutto`).
    #[serde(
        default,
        rename(deserialize = "brutto"),
        deserialize_with = "de::empty_as_none"
    )]
    pub gross: Option<Decimal>,
}

impl VatTotal {
    /// Returns effective VAT semantics for a per-rate total.
    #[must_use]
    pub fn effective_vat(&self) -> Option<VatRate<'_>> {
        effective_vat(self.vat_type.as_deref(), self.vat_rate)
    }
}

fn effective_vat(vat_type: Option<&str>, vat_rate: Option<Decimal>) -> Option<VatRate<'_>> {
    vat_type
        .filter(|value| !value.is_empty())
        .map(VatRate::Special)
        .or_else(|| vat_rate.map(VatRate::Percentage))
}

/// Document totals (`osszegek`).
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct Totals {
    /// Totals per VAT rate (`afakulcsossz`).
    #[serde(default, rename(deserialize = "afakulcsossz"))]
    pub per_vat_rate: Vec<VatTotal>,
    /// Grand totals (`totalossz`).
    #[serde(default, rename(deserialize = "totalossz"))]
    pub grand: Option<VatTotal>,
}

/// A payment recorded on the invoice (`kifizetes`).
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct RecordedPayment {
    /// Payment date (`datum`).
    #[serde(
        default,
        rename(deserialize = "datum"),
        deserialize_with = "de::opt_xs_date"
    )]
    pub date: Option<Date>,
    /// Payment method / legal title (`jogcim`).
    #[doc(alias = "jogcím")]
    #[serde(default, rename(deserialize = "jogcim"))]
    pub method: Option<String>,
    /// Amount (`osszeg`).
    #[serde(
        default,
        rename(deserialize = "osszeg"),
        deserialize_with = "de::empty_as_none"
    )]
    pub amount: Option<Decimal>,
    /// Comment (`megjegyzes`).
    #[serde(default, rename(deserialize = "megjegyzes"))]
    pub comment: Option<String>,
    /// Bank account the payment arrived on (`bankszamlaszam`).
    #[serde(default, rename(deserialize = "bankszamlaszam"))]
    pub bank_account: Option<String>,
    /// Linked bank transaction id (`banktranzid`).
    #[serde(
        default,
        rename(deserialize = "banktranzid"),
        deserialize_with = "de::empty_as_none"
    )]
    pub bank_transaction_id: Option<i64>,
    /// Exchange rate used for this credit entry (`devizaarf`).
    #[serde(
        default,
        rename(deserialize = "devizaarf"),
        deserialize_with = "de::empty_as_none"
    )]
    pub exchange_rate: Option<Decimal>,
}

/// A financial item (`qutet`) linked to an invoice.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct FinancialItem {
    /// Name (`nev`).
    #[serde(default, rename(deserialize = "nev"))]
    pub name: Option<String>,
    /// Optional NAV special VAT category (`afatipus`).
    #[serde(default, rename(deserialize = "afatipus"))]
    pub vat_type: Option<String>,
    /// Numeric VAT percentage (`afakulcs`).
    #[serde(
        default,
        rename(deserialize = "afakulcs"),
        deserialize_with = "de::empty_as_none"
    )]
    pub vat_rate: Option<Decimal>,
    /// Net value (`netto`).
    #[serde(
        default,
        rename(deserialize = "netto"),
        deserialize_with = "de::empty_as_none"
    )]
    pub net: Option<Decimal>,
    /// VAT value (`afa`).
    #[serde(
        default,
        rename(deserialize = "afa"),
        deserialize_with = "de::empty_as_none"
    )]
    pub vat: Option<Decimal>,
    /// Gross value (`brutto`).
    #[serde(
        default,
        rename(deserialize = "brutto"),
        deserialize_with = "de::empty_as_none"
    )]
    pub gross: Option<Decimal>,
    /// Settlement period start (`elszdattol`).
    #[serde(
        default,
        rename(deserialize = "elszdattol"),
        deserialize_with = "de::opt_xs_date"
    )]
    pub settlement_start: Option<Date>,
    /// Settlement period end (`elszdatig`).
    #[serde(
        default,
        rename(deserialize = "elszdatig"),
        deserialize_with = "de::opt_xs_date"
    )]
    pub settlement_end: Option<Date>,
    /// Deductible VAT indicator (`afalevon`).
    #[serde(
        default,
        rename(deserialize = "afalevon"),
        deserialize_with = "de::empty_as_none"
    )]
    pub deductible_vat: Option<i64>,
    /// Tags (`cimkek`).
    #[serde(default, rename(deserialize = "cimkek"), deserialize_with = "de::tags")]
    pub tags: Vec<String>,
}

impl FinancialItem {
    /// Returns effective VAT semantics, preferring a special category.
    #[must_use]
    pub fn effective_vat(&self) -> Option<VatRate<'_>> {
        effective_vat(self.vat_type.as_deref(), self.vat_rate)
    }
}

/// A pushed invoice document: `<szamla>` (outgoing) or `<szamlabe>`
/// (incoming) — the two shapes are near-identical and share this type; which
/// one arrived is expressed by the [`Document`] variant / [`Handler`] method.
///
/// [`Handler`]: crate::Handler
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct InvoiceDocument {
    /// The supplier (`szallito`).
    #[doc(alias = "szállító")]
    #[serde(rename(deserialize = "szallito"))]
    pub supplier: Party,
    /// Identity and metadata (`alap`).
    #[serde(rename(deserialize = "alap"))]
    pub info: InvoiceInfo,
    /// The buyer (`vevo`).
    #[doc(alias = "vevő")]
    #[serde(rename(deserialize = "vevo"))]
    pub buyer: Party,
    /// Line items (`tetelek`).
    #[serde(
        default,
        rename(deserialize = "tetelek"),
        deserialize_with = "de::items"
    )]
    pub items: Vec<InvoiceItem>,
    /// Financial items (`qutetek`).
    #[serde(
        default,
        rename(deserialize = "qutetek"),
        deserialize_with = "de::financial_items"
    )]
    pub financial_items: Vec<FinancialItem>,
    /// Invoice-level tags (`cimkek`).
    #[serde(default, rename(deserialize = "cimkek"), deserialize_with = "de::tags")]
    pub tags: Vec<String>,
    /// Totals (`osszegek`).
    #[serde(default, rename(deserialize = "osszegek"))]
    pub totals: Totals,
    /// Recorded payments (`kifizetesek`).
    #[serde(
        default,
        rename(deserialize = "kifizetesek"),
        deserialize_with = "de::payments"
    )]
    pub payments: Vec<RecordedPayment>,
    /// The invoice PDF (`pdf`), base64 on the wire, decoded here.
    #[serde(default, deserialize_with = "de::base64_pdf")]
    pub pdf: Option<Pdf>,
    /// Exact UTF-8 XML request from which this document was parsed.
    #[serde(skip)]
    raw_xml: Option<Arc<str>>,
}

#[derive(Clone, Copy)]
enum InvoiceKind {
    Outgoing,
    Incoming,
}

impl InvoiceDocument {
    /// Returns the exact pushed XML, when this value came from [`Document::parse`].
    #[must_use]
    pub fn raw_xml(&self) -> Option<&str> {
        self.raw_xml.as_deref()
    }

    fn validate(&self, kind: InvoiceKind) -> Result<(), ParseError> {
        required_text(
            self.info.invoice_number.as_deref(),
            "invoice alap/szamlaszam",
        )?;
        required(
            self.info.economic_event_id.as_ref(),
            "invoice alap/gazdEsemAzon",
        )?;
        required_text(self.info.kind.as_deref(), "invoice alap/tipus")?;
        required(self.info.e_invoice.as_ref(), "invoice alap/eszamla")?;
        required(self.info.issue_date.as_ref(), "invoice alap/kelt")?;
        required(self.info.fulfillment_date.as_ref(), "invoice alap/telj")?;
        required(self.info.due_date.as_ref(), "invoice alap/fizh")?;
        required_text(self.info.payment_method.as_deref(), "invoice alap/fizmod")?;
        required_text(
            self.info.payment_method_unified.as_deref(),
            "invoice alap/fizmodunified",
        )?;
        required(self.info.cash.as_ref(), "invoice alap/keszpenz")?;
        required_text(self.info.language.as_deref(), "invoice alap/nyelv")?;
        required_text(self.info.currency.as_deref(), "invoice alap/devizanem")?;
        required(self.info.cash_accounting.as_ref(), "invoice alap/penzforg")?;
        required(self.info.kata.as_ref(), "invoice alap/kata")?;
        required(self.info.kata_ledger.as_ref(), "invoice alap/katafokonyv")?;
        required(self.info.test.as_ref(), "invoice alap/teszt")?;

        required(self.supplier.id.as_ref(), "invoice szallito/id")?;
        required_text(self.supplier.name.as_deref(), "invoice szallito/nev")?;
        validate_address(
            required(self.supplier.address.as_ref(), "invoice szallito/cim")?,
            "invoice szallito/cim",
        )?;
        required_text(
            self.supplier.tax_number.as_deref(),
            "invoice szallito/adoszam",
        )?;
        required_text(self.buyer.name.as_deref(), "invoice vevo/nev")?;
        validate_address(
            required(self.buyer.address.as_ref(), "invoice vevo/cim")?,
            "invoice vevo/cim",
        )?;
        required_text(self.buyer.tax_number.as_deref(), "invoice vevo/adoszam")?;
        required(self.buyer.location.as_ref(), "invoice vevo/lokacio")?;
        if matches!(kind, InvoiceKind::Outgoing) {
            required(
                self.buyer.private_person.as_ref(),
                "outgoing invoice vevo/privatePersonIndicator",
            )?;
        }

        if self.items.is_empty() {
            return validation("invoice tetelek must contain at least one tetel");
        }
        for item in &self.items {
            required_text(item.name.as_deref(), "invoice tetel/nev")?;
            required(item.quantity.as_ref(), "invoice tetel/mennyiseg")?;
            required_text(item.unit.as_deref(), "invoice tetel/mennyisegiegyseg")?;
            required(item.unit_price.as_ref(), "invoice tetel/nettoegysegar")?;
            required(item.vat_rate.as_ref(), "invoice tetel/afakulcs")?;
            non_negative(item.vat_rate, "invoice tetel/afakulcs")?;
            required(item.net_value.as_ref(), "invoice tetel/netto")?;
            required(item.vat_value.as_ref(), "invoice tetel/afa")?;
            required(item.gross_value.as_ref(), "invoice tetel/brutto")?;
            required(item.ordering.as_ref(), "invoice tetel/sztetordering")?;
        }
        validate_totals(&self.totals, "invoice")?;
        for payment in &self.payments {
            required(payment.date.as_ref(), "invoice kifizetes/datum")?;
            required_text(payment.method.as_deref(), "invoice kifizetes/jogcim")?;
            required(payment.amount.as_ref(), "invoice kifizetes/osszeg")?;
        }
        for item in &self.financial_items {
            required_text(item.name.as_deref(), "invoice qutet/nev")?;
            required(item.vat_rate.as_ref(), "invoice qutet/afakulcs")?;
            non_negative(item.vat_rate, "invoice qutet/afakulcs")?;
            required(item.net.as_ref(), "invoice qutet/netto")?;
            required(item.vat.as_ref(), "invoice qutet/afa")?;
            required(item.gross.as_ref(), "invoice qutet/brutto")?;
            required(item.deductible_vat.as_ref(), "invoice qutet/afalevon")?;
        }

        Ok(())
    }
}

fn validate_totals(totals: &Totals, document: &str) -> Result<(), ParseError> {
    if totals.per_vat_rate.is_empty() {
        return validation(format!(
            "{document} osszegek must contain at least one afakulcsossz"
        ));
    }
    for total in &totals.per_vat_rate {
        required(total.vat_rate.as_ref(), "afakulcsossz/afakulcs")?;
        non_negative(total.vat_rate, "afakulcsossz/afakulcs")?;
        required(total.net.as_ref(), "afakulcsossz/netto")?;
        required(total.vat.as_ref(), "afakulcsossz/afa")?;
        required(total.gross.as_ref(), "afakulcsossz/brutto")?;
    }
    let grand = required(totals.grand.as_ref(), "osszegek/totalossz")?;
    required(grand.net.as_ref(), "totalossz/netto")?;
    required(grand.vat.as_ref(), "totalossz/afa")?;
    required(grand.gross.as_ref(), "totalossz/brutto")?;

    Ok(())
}

fn required<'a, T>(value: Option<&'a T>, field: &str) -> Result<&'a T, ParseError> {
    value.ok_or_else(|| ParseError::Validation(format!("missing required {field}")))
}

fn required_text<'a>(value: Option<&'a str>, field: &str) -> Result<&'a str, ParseError> {
    value.ok_or_else(|| ParseError::Validation(format!("missing required {field}")))
}

fn validate_address(address: &Address, field: &str) -> Result<(), ParseError> {
    required_text(address.zip.as_deref(), &format!("{field}/irsz"))?;
    required_text(address.city.as_deref(), &format!("{field}/telepules"))?;
    required_text(address.address.as_deref(), &format!("{field}/cim"))?;

    Ok(())
}

fn non_negative(value: Option<Decimal>, field: &str) -> Result<(), ParseError> {
    if value.is_some_and(|value| value.is_sign_negative()) {
        return validation(format!("{field} must not be negative"));
    }
    Ok(())
}

fn validation<T>(message: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError::Validation(message.into()))
}

/// Direction of a bank transaction (`irany`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub enum TransactionDirection {
    /// `BE` — incoming.
    #[serde(rename(deserialize = "BE"))]
    Incoming,
    /// `KI` — outgoing.
    #[serde(rename(deserialize = "KI"))]
    Outgoing,
}

/// The other side of a bank transaction (`partner`).
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct TransactionPartner {
    /// Name (`nev`).
    #[serde(default, rename(deserialize = "nev"))]
    pub name: Option<String>,
    /// Bank account number (`bankszamla`).
    #[serde(default, rename(deserialize = "bankszamla"))]
    pub bank_account: Option<String>,
}

/// A pushed bank transaction (`<banktranz>`).
///
/// The fields typed as non-`Option` (`id`, `bank_account`, `value_date`,
/// `direction`, `amount`, `currency`) are treated as the protocol's
/// contractually-required core: a push missing or malforming one of them fails
/// to parse (→ HTTP 400 → szamlazz.hu retries for 72 hours). This is
/// deliberate — these define what a transaction *is*, and szamlazz.hu cannot
/// drop them without a breaking protocol change that would require a receiver
/// update anyway. Optional, evolving detail (`kind`, `partner`, `memo`)
/// degrades to absent instead.
#[doc(alias = "banki tranzakció")]
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct BankTransaction {
    /// Transaction id (`id`).
    pub id: i64,
    /// The account's own bank account number (`bankszamla`).
    #[serde(rename(deserialize = "bankszamla"))]
    pub bank_account: String,
    /// Value date (`erteknap`).
    #[doc(alias = "értéknap")]
    #[serde(rename(deserialize = "erteknap"), deserialize_with = "de::xs_date")]
    pub value_date: Date,
    /// Direction (`irany`).
    #[serde(rename(deserialize = "irany"))]
    pub direction: TransactionDirection,
    /// Transaction type (`tipus`).
    #[serde(default, rename(deserialize = "tipus"))]
    pub kind: Option<String>,
    /// Required technical (non-business) transaction flag (`technikai`).
    #[serde(
        rename(deserialize = "technikai"),
        deserialize_with = "de::flexible_bool"
    )]
    pub technical: bool,
    /// Amount (`osszeg`).
    #[serde(rename(deserialize = "osszeg"), deserialize_with = "de::decimal_text")]
    pub amount: Decimal,
    /// Currency (`devizanem`).
    #[serde(rename(deserialize = "devizanem"))]
    pub currency: String,
    /// The other party (`partner`).
    #[serde(default)]
    pub partner: Option<TransactionPartner>,
    /// Transfer memo (`kozlemeny`).
    #[doc(alias = "közlemény")]
    #[serde(default, rename(deserialize = "kozlemeny"))]
    pub memo: Option<String>,
    /// Exact UTF-8 XML request from which this transaction was parsed.
    #[serde(skip)]
    raw_xml: Option<Arc<str>>,
}

impl BankTransaction {
    /// Returns the exact pushed XML, when this value came from [`Document::parse`].
    #[must_use]
    pub fn raw_xml(&self) -> Option<&str> {
        self.raw_xml.as_deref()
    }
}

/// Identity and metadata of an archived receipt (`alap`).
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct ReceiptInfo {
    /// The record id (`id`).
    pub id: i64,
    /// Idempotency call id the receipt was created with (`hivasAzonosito`).
    #[serde(default, rename(deserialize = "hivasAzonosito"))]
    pub call_id: Option<String>,
    /// Receipt number (`nyugtaszam`).
    #[doc(alias = "nyugtaszám")]
    #[serde(default, rename(deserialize = "nyugtaszam"))]
    pub receipt_number: Option<String>,
    /// Receipt type (`tipus`): `NY` receipt, `SN` reversal. Unknown codes
    /// are preserved.
    #[serde(default, rename(deserialize = "tipus"))]
    pub kind: Option<String>,
    /// Whether the receipt has been reversed (`stornozott`).
    #[serde(
        default,
        rename(deserialize = "stornozott"),
        deserialize_with = "de::opt_flexible_bool"
    )]
    pub reversed: Option<bool>,
    /// The receipt this one reverses (`stornozottNyugtaszam`).
    #[serde(default, rename(deserialize = "stornozottNyugtaszam"))]
    pub reversed_receipt_number: Option<String>,
    /// Issue date (`kelt`).
    #[serde(
        default,
        rename(deserialize = "kelt"),
        deserialize_with = "de::opt_xs_date"
    )]
    pub issue_date: Option<Date>,
    /// Payment method (`fizmod`).
    #[serde(default, rename(deserialize = "fizmod"))]
    pub payment_method: Option<String>,
    /// Currency (`penznem`).
    #[serde(default, rename(deserialize = "penznem"))]
    pub currency: Option<String>,
    /// Quoting bank for the exchange rate (`devizabank`).
    #[serde(default, rename(deserialize = "devizabank"))]
    pub exchange_rate_bank: Option<String>,
    /// Exchange rate (`devizaarf`).
    #[serde(
        default,
        rename(deserialize = "devizaarf"),
        deserialize_with = "de::empty_as_none"
    )]
    pub exchange_rate: Option<Decimal>,
    /// Comment (`megjegyzes`).
    #[serde(default, rename(deserialize = "megjegyzes"))]
    pub comment: Option<String>,
    /// General-ledger identifier of the customer (`fokonyvVevo`).
    #[serde(default, rename(deserialize = "fokonyvVevo"))]
    pub customer_ledger: Option<String>,
    /// Issued by a test account (`teszt`).
    #[serde(
        default,
        rename(deserialize = "teszt"),
        deserialize_with = "de::opt_flexible_bool"
    )]
    pub test: Option<bool>,
    /// Issuer tax number (`adoszam`).
    #[doc(alias = "adószám")]
    #[serde(default, rename(deserialize = "adoszam"))]
    pub tax_number: Option<String>,
    /// Order number (`rendelesSzam`).
    #[serde(default, rename(deserialize = "rendelesSzam"))]
    pub order_number: Option<String>,
}

/// One line item of an archived receipt (`tetel`).
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct ReceiptItem {
    /// Item name (`megnevezes`).
    #[serde(default, rename(deserialize = "megnevezes"))]
    pub name: Option<String>,
    /// Item identifier (`azonosito`).
    #[serde(default, rename(deserialize = "azonosito"))]
    pub id: Option<String>,
    /// Net unit price (`nettoEgysegar`).
    #[serde(
        default,
        rename(deserialize = "nettoEgysegar"),
        deserialize_with = "de::empty_as_none"
    )]
    pub unit_price: Option<Decimal>,
    /// Quantity (`mennyiseg`).
    #[serde(
        default,
        rename(deserialize = "mennyiseg"),
        deserialize_with = "de::empty_as_none"
    )]
    pub quantity: Option<Decimal>,
    /// Unit of measure (`mennyisegiEgyseg`).
    #[serde(default, rename(deserialize = "mennyisegiEgyseg"))]
    pub unit: Option<String>,
    /// Net value (`netto`).
    #[serde(
        default,
        rename(deserialize = "netto"),
        deserialize_with = "de::empty_as_none"
    )]
    pub net_value: Option<Decimal>,
    /// Optional NAV special VAT category (`afatipus`).
    #[serde(default, rename(deserialize = "afatipus"))]
    pub vat_type: Option<String>,
    /// Required numeric VAT percentage (`afakulcs`).
    #[serde(
        default,
        rename(deserialize = "afakulcs"),
        deserialize_with = "de::empty_as_none"
    )]
    pub vat_rate: Option<Decimal>,
    /// VAT value (`afa`).
    #[serde(
        default,
        rename(deserialize = "afa"),
        deserialize_with = "de::empty_as_none"
    )]
    pub vat_value: Option<Decimal>,
    /// Gross value (`brutto`).
    #[serde(
        default,
        rename(deserialize = "brutto"),
        deserialize_with = "de::empty_as_none"
    )]
    pub gross_value: Option<Decimal>,
    /// Item accounting data (`fokonyv`).
    #[serde(default, rename(deserialize = "fokonyv"))]
    pub ledger: Option<ReceiptItemLedger>,
    /// Comment (`megjegyzes`).
    #[serde(default, rename(deserialize = "megjegyzes"))]
    pub comment: Option<String>,
}

impl ReceiptItem {
    /// Returns effective VAT semantics, preferring a special category.
    #[must_use]
    pub fn effective_vat(&self) -> Option<VatRate<'_>> {
        effective_vat(self.vat_type.as_deref(), self.vat_rate)
    }
}

/// Accounting data attached to a receipt line (`fokonyv`).
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct ReceiptItemLedger {
    /// Revenue ledger number (`arbevetel`).
    #[serde(default, rename(deserialize = "arbevetel"))]
    pub revenue: Option<String>,
    /// VAT ledger number (`afa`).
    #[serde(default, rename(deserialize = "afa"))]
    pub vat: Option<String>,
}

/// A payment on an archived receipt (`kifizetes`).
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct ReceiptPayment {
    /// Legal tender (`fizetoeszkoz`).
    #[doc(alias = "fizetőeszköz")]
    #[serde(default, rename(deserialize = "fizetoeszkoz"))]
    pub method: Option<String>,
    /// Amount (`osszeg`).
    #[serde(
        default,
        rename(deserialize = "osszeg"),
        deserialize_with = "de::empty_as_none"
    )]
    pub amount: Option<Decimal>,
    /// Description (`leiras`).
    #[serde(default, rename(deserialize = "leiras"))]
    pub description: Option<String>,
}

/// One receipt inside a pushed archive (`nyugta`).
#[doc(alias = "nyugta")]
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct ReceiptDocument {
    /// Identity and metadata (`alap`).
    #[serde(rename(deserialize = "alap"))]
    pub info: ReceiptInfo,
    /// Line items (`tetelek`).
    #[serde(
        default,
        rename(deserialize = "tetelek"),
        deserialize_with = "de::receipt_items"
    )]
    pub items: Vec<ReceiptItem>,
    /// Payments (`kifizetesek`).
    #[serde(
        default,
        rename(deserialize = "kifizetesek"),
        deserialize_with = "de::receipt_payments"
    )]
    pub payments: Vec<ReceiptPayment>,
    /// Totals (`osszegek`).
    #[serde(default, rename(deserialize = "osszegek"))]
    pub totals: Totals,
}

/// A pushed receipt batch (`<xmlnyugtaarchiv>`) — receipts are delivered in
/// daily batches, unlike the other document types.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct ReceiptBatch {
    /// The receipts in this delivery.
    #[serde(default, rename(deserialize = "nyugta"))]
    pub receipts: Vec<ReceiptDocument>,
    /// Exact UTF-8 XML request from which this batch was parsed.
    #[serde(skip)]
    raw_xml: Option<Arc<str>>,
}

impl ReceiptBatch {
    /// Returns the exact pushed XML, when this value came from [`Document::parse`].
    #[must_use]
    pub fn raw_xml(&self) -> Option<&str> {
        self.raw_xml.as_deref()
    }

    fn validate(&self) -> Result<(), ParseError> {
        if self.receipts.is_empty() {
            return validation("receipt archive must contain at least one nyugta");
        }
        for receipt in &self.receipts {
            required_text(
                receipt.info.receipt_number.as_deref(),
                "receipt alap/nyugtaszam",
            )?;
            required_text(receipt.info.kind.as_deref(), "receipt alap/tipus")?;
            required(receipt.info.reversed.as_ref(), "receipt alap/stornozott")?;
            required(receipt.info.issue_date.as_ref(), "receipt alap/kelt")?;
            required_text(
                receipt.info.payment_method.as_deref(),
                "receipt alap/fizmod",
            )?;
            required_text(receipt.info.currency.as_deref(), "receipt alap/penznem")?;
            required(receipt.info.test.as_ref(), "receipt alap/teszt")?;
            if receipt.items.is_empty() {
                return validation("receipt tetelek must contain at least one tetel");
            }
            for item in &receipt.items {
                required_text(item.name.as_deref(), "receipt tetel/megnevezes")?;
                required(item.unit_price.as_ref(), "receipt tetel/nettoEgysegar")?;
                required(item.quantity.as_ref(), "receipt tetel/mennyiseg")?;
                required_text(item.unit.as_deref(), "receipt tetel/mennyisegiEgyseg")?;
                required(item.net_value.as_ref(), "receipt tetel/netto")?;
                required(item.vat_rate.as_ref(), "receipt tetel/afakulcs")?;
                non_negative(item.vat_rate, "receipt tetel/afakulcs")?;
                required(item.vat_value.as_ref(), "receipt tetel/afa")?;
                required(item.gross_value.as_ref(), "receipt tetel/brutto")?;
            }
            for payment in &receipt.payments {
                required_text(payment.method.as_deref(), "receipt kifizetes/fizetoeszkoz")?;
                required(payment.amount.as_ref(), "receipt kifizetes/osszeg")?;
            }
            validate_totals(&receipt.totals, "receipt")?;
        }

        Ok(())
    }
}

/// Lenient deserialization helpers: szamlazz.hu sends absent values as empty
/// elements and bools as either `true`/`false` or `0`/`1`.
pub(crate) mod de {
    use serde::{Deserialize, Deserializer};

    use super::{Date, InvoiceAppearance, Pdf};

    pub fn empty_string_as_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<String>::deserialize(deserializer)?;
        Ok(value.filter(|text| !text.trim().is_empty()))
    }

    pub fn empty_as_none<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
    where
        D: Deserializer<'de>,
        T: std::str::FromStr,
        T::Err: std::fmt::Display,
    {
        let value = Option::<String>::deserialize(deserializer)?;

        match value.as_deref().map(str::trim) {
            None | Some("") => Ok(None),
            Some(text) => text.parse().map(Some).map_err(serde::de::Error::custom),
        }
    }

    /// Strips XML Schema's optional timezone suffix (`Z` or `±hh:mm`) from an
    /// `xs:date` lexical value. The offset carries nothing a civil [`Date`]
    /// can represent, but a schema-valid value must not fail the parse.
    fn strip_xs_date_timezone(text: &str) -> &str {
        if let Some(date) = text.strip_suffix('Z') {
            return date;
        }
        if text.len() > 6 {
            let (date, suffix) = text.split_at(text.len() - 6);
            let bytes = suffix.as_bytes();
            if matches!(bytes[0], b'+' | b'-')
                && bytes[1].is_ascii_digit()
                && bytes[2].is_ascii_digit()
                && bytes[3] == b':'
                && bytes[4].is_ascii_digit()
                && bytes[5].is_ascii_digit()
            {
                return date;
            }
        }
        text
    }

    /// Deserializes a required `xs:date`, discarding any timezone suffix.
    pub fn xs_date<'de, D>(deserializer: D) -> Result<Date, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        strip_xs_date_timezone(value.trim())
            .parse()
            .map_err(serde::de::Error::custom)
    }

    /// Deserializes an optional `xs:date`, reading empty elements as absent
    /// and discarding any timezone suffix.
    pub fn opt_xs_date<'de, D>(deserializer: D) -> Result<Option<Date>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<String>::deserialize(deserializer)?;

        match value.as_deref().map(str::trim) {
            None | Some("") => Ok(None),
            Some(text) => strip_xs_date_timezone(text)
                .parse()
                .map(Some)
                .map_err(serde::de::Error::custom),
        }
    }

    /// Deserializes a required decimal from element text; `rust_decimal`'s own
    /// `Deserialize` uses `deserialize_any`, which the XML deserializer
    /// answers with a map.
    pub fn decimal_text<'de, D>(deserializer: D) -> Result<rust_decimal::Decimal, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.trim().parse().map_err(serde::de::Error::custom)
    }

    pub fn flexible_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;

        match value.trim() {
            "true" | "1" => Ok(true),
            "false" | "0" => Ok(false),
            other => Err(serde::de::Error::custom(format!("invalid bool: {other}"))),
        }
    }

    pub fn opt_flexible_bool<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<String>::deserialize(deserializer)?;

        match value.as_deref().map(str::trim) {
            None | Some("") => Ok(None),
            Some("true" | "1") => Ok(Some(true)),
            Some("false" | "0") => Ok(Some(false)),
            Some(other) => Err(serde::de::Error::custom(format!("invalid bool: {other}"))),
        }
    }

    pub fn opt_invoice_appearance<'de, D>(
        deserializer: D,
    ) -> Result<Option<InvoiceAppearance>, D::Error>
    where
        D: Deserializer<'de>,
    {
        empty_as_none::<D, i64>(deserializer).map(|value| value.map(InvoiceAppearance::from))
    }

    pub fn base64_pdf<'de, D>(deserializer: D) -> Result<Option<Pdf>, D::Error>
    where
        D: Deserializer<'de>,
    {
        use base64::Engine as _;
        let value = Option::<String>::deserialize(deserializer)?;

        match value {
            None => Ok(None),
            Some(encoded) => {
                let compact: String = encoded.split_whitespace().collect();

                if compact.is_empty() {
                    return Ok(None);
                }
                base64::engine::general_purpose::STANDARD
                    .decode(compact)
                    .map(|bytes| Some(Pdf(bytes)))
                    .map_err(serde::de::Error::custom)
            }
        }
    }

    /// Generates a deserializer for the `<wrapper><child/>…</wrapper>` list
    /// pattern.
    macro_rules! wrapped_list {
        ($fn_name:ident, $child:literal, $ty:ty) => {
            pub fn $fn_name<'de, D>(deserializer: D) -> Result<Vec<$ty>, D::Error>
            where
                D: Deserializer<'de>,
            {
                #[derive(serde::Deserialize)]
                struct Wrapper {
                    #[serde(default, rename = $child)]
                    children: Vec<$ty>,
                }
                Ok(Option::<Wrapper>::deserialize(deserializer)?
                    .map(|wrapper| wrapper.children)
                    .unwrap_or_default())
            }
        };
    }

    wrapped_list!(items, "tetel", super::InvoiceItem);
    wrapped_list!(payments, "kifizetes", super::RecordedPayment);
    wrapped_list!(receipt_items, "tetel", super::ReceiptItem);
    wrapped_list!(receipt_payments, "kifizetes", super::ReceiptPayment);
    wrapped_list!(financial_items, "qutet", super::FinancialItem);
    wrapped_list!(tags, "cimke", String);
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTGOING_INVOICE: &str = include_str!("../tests/synthetic/szamla.xml");

    #[test]
    fn empty_optional_numeric_elements_read_as_absent() {
        let body = OUTGOING_INVOICE
            .replace("<forras>34</forras>", "<forras></forras>")
            .replace("<id>1234567</id>", "<id></id>");
        let Document::OutgoingInvoice(invoice) =
            Document::parse(body.as_bytes()).expect("empty optional elements must not fail")
        else {
            panic!("expected outgoing invoice");
        };
        assert_eq!(invoice.info.source, None);
        assert_eq!(invoice.buyer.id, None);
        // The fixture's <rendelesszam> is empty on the wire.
        assert_eq!(invoice.info.order_number, None);
    }

    #[test]
    fn unknown_receipt_type_is_preserved() {
        let body = b"<xmlnyugtaarchiv xmlns=\"http://www.szamlazz.hu/xmlnyugtaarchiv\"><nyugta>\
            <alap><id>1</id><nyugtaszam>NYGTA-1</nyugtaszam><tipus>XX</tipus>\
            <stornozott>false</stornozott><kelt>2026-07-03</kelt><fizmod>k\xc3\xa9szp\xc3\xa9nz</fizmod>\
            <penznem>HUF</penznem><teszt>false</teszt></alap>\
            <tetelek><tetel><megnevezes>Service</megnevezes><nettoEgysegar>100</nettoEgysegar>\
            <mennyiseg>1</mennyiseg><mennyisegiEgyseg>db</mennyisegiEgyseg><netto>100</netto>\
            <afakulcs>27</afakulcs><afa>27</afa><brutto>127</brutto></tetel></tetelek>\
            <osszegek><afakulcsossz><afakulcs>27</afakulcs><netto>100</netto><afa>27</afa>\
            <brutto>127</brutto></afakulcsossz><totalossz><netto>100</netto><afa>27</afa>\
            <brutto>127</brutto></totalossz></osszegek></nyugta></xmlnyugtaarchiv>";
        let Document::Receipts(batch) =
            Document::parse(body).expect("unknown receipt tipus must not fail")
        else {
            panic!("expected receipt batch");
        };
        assert_eq!(batch.receipts[0].info.kind.as_deref(), Some("XX"));
    }

    #[test]
    fn bank_transaction_rejects_missing_technical_flag() {
        let body = b"<banktranz xmlns=\"http://www.szamlazz.hu/banktranz\">\
            <id>1</id><bankszamla>111</bankszamla><erteknap>2026-07-04</erteknap>\
            <irany>BE</irany><osszeg>1000</osszeg><devizanem>HUF</devizanem>\
            </banktranz>";
        assert!(Document::parse(body).is_err());
    }

    #[test]
    fn bank_transaction_requires_valid_technical_flag() {
        for (value, expected) in [("true", true), ("1", true), ("false", false), ("0", false)] {
            let body = format!(
                "<banktranz xmlns=\"http://www.szamlazz.hu/banktranz\">\
                 <id>1</id><bankszamla>111</bankszamla><erteknap>2026-07-04</erteknap>\
                 <irany>BE</irany><technikai>{value}</technikai>\
                 <osszeg>1000</osszeg><devizanem>HUF</devizanem></banktranz>"
            );
            let Document::BankTransaction(transaction) =
                Document::parse(body.as_bytes()).expect("valid boolean")
            else {
                panic!("expected bank transaction");
            };
            assert_eq!(transaction.technical, expected);
        }

        let body = b"<banktranz xmlns=\"http://www.szamlazz.hu/banktranz\">\
            <id>1</id><bankszamla>111</bankszamla><erteknap>2026-07-04</erteknap>\
            <irany>BE</irany><technikai/><osszeg>1000</osszeg><devizanem>HUF</devizanem>\
            </banktranz>";
        assert!(Document::parse(body).is_err());
    }

    #[test]
    fn xs_date_timezone_suffix_is_discarded() {
        // xs:date permits an optional timezone suffix; the offset is
        // discarded, the civil date kept.
        for value in [
            "2026-07-04",
            "2026-07-04Z",
            "2026-07-04+02:00",
            "2026-07-04-05:00",
        ] {
            let body = format!(
                "<banktranz xmlns=\"http://www.szamlazz.hu/banktranz\">\
                 <id>1</id><bankszamla>111</bankszamla><erteknap>{value}</erteknap>\
                 <irany>BE</irany><technikai>false</technikai>\
                 <osszeg>1000</osszeg><devizanem>HUF</devizanem></banktranz>"
            );
            let Document::BankTransaction(transaction) =
                Document::parse(body.as_bytes()).expect("schema-valid xs:date")
            else {
                panic!("expected bank transaction");
            };
            assert_eq!(transaction.value_date, jiff::civil::date(2026, 7, 4));
        }

        let body =
            OUTGOING_INVOICE.replace("<kelt>2015-12-01</kelt>", "<kelt>2015-12-01+01:00</kelt>");
        let Document::OutgoingInvoice(invoice) =
            Document::parse(body.as_bytes()).expect("schema-valid xs:date on optional field")
        else {
            panic!("expected outgoing invoice");
        };
        assert_eq!(
            invoice.info.issue_date,
            Some(jiff::civil::date(2015, 12, 1))
        );

        // Garbage after the date is still rejected.
        let body =
            OUTGOING_INVOICE.replace("<kelt>2015-12-01</kelt>", "<kelt>2015-12-01junk</kelt>");
        assert!(Document::parse(body.as_bytes()).is_err());
    }
}
