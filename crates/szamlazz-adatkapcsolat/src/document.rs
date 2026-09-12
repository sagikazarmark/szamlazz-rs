//! The pushed document types, deserialized leniently: unknown elements are
//! ignored, empty elements read as absent, and an element the XSD requires
//! is as optional as any other; [`Document::parse`] refuses **shape** (a body
//! that is not the pushed document) and never **content**, because a push is
//! at-most-N-times delivery and a refusal szamlazz.hu retries identically for
//! 72 hours loses the record. The XSD's requirements are available as a signal
//! through [`Document::validate`] / [`Document::parse_strict`]. Date fields are
//! the business-level civil [`Date`] type; XML Schema's optional `xs:date`
//! timezone suffix (`Z`/`±hh:mm`) is accepted on the wire and discarded, and a
//! date text that is not a date reads as `None`, so no date element, whatever
//! text it holds, can fail the delivery of the record it sits in.

use std::fmt;
use std::sync::Arc;

use jiff::civil::Date;
use quick_xml::Reader;
use quick_xml::events::Event;
use rust_decimal::Decimal;

use crate::ack::InvoiceDirection;
use crate::error::{ParseError, ValidationError};

mod xml;

/// One pushed document, identified by the XML root element.
///
/// Exhaustive on purpose: the four variants are the four streams an
/// Adatkapcsolat connection can push, and [`Handler`](crate::Handler) makes a
/// fifth a breaking change by design (every method is required, so a new
/// stream cannot be acknowledged and discarded by an implementation that does
/// not know it). A wildcard arm would buy nothing and hide that. Breaking
/// change in 0.4: the enum was `#[non_exhaustive]`.
#[derive(Debug)]
pub enum Document {
    /// An outgoing invoice (`<szamla>`), pushed within ~15 minutes of issue.
    #[doc(alias = "kimenő számla")]
    OutgoingInvoice(InvoiceDocument),
    /// An incoming (received) invoice (`<szamlabe>`).
    #[doc(alias = "bejövő számla")]
    IncomingInvoice(InvoiceDocument),
    /// A bank transaction (`<banktranz>`), pushed in periodic batches, one
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
    /// Refuses **shape**, never **content**. A push is at-most-N-times
    /// delivery: szamlazz.hu retries a non-200 answer, identically, for up
    /// to 72 hours and then drops the record; for a bank transaction or a
    /// receipt that is the last time it offers it. A body this parse cannot
    /// read is therefore lost, so it refuses only a body that is not the
    /// pushed record: one that is not UTF-8 or not well-formed XML, an
    /// unknown root or an element outside the document's namespace, a
    /// record without its **identity** (an invoice's `alap/id` and
    /// `szamlaszam`, a bank transaction's `id`, a receipt's `alap/id`;
    /// missing or not an integer), and a number or boolean that is not one
    /// (an `<osszeg>` that is not a number, a `<teszt>` that is not a
    /// boolean). Everything else is content and reads as the wire delivers
    /// it: a missing element is `None`, an unknown `<irany>` is
    /// [`TransactionDirection::Other`], a `<pdf>` that does not decode is
    /// [`None`](InvoiceDocument::pdf) with the encoded text still in
    /// [`raw_xml`](InvoiceDocument::raw_xml), a date that is not a date (a
    /// `<kelt>` of `2015-12-01junk`) is `None` with its text likewise in the
    /// raw XML, an empty receipt batch has no receipts.
    ///
    /// The identity is shape because it is what the receiver keys the
    /// record by, not because every Ack echoes it: an invoice Ack echoes
    /// `alap/id` and nothing echoes `szamlaszam`, a bank transaction's or a
    /// receipt batch's Ack carries no id at all, yet redelivery is the
    /// protocol's normal case (a lost Ack, a fan-out member that failed) and
    /// a receiver tolerates it by the id (the archiver names its objects by
    /// it). szamlazz.hu assigns the id itself and its schemas type it an
    /// integer, so a push without one is not a record it holds, the same
    /// class of body as a truncated one. The cost is stated: a receipt
    /// batch with one id-less `<nyugta>` is refused whole, and szamlazz.hu
    /// drops it after 72 hours.
    ///
    /// The line between an unknown enumeration token (content) and a
    /// malformed lexical value (shape) is the one the crate has always drawn
    /// with [`InvoiceAppearance::Unknown`]: an enumeration is an open set
    /// szamlazz.hu extends (a new direction or document type is a protocol
    /// extension the receiver must survive), while a fourth spelling of
    /// `true` is not an extension but a value the type cannot hold, and
    /// reading it as `None` would hide it behind the same answer as an
    /// omission. A date that is not a date is the one lexical failure read
    /// as content: the crate already reads `xs:date` on its own terms (the
    /// timezone suffix a civil [`Date`] cannot hold is discarded), a text it
    /// cannot make a date of is `None` with the text kept in the raw XML,
    /// and where the XSD requires the date [`validate`](Self::validate)
    /// reports the requirement it fails to meet, so the record is kept and
    /// the verdict is still to be had.
    ///
    /// The XSD's own requirements (its `minOccurs="1"` elements, its
    /// enumerations, its non-negative VAT rates) are a **signal**, not a
    /// gate: [`Document::validate`] reports the first one a parsed document
    /// misses, and [`Document::parse_strict`] makes it a gate for callers
    /// that want one.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid UTF-8 or XML, an unknown root or wrong
    /// namespace, or a shape the typed parse cannot read.
    pub fn parse(body: &[u8]) -> Result<Self, ParseError> {
        let root = Self::identify(body)?;
        Self::parse_identified(body, root)
    }

    /// [`parse`](Self::parse), then [`validate`](Self::validate): the parse
    /// as a gate on XSD conformance, for a receiver that would rather
    /// refuse a non-conforming push (and have szamlazz.hu retry, then drop
    /// it) than read it leniently.
    ///
    /// # Errors
    ///
    /// Everything [`parse`](Self::parse) refuses, plus
    /// [`ParseError::Validation`] for the first XSD requirement the document
    /// misses.
    pub fn parse_strict(body: &[u8]) -> Result<Self, ParseError> {
        let document = Self::parse(body)?;
        document.validate()?;
        Ok(document)
    }

    /// Checks the document against its XSD's requirements: the
    /// `minOccurs="1"` elements, the `irany` enumeration, non-negative VAT
    /// rates, at least one line item / receipt / VAT total. The parse does
    /// not need any of them; a receiver may.
    ///
    /// # Errors
    ///
    /// The first requirement the document misses.
    pub fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::OutgoingInvoice(invoice) => invoice.validate(InvoiceDirection::Outgoing),
            Self::IncomingInvoice(invoice) => invoice.validate(InvoiceDirection::Incoming),
            Self::BankTransaction(transaction) => transaction.validate(),
            Self::Receipts(batch) => batch.validate(),
        }
    }

    /// Which kind of document this is.
    #[must_use]
    pub fn kind(&self) -> RootKind {
        match self {
            Self::OutgoingInvoice(_) => RootKind::OutgoingInvoice,
            Self::IncomingInvoice(_) => RootKind::IncomingInvoice,
            Self::BankTransaction(_) => RootKind::BankTransaction,
            Self::Receipts(_) => RootKind::Receipts,
        }
    }

    /// Validates UTF-8 and identifies the pushed kind by the root element,
    /// verifying the root's own namespace. The XML is read only up to the
    /// first start tag (the UTF-8 check covers the whole body), so a
    /// receiver can follow the protocol's order without a framework:
    /// identify the root, authenticate the key, and only then
    /// [`parse`](Self::parse) the body, or answer a `KEY_ERR` Ack in the
    /// shape of the pushed kind ([`ControlCode::to_xml`](crate::ControlCode::to_xml))
    /// for a push it has not authenticated, having parsed nothing of it.
    ///
    /// # Errors
    ///
    /// [`ParseError::Utf8`] for a body that is not UTF-8,
    /// [`ParseError::Empty`] for one without a root element,
    /// [`ParseError::UnknownRoot`] for a root this crate does not know,
    /// [`ParseError::WrongNamespace`] for a known root outside its official
    /// namespace, and [`ParseError::Xml`] for XML that is not well-formed up
    /// to the root.
    pub fn identify(body: &[u8]) -> Result<RootKind, ParseError> {
        let text = std::str::from_utf8(body)?;

        root_kind(text)
    }

    /// The full parse of an identified body: complete XML shape and every
    /// element's normalized namespace, then lenient typed deserialization
    /// (including embedded payloads such as PDFs).
    pub(crate) fn parse_identified(body: &[u8], root: RootKind) -> Result<Self, ParseError> {
        let text = std::str::from_utf8(body)?;
        let normalized = xml::validate(text, root)?;
        let raw_xml: Arc<str> = Arc::from(text);
        let text = normalized.as_ref();

        match root {
            RootKind::OutgoingInvoice => {
                let mut invoice: InvoiceDocument = quick_xml::de::from_str(text)?;
                invoice.raw_xml = Some(raw_xml);
                Ok(Self::OutgoingInvoice(invoice))
            }
            RootKind::IncomingInvoice => {
                let mut invoice: InvoiceDocument = quick_xml::de::from_str(text)?;
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
                batch.raw_xml = Some(raw_xml);
                Ok(Self::Receipts(batch))
            }
        }
    }
}

/// Which kind of document a push carries, named by its XML root element:
/// the one enumeration of the four Adatkapcsolat streams, shared by the
/// parse ([`Document::identify`], [`Document::kind`]), the Acks
/// ([`ControlCode::to_xml`](crate::ControlCode::to_xml)) and the archiver's
/// layout.
///
/// Exhaustive for the same reason [`Document`] is: a fifth stream is a new
/// [`Handler`](crate::Handler) method, a breaking change by design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RootKind {
    /// `<szamla>`: an outgoing invoice.
    OutgoingInvoice,
    /// `<szamlabe>`: an incoming invoice.
    IncomingInvoice,
    /// `<banktranz>`: a bank transaction.
    BankTransaction,
    /// `<xmlnyugtaarchiv>`: a receipt batch.
    Receipts,
}

impl RootKind {
    /// The local name of the pushed root element.
    #[must_use]
    pub fn root_element(self) -> &'static str {
        match self {
            Self::OutgoingInvoice => "szamla",
            Self::IncomingInvoice => "szamlabe",
            Self::BankTransaction => "banktranz",
            Self::Receipts => "xmlnyugtaarchiv",
        }
    }

    /// The official namespace of the pushed document, which every element
    /// of it is in.
    #[must_use]
    pub fn namespace(self) -> &'static str {
        match self {
            Self::OutgoingInvoice => "http://www.szamlazz.hu/szamla",
            Self::IncomingInvoice => "http://www.szamlazz.hu/szamlabe",
            Self::BankTransaction => "http://www.szamlazz.hu/banktranz",
            Self::Receipts => "http://www.szamlazz.hu/xmlnyugtaarchiv",
        }
    }

    /// The local name of the Ack's root element for this kind.
    #[must_use]
    pub fn ack_root_element(self) -> &'static str {
        match self {
            Self::OutgoingInvoice => "szamlavalasz",
            Self::IncomingInvoice => "szamlabevalasz",
            Self::BankTransaction => "banktranzvalasz",
            Self::Receipts => "nyugtavalasz",
        }
    }

    /// The invoice direction, for the two invoice kinds; `None` for a bank
    /// transaction or a receipt batch.
    #[must_use]
    pub fn direction(self) -> Option<InvoiceDirection> {
        match self {
            Self::OutgoingInvoice => Some(InvoiceDirection::Outgoing),
            Self::IncomingInvoice => Some(InvoiceDirection::Incoming),
            Self::BankTransaction | Self::Receipts => None,
        }
    }
}

impl From<InvoiceDirection> for RootKind {
    fn from(direction: InvoiceDirection) -> Self {
        match direction {
            InvoiceDirection::Outgoing => Self::OutgoingInvoice,
            InvoiceDirection::Incoming => Self::IncomingInvoice,
        }
    }
}

impl fmt::Display for RootKind {
    /// The root element's local name.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.root_element())
    }
}

/// Identifies a known root and verifies its exact official namespace.
fn root_kind(text: &str) -> Result<RootKind, ParseError> {
    let mut reader = Reader::from_str(text);

    loop {
        match reader.read_event().map_err(quick_xml::DeError::from)? {
            Event::Start(start) | Event::Empty(start) => {
                let local = start.local_name();
                let local = local.as_ref();
                let kind = match local {
                    "szamla" => RootKind::OutgoingInvoice,
                    "szamlabe" => RootKind::IncomingInvoice,
                    "banktranz" => RootKind::BankTransaction,
                    "xmlnyugtaarchiv" => RootKind::Receipts,
                    other => return Err(ParseError::UnknownRoot(other.to_owned())),
                };
                let qualified_name = start.name();
                let qualified = qualified_name.as_ref();
                let prefix = qualified
                    .strip_suffix(local)
                    .and_then(|prefix| prefix.strip_suffix(':'));
                let namespace_attribute =
                    prefix.map_or_else(|| "xmlns".to_owned(), |prefix| format!("xmlns:{prefix}"));
                let mut namespace = None;

                for attribute in start.attributes().with_checks(false) {
                    let attribute = attribute.map_err(quick_xml::DeError::from)?;
                    if attribute.key.as_ref() == namespace_attribute {
                        namespace = Some(
                            attribute
                                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .map_err(quick_xml::DeError::from)?
                                .into_owned(),
                        );
                        break;
                    }
                }

                if namespace.as_deref() != Some(kind.namespace()) {
                    return Err(ParseError::WrongNamespace {
                        root: kind.root_element().to_owned(),
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
/// The same enum, width (`i64`) and JSON shape (the integer code) as the
/// `szamlazz-agent` crate's `InvoiceAppearance`; the two are separate types
/// by decision (ADR 0010).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

    /// Whether this is an e-invoice: the [`Electronic`](Self::Electronic)
    /// variant, whatever code it carries (the wire puts only `2` or `3`
    /// there; a hand-built `Electronic(4)` is an e-invoice too, not a trap).
    #[must_use]
    pub fn is_e_invoice(self) -> bool {
        matches!(self, Self::Electronic(_))
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
    /// The numeric percentage from `<afakulcs>`.
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

/// Serializes as a base64 string, the wire representation.
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
    pub location: Option<i64>,
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
    /// The document id (`id`): this is the value an [`InvoiceAck`] must
    /// echo. An `i64` like every integer of a pushed document (the crate's
    /// [integer-width policy](crate#integer-widths)); it was an `i32` in 0.3.
    ///
    /// [`InvoiceAck`]: crate::InvoiceAck
    pub id: i64,
    /// Invoice number (`szamlaszam`). With [`id`](Self::id) the identity of
    /// the pushed document: the one element besides the id
    /// [`Document::parse`] requires, so a push without it is a shape error.
    ///
    /// Breaking change in 0.4: this was an `Option<String>` that the parse
    /// required to be `Some` anyway.
    #[doc(alias = "számlaszám")]
    #[serde(rename(deserialize = "szamlaszam"))]
    pub invoice_number: String,
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
    /// Document type code (`tipus`): `SZ` invoice, `D` proforma, `ES`
    /// prepayment invoice, `VS` final invoice, `HS` corrective, `SS` storno,
    /// `SL` delivery note. Kept as the wire token (the `szamlazz-agent`
    /// crate's `DocumentType` is the typed reading; the two crates share the
    /// vocabulary, not the type: ADR 0010). Named `kind` in 0.3.
    #[doc(alias = "tipus")]
    #[serde(default, rename(deserialize = "tipus"))]
    pub document_type: Option<String>,
    /// Document appearance (`eszamla`): `0` not an invoice, `1` paper, and
    /// `2`/`3` e-invoice. Unknown integer values are preserved. Named
    /// `e_invoice` in 0.3; it is a code, not a flag.
    #[doc(alias = "e-számla")]
    #[doc(alias = "eszamla")]
    #[serde(
        default,
        rename(deserialize = "eszamla"),
        deserialize_with = "de::opt_invoice_appearance"
    )]
    pub appearance: Option<InvoiceAppearance>,
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
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
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
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
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
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
    pub unit_price: Option<Decimal>,
    /// Optional NAV special VAT category (`afatipus`).
    #[serde(default, rename(deserialize = "afatipus"))]
    pub vat_type: Option<String>,
    /// Numeric VAT percentage (`afakulcs`).
    #[doc(alias = "áfakulcs")]
    #[serde(
        default,
        rename(deserialize = "afakulcs"),
        deserialize_with = "de::empty_as_none"
    )]
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
    pub vat_rate: Option<Decimal>,
    /// Net value (`netto`).
    #[serde(
        default,
        rename(deserialize = "netto"),
        deserialize_with = "de::empty_as_none"
    )]
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
    pub net_value: Option<Decimal>,
    /// Margin-scheme VAT base (`arresafaalap`).
    #[serde(
        default,
        rename(deserialize = "arresafaalap"),
        deserialize_with = "de::empty_as_none"
    )]
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
    pub margin_vat_base: Option<Decimal>,
    /// VAT value (`afa`).
    #[serde(
        default,
        rename(deserialize = "afa"),
        deserialize_with = "de::empty_as_none"
    )]
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
    pub vat_value: Option<Decimal>,
    /// Gross value (`brutto`).
    #[serde(
        default,
        rename(deserialize = "brutto"),
        deserialize_with = "de::empty_as_none"
    )]
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
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
    /// Numeric VAT percentage (`afakulcs`); the grand total's schema has none.
    #[serde(
        default,
        rename(deserialize = "afakulcs"),
        deserialize_with = "de::empty_as_none"
    )]
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
    pub vat_rate: Option<Decimal>,
    /// Net total (`netto`).
    #[serde(
        default,
        rename(deserialize = "netto"),
        deserialize_with = "de::empty_as_none"
    )]
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
    pub net: Option<Decimal>,
    /// VAT total (`afa`).
    #[serde(
        default,
        rename(deserialize = "afa"),
        deserialize_with = "de::empty_as_none"
    )]
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
    pub vat: Option<Decimal>,
    /// Gross total (`brutto`).
    #[serde(
        default,
        rename(deserialize = "brutto"),
        deserialize_with = "de::empty_as_none"
    )]
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
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

/// A credit entry recorded on the invoice (`kifizetes`).
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct RecordedCreditEntry {
    /// Credit entry date (`datum`).
    #[serde(
        default,
        rename(deserialize = "datum"),
        deserialize_with = "de::opt_xs_date"
    )]
    pub date: Option<Date>,
    /// Title: the payment method's wire token (`jogcim`), preserved as received.
    #[doc(alias = "jogcím")]
    #[serde(default, rename(deserialize = "jogcim"))]
    pub title: Option<String>,
    /// Amount (`osszeg`).
    #[serde(
        default,
        rename(deserialize = "osszeg"),
        deserialize_with = "de::empty_as_none"
    )]
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
    pub amount: Option<Decimal>,
    /// Comment (`megjegyzes`).
    #[serde(default, rename(deserialize = "megjegyzes"))]
    pub comment: Option<String>,
    /// Bank account credited (`bankszamlaszam`).
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
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
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
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
    pub vat_rate: Option<Decimal>,
    /// Net value (`netto`).
    #[serde(
        default,
        rename(deserialize = "netto"),
        deserialize_with = "de::empty_as_none"
    )]
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
    pub net: Option<Decimal>,
    /// VAT value (`afa`).
    #[serde(
        default,
        rename(deserialize = "afa"),
        deserialize_with = "de::empty_as_none"
    )]
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
    pub vat: Option<Decimal>,
    /// Gross value (`brutto`).
    #[serde(
        default,
        rename(deserialize = "brutto"),
        deserialize_with = "de::empty_as_none"
    )]
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
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
/// (incoming). The two shapes are near-identical and share this type; which
/// one arrived is expressed by the [`Document`] variant / [`Handler`] method.
///
/// [`Handler`]: crate::Handler
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct InvoiceDocument {
    /// The supplier (`szallito`). Empty when the push omits the block: every
    /// field of a [`Party`] is optional, so an absent block and an empty one
    /// read the same.
    #[doc(alias = "szállító")]
    #[serde(default, rename(deserialize = "szallito"))]
    pub supplier: Party,
    /// Identity and metadata (`alap`).
    #[serde(rename(deserialize = "alap"))]
    pub info: InvoiceInfo,
    /// The buyer (`vevo`). Empty when the push omits the block.
    #[doc(alias = "vevő")]
    #[serde(default, rename(deserialize = "vevo"))]
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
    /// Recorded credit entries (`kifizetesek`).
    #[serde(
        default,
        rename(deserialize = "kifizetesek"),
        deserialize_with = "de::credit_entries"
    )]
    pub credit_entries: Vec<RecordedCreditEntry>,
    /// The invoice PDF (`pdf`), base64 on the wire, decoded here.
    ///
    /// `None` when the element is absent or empty, and when its content does
    /// not decode. The decoder forgives an encoder's sloppiness (missing
    /// padding, set trailing bits); what it still cannot read is not a reason
    /// to refuse the invoice, so the parse degrades the PDF to `None` and
    /// keeps the encoded text in [`raw_xml`](Self::raw_xml) for recovery.
    #[serde(default, deserialize_with = "de::base64_pdf")]
    pub pdf: Option<Pdf>,
    /// Exact UTF-8 XML request from which this document was parsed.
    #[serde(skip)]
    raw_xml: Option<Arc<str>>,
}

impl InvoiceDocument {
    /// Returns the exact pushed XML, when this value came from [`Document::parse`].
    #[must_use]
    pub fn raw_xml(&self) -> Option<&str> {
        self.raw_xml.as_deref()
    }

    /// Checks the invoice against `szamla.xsd` (outgoing) or `szamlabe.xsd`
    /// (incoming): every `minOccurs="1"` element, at least one `tetel` and
    /// one `afakulcsossz`, non-negative VAT rates. The direction matters
    /// because only an outgoing invoice's buyer carries
    /// `privatePersonIndicator`.
    ///
    /// # Errors
    ///
    /// The first requirement the invoice misses.
    pub fn validate(&self, direction: InvoiceDirection) -> Result<(), ValidationError> {
        required(
            self.info.economic_event_id.as_ref(),
            "invoice alap/gazdEsemAzon",
        )?;
        required_text(self.info.document_type.as_deref(), "invoice alap/tipus")?;
        required(self.info.appearance.as_ref(), "invoice alap/eszamla")?;
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
        if matches!(direction, InvoiceDirection::Outgoing) {
            required(
                self.buyer.private_person.as_ref(),
                "outgoing invoice vevo/privatePersonIndicator",
            )?;
        }

        if self.items.is_empty() {
            return Err(ValidationError::empty("invoice tetelek", "tetel"));
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
        for entry in &self.credit_entries {
            required(entry.date.as_ref(), "invoice kifizetes/datum")?;
            required_text(entry.title.as_deref(), "invoice kifizetes/jogcim")?;
            required(entry.amount.as_ref(), "invoice kifizetes/osszeg")?;
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

fn validate_totals(totals: &Totals, document: &str) -> Result<(), ValidationError> {
    if totals.per_vat_rate.is_empty() {
        return Err(ValidationError::empty(
            format!("{document} osszegek"),
            "afakulcsossz",
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

fn required<'a, T>(value: Option<&'a T>, path: &str) -> Result<&'a T, ValidationError> {
    value.ok_or_else(|| ValidationError::missing(path))
}

fn required_text<'a>(value: Option<&'a str>, path: &str) -> Result<&'a str, ValidationError> {
    value.ok_or_else(|| ValidationError::missing(path))
}

fn validate_address(address: &Address, field: &str) -> Result<(), ValidationError> {
    required_text(address.zip.as_deref(), &format!("{field}/irsz"))?;
    required_text(address.city.as_deref(), &format!("{field}/telepules"))?;
    required_text(address.address.as_deref(), &format!("{field}/cim"))?;

    Ok(())
}

fn non_negative(value: Option<Decimal>, path: &str) -> Result<(), ValidationError> {
    if value.is_some_and(|value| value.is_sign_negative()) {
        return Err(ValidationError::negative(path));
    }
    Ok(())
}

/// Direction of a bank transaction (`irany`).
///
/// `banktranz.xsd` enumerates `BE` and `KI`; a token outside the enumeration
/// is kept as [`Other`](Self::Other) so a protocol extension cannot fail the
/// delivery of a transaction (the strict [`BankTransaction::validate`] flags
/// it). Serializes as a plain string: the variant name for the two known
/// directions, the wire token for an unknown one.
///
/// Breaking change in 0.4: gained `Other(String)`, so it is no longer `Copy`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub enum TransactionDirection {
    /// `BE`: incoming.
    Incoming,
    /// `KI`: outgoing.
    Outgoing,
    /// A token `banktranz.xsd` does not enumerate, kept exactly as received.
    #[serde(untagged)]
    Other(String),
}

impl TransactionDirection {
    /// Reads the wire token: `BE`, `KI`, or anything else, trimmed, as
    /// [`Other`](Self::Other).
    fn from_code(code: &str) -> Self {
        match code.trim() {
            "BE" => Self::Incoming,
            "KI" => Self::Outgoing,
            other => Self::Other(other.to_owned()),
        }
    }

    /// The wire token: `BE`, `KI`, or the unknown token as received.
    #[must_use]
    pub fn code(&self) -> &str {
        match self {
            Self::Incoming => "BE",
            Self::Outgoing => "KI",
            Self::Other(code) => code,
        }
    }
}

/// Reads the wire token, like every other field of the crate's `Deserialize`
/// implementations (`BE` / `KI`, anything else as [`Other`](Self::Other)); the
/// `Serialize` side writes the Rust names.
impl<'de> serde::Deserialize<'de> for TransactionDirection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = String::deserialize(deserializer)?;
        Ok(Self::from_code(&code))
    }
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
/// Only the `id` is required to parse: it is what identifies the record. The
/// elements `banktranz.xsd` marks required (`bankszamla`, `erteknap`, `irany`,
/// `technikai`, `osszeg`, `devizanem`) are read as the wire delivers them,
/// `None` when absent or empty: a bank transaction answered non-200 is
/// retried for 72 hours and then dropped, so an omission szamlazz.hu made
/// must not cost the record. [`validate`](Self::validate) reports it.
///
/// Breaking change in 0.4: `bank_account`, `value_date`, `direction`,
/// `technical`, `amount` and `currency` were required, non-`Option` fields.
#[doc(alias = "banki tranzakció")]
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct BankTransaction {
    /// Transaction id (`id`).
    pub id: i64,
    /// The account's own bank account number (`bankszamla`).
    #[serde(default, rename(deserialize = "bankszamla"))]
    pub bank_account: Option<String>,
    /// Value date (`erteknap`).
    #[doc(alias = "értéknap")]
    #[serde(
        default,
        rename(deserialize = "erteknap"),
        deserialize_with = "de::opt_xs_date"
    )]
    pub value_date: Option<Date>,
    /// Direction (`irany`).
    #[serde(
        default,
        rename(deserialize = "irany"),
        deserialize_with = "de::opt_transaction_direction"
    )]
    pub direction: Option<TransactionDirection>,
    /// Transaction type (`tipus`). Named `kind` in 0.3.
    #[doc(alias = "tipus")]
    #[serde(default, rename(deserialize = "tipus"))]
    pub transaction_type: Option<String>,
    /// Technical (non-business) transaction flag (`technikai`).
    #[serde(
        default,
        rename(deserialize = "technikai"),
        deserialize_with = "de::opt_flexible_bool"
    )]
    pub technical: Option<bool>,
    /// Amount (`osszeg`).
    #[serde(
        default,
        rename(deserialize = "osszeg"),
        deserialize_with = "de::empty_as_none"
    )]
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
    pub amount: Option<Decimal>,
    /// Currency (`devizanem`).
    #[serde(default, rename(deserialize = "devizanem"))]
    pub currency: Option<String>,
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

    /// Checks the transaction against `banktranz.xsd`: every `minOccurs="1"`
    /// element and a known `irany`.
    ///
    /// # Errors
    ///
    /// The first requirement the transaction misses.
    pub fn validate(&self) -> Result<(), ValidationError> {
        required_text(self.bank_account.as_deref(), "bank transaction bankszamla")?;
        required(self.value_date.as_ref(), "bank transaction erteknap")?;
        match required(self.direction.as_ref(), "bank transaction irany")? {
            TransactionDirection::Incoming | TransactionDirection::Outgoing => {}
            TransactionDirection::Other(code) => {
                return Err(ValidationError::unknown_token(
                    "bank transaction irany",
                    code,
                ));
            }
        }
        required(self.technical.as_ref(), "bank transaction technikai")?;
        required(self.amount.as_ref(), "bank transaction osszeg")?;
        required_text(self.currency.as_deref(), "bank transaction devizanem")?;

        Ok(())
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
    /// Receipt type (`tipus`): `NY` receipt, `SN` storno receipt. Unknown
    /// codes are preserved. Named `kind` in 0.3.
    #[doc(alias = "tipus")]
    #[serde(default, rename(deserialize = "tipus"))]
    pub document_type: Option<String>,
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
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
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
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
    pub unit_price: Option<Decimal>,
    /// Quantity (`mennyiseg`).
    #[serde(
        default,
        rename(deserialize = "mennyiseg"),
        deserialize_with = "de::empty_as_none"
    )]
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
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
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
    pub net_value: Option<Decimal>,
    /// Optional NAV special VAT category (`afatipus`).
    #[serde(default, rename(deserialize = "afatipus"))]
    pub vat_type: Option<String>,
    /// Numeric VAT percentage (`afakulcs`).
    #[serde(
        default,
        rename(deserialize = "afakulcs"),
        deserialize_with = "de::empty_as_none"
    )]
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
    pub vat_rate: Option<Decimal>,
    /// VAT value (`afa`).
    #[serde(
        default,
        rename(deserialize = "afa"),
        deserialize_with = "de::empty_as_none"
    )]
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
    pub vat_value: Option<Decimal>,
    /// Gross value (`brutto`).
    #[serde(
        default,
        rename(deserialize = "brutto"),
        deserialize_with = "de::empty_as_none"
    )]
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
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
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
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

/// A pushed receipt batch (`<xmlnyugtaarchiv>`): receipts are delivered in
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

    /// Checks the batch against `xmlnyugtaarchiv.xsd`: at least one
    /// `nyugta`, every receipt's `minOccurs="1"` elements, at least one
    /// `tetel` and one `afakulcsossz` per receipt, non-negative VAT rates.
    /// `alap/adoszam` is not required although the schema says so: official
    /// batches omit it.
    ///
    /// # Errors
    ///
    /// The first requirement the batch misses.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.receipts.is_empty() {
            return Err(ValidationError::empty("receipt archive", "nyugta"));
        }
        for receipt in &self.receipts {
            required_text(
                receipt.info.receipt_number.as_deref(),
                "receipt alap/nyugtaszam",
            )?;
            required_text(receipt.info.document_type.as_deref(), "receipt alap/tipus")?;
            required(receipt.info.reversed.as_ref(), "receipt alap/stornozott")?;
            required(receipt.info.issue_date.as_ref(), "receipt alap/kelt")?;
            required_text(
                receipt.info.payment_method.as_deref(),
                "receipt alap/fizmod",
            )?;
            required_text(receipt.info.currency.as_deref(), "receipt alap/penznem")?;
            required(receipt.info.test.as_ref(), "receipt alap/teszt")?;
            if receipt.items.is_empty() {
                return Err(ValidationError::empty("receipt tetelek", "tetel"));
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
///
/// Every helper reads wire text an authenticated push delivered, which may
/// hold any UTF-8. The rule for it: never index or split a `str` by a byte
/// offset the text has not been shown to have a char boundary at. Byte
/// positions are read through `strip_suffix`, `split_at_checked` or a slice
/// pattern over `as_bytes()`, so that no text of any length or encoding
/// panics; a panic here escapes the router after the key check and leaves
/// szamlazz.hu with no answer to retry.
pub(crate) mod de {
    use serde::{Deserialize, Deserializer};

    use super::{Date, InvoiceAppearance, Pdf, TransactionDirection};

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
    ///
    /// The `±hh:mm` form is the last six bytes of the text; the split is
    /// boundary-checked because the text is wire content and byte six from
    /// the end may fall inside a multi-byte character.
    fn strip_xs_date_timezone(text: &str) -> &str {
        if let Some(date) = text.strip_suffix('Z') {
            return date;
        }
        let Some((date, suffix)) = text
            .len()
            .checked_sub(6)
            .and_then(|at| text.split_at_checked(at))
        else {
            return text;
        };
        if is_xs_timezone_offset(suffix) {
            date
        } else {
            text
        }
    }

    /// Whether `suffix` is exactly an `xs:date` `±hh:mm` offset as XML Schema
    /// bounds it: `-14:00..=+14:00`, the hour `14` with the minute `00` only.
    /// A slice pattern over the bytes, so a suffix of any other length or
    /// content is simply `false`; a suffix of the right form outside the
    /// range is not the schema's timezone but text that is not a date.
    fn is_xs_timezone_offset(suffix: &str) -> bool {
        let &[
            b'+' | b'-',
            h1 @ b'0'..=b'9',
            h2 @ b'0'..=b'9',
            b':',
            m1 @ b'0'..=b'5',
            m2 @ b'0'..=b'9',
        ] = suffix.as_bytes()
        else {
            return false;
        };
        let hours = (h1 - b'0') * 10 + (h2 - b'0');
        let minutes = (m1 - b'0') * 10 + (m2 - b'0');

        hours < 14 || (hours == 14 && minutes == 0)
    }

    /// Deserializes an optional `xs:date`, reading empty elements as absent
    /// and discarding any timezone suffix. A text that is not a date is
    /// content, not shape: it reads as `None` like an omitted element, the
    /// text stays in the document's raw XML, and where the XSD requires the
    /// date the strict parse reports the requirement it fails to meet. Never
    /// an error for any text: the push must be Acked whatever one date
    /// element holds.
    pub fn opt_xs_date<'de, D>(deserializer: D) -> Result<Option<Date>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<String>::deserialize(deserializer)?;

        Ok(value
            .as_deref()
            .map(str::trim)
            .and_then(|text| strip_xs_date_timezone(text).parse().ok()))
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

    /// Deserializes an optional `<irany>`, reading an empty element as absent
    /// and any non-empty token (known or not) as a direction.
    pub fn opt_transaction_direction<'de, D>(
        deserializer: D,
    ) -> Result<Option<TransactionDirection>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(empty_string_as_none(deserializer)?.map(|code| TransactionDirection::from_code(&code)))
    }

    /// The base64 decoder for a pushed `<pdf>`: the standard alphabet with
    /// padding indifferent and trailing bits ignored, so an encoder's
    /// sloppiness (`JVBERi0` for `JVBERi0=`, a set bit in the last symbol)
    /// still yields the PDF. `szamla.xsd` types the element `string`;
    /// base64 is this crate's reading of it.
    const PDF_ENGINE: base64::engine::GeneralPurpose = base64::engine::GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        base64::engine::GeneralPurposeConfig::new()
            .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent)
            .with_decode_allow_trailing_bits(true),
    );

    /// Deserializes the `<pdf>` element: absent or empty is `None`, and so is
    /// content [`PDF_ENGINE`] cannot decode; a document-content detail must
    /// not decide the delivery of a legal record. The encoded text stays in
    /// the document's raw XML.
    pub fn base64_pdf<'de, D>(deserializer: D) -> Result<Option<Pdf>, D::Error>
    where
        D: Deserializer<'de>,
    {
        use base64::Engine as _;
        let value = Option::<String>::deserialize(deserializer)?;

        Ok(value.and_then(|encoded| {
            let compact: String = encoded.split_whitespace().collect();

            if compact.is_empty() {
                return None;
            }
            PDF_ENGINE.decode(compact).ok().map(Pdf)
        }))
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
    wrapped_list!(credit_entries, "kifizetes", super::RecordedCreditEntry);
    wrapped_list!(receipt_items, "tetel", super::ReceiptItem);
    wrapped_list!(receipt_payments, "kifizetes", super::ReceiptPayment);
    wrapped_list!(financial_items, "qutet", super::FinancialItem);
    wrapped_list!(tags, "cimke", String);
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTGOING_INVOICE: &str = include_str!("../tests/synthetic/szamla.xml");
    const BANK_TRANSACTION: &str = include_str!("../tests/synthetic/banktranz.xml");
    const RECEIPT_BATCH: &str = include_str!("../tests/synthetic/xmlnyugtaarchiv.xml");

    fn outgoing(body: &str) -> InvoiceDocument {
        match Document::parse(body.as_bytes()).expect("parses") {
            Document::OutgoingInvoice(invoice) => invoice,
            other => panic!("expected outgoing invoice, got {other:?}"),
        }
    }

    fn bank_transaction(body: &str) -> BankTransaction {
        match Document::parse(body.as_bytes()).expect("parses") {
            Document::BankTransaction(transaction) => transaction,
            other => panic!("expected bank transaction, got {other:?}"),
        }
    }

    /// The fixture with `element` replaced by `replacement`, asserting the
    /// fixture still carries the element.
    fn with(fixture: &str, element: &str, replacement: &str) -> String {
        assert!(fixture.contains(element), "fixture drifted: {element}");
        fixture.replacen(element, replacement, 1)
    }

    #[test]
    fn identify_reads_the_root_and_its_namespace_only() {
        // Every fixture identifies as its root; nothing past the start tag is
        // read, so a body that would not parse still identifies.
        for (body, kind) in [
            (OUTGOING_INVOICE, RootKind::OutgoingInvoice),
            (BANK_TRANSACTION, RootKind::BankTransaction),
            (RECEIPT_BATCH, RootKind::Receipts),
        ] {
            assert_eq!(
                Document::identify(body.as_bytes()).expect("identifies"),
                kind
            );
            assert_eq!(
                Document::parse(body.as_bytes()).expect("parses").kind(),
                kind
            );
        }
        let incoming = OUTGOING_INVOICE
            .replace(
                "http://www.szamlazz.hu/szamla",
                "http://www.szamlazz.hu/szamlabe",
            )
            .replace("<szamla xmlns=", "<szamlabe xmlns=")
            .replace("</szamla>", "</szamlabe>");
        assert_eq!(
            Document::identify(incoming.as_bytes()).expect("identifies"),
            RootKind::IncomingInvoice
        );

        let truncated = &OUTGOING_INVOICE.as_bytes()[..OUTGOING_INVOICE.len() - 20];
        assert_eq!(
            Document::identify(truncated).expect("the start tag is enough"),
            RootKind::OutgoingInvoice
        );
        let without_id = with(OUTGOING_INVOICE, "<id>123456</id>", "");
        assert_eq!(
            Document::identify(without_id.as_bytes()).expect("identity is the parse's concern"),
            RootKind::OutgoingInvoice
        );
        // A prefixed root binds its namespace through the prefix.
        let prefixed = br#"<s:szamla xmlns:s="http://www.szamlazz.hu/szamla"/>"#;
        assert_eq!(
            Document::identify(prefixed).expect("identifies"),
            RootKind::OutgoingInvoice
        );
    }

    #[test]
    fn identify_refuses_a_body_without_a_known_namespaced_root() {
        assert!(matches!(
            Document::identify(b"\xff<szamla/>"),
            Err(ParseError::Utf8(_))
        ));
        assert!(matches!(Document::identify(b""), Err(ParseError::Empty)));
        assert!(matches!(
            Document::identify(b"<?xml version=\"1.0\"?>"),
            Err(ParseError::Empty)
        ));
        assert!(matches!(
            Document::identify(b"<whatever/>"),
            Err(ParseError::UnknownRoot(root)) if root == "whatever"
        ));
        for kind in [
            RootKind::OutgoingInvoice,
            RootKind::IncomingInvoice,
            RootKind::BankTransaction,
            RootKind::Receipts,
        ] {
            let unqualified = format!("<{kind}/>");
            assert!(matches!(
                Document::identify(unqualified.as_bytes()),
                Err(ParseError::WrongNamespace { root, expected, actual })
                    if root == kind.root_element() && expected == kind.namespace() && actual.is_empty()
            ));
            let wrong = format!(r#"<{kind} xmlns="https://wrong.example"/>"#);
            assert!(matches!(
                Document::identify(wrong.as_bytes()),
                Err(ParseError::WrongNamespace { actual, .. }) if actual == "https://wrong.example"
            ));
        }
        assert!(matches!(
            Document::identify(b"not xml at all"),
            Err(ParseError::Empty)
        ));
        assert!(matches!(
            Document::identify(b"<szamla xmlns=\"http://www.szamlazz.hu/szamla\" <"),
            Err(ParseError::Xml(_))
        ));
    }

    #[test]
    fn root_kind_names_both_roots_and_the_direction() {
        for (kind, root, ack_root, direction) in [
            (
                RootKind::OutgoingInvoice,
                "szamla",
                "szamlavalasz",
                Some(InvoiceDirection::Outgoing),
            ),
            (
                RootKind::IncomingInvoice,
                "szamlabe",
                "szamlabevalasz",
                Some(InvoiceDirection::Incoming),
            ),
            (
                RootKind::BankTransaction,
                "banktranz",
                "banktranzvalasz",
                None,
            ),
            (RootKind::Receipts, "xmlnyugtaarchiv", "nyugtavalasz", None),
        ] {
            assert_eq!(kind.root_element(), root);
            assert_eq!(kind.to_string(), root);
            assert_eq!(kind.namespace(), format!("http://www.szamlazz.hu/{root}"));
            assert_eq!(kind.ack_root_element(), ack_root);
            assert_eq!(kind.direction(), direction);
            if let Some(direction) = direction {
                assert_eq!(RootKind::from(direction), kind);
            }
        }
    }

    #[test]
    fn empty_optional_numeric_elements_read_as_absent() {
        let body = OUTGOING_INVOICE
            .replace("<forras>34</forras>", "<forras></forras>")
            .replace("<id>1234567</id>", "<id></id>");
        let invoice = outgoing(&body);
        assert_eq!(invoice.info.source, None);
        assert_eq!(invoice.buyer.id, None);
        // The fixture's <rendelesszam> is empty on the wire.
        assert_eq!(invoice.info.order_number, None);
    }

    #[test]
    fn unknown_receipt_type_is_preserved() {
        let body = with(RECEIPT_BATCH, "<tipus>NY</tipus>", "<tipus>XX</tipus>");
        let Document::Receipts(batch) =
            Document::parse(body.as_bytes()).expect("unknown receipt tipus must not fail")
        else {
            panic!("expected receipt batch");
        };
        assert_eq!(batch.receipts[0].info.document_type.as_deref(), Some("XX"));
    }

    #[test]
    fn technical_flag_reads_leniently_but_must_be_a_boolean() {
        for (value, expected) in [("true", true), ("1", true), ("false", false), ("0", false)] {
            let body = with(
                BANK_TRANSACTION,
                "<technikai>false</technikai>",
                &format!("<technikai>{value}</technikai>"),
            );
            assert_eq!(bank_transaction(&body).technical, Some(expected));
        }

        // Absent or empty is content: the flag reads as absent, and only the
        // strict parse minds.
        let body = with(
            BANK_TRANSACTION,
            "<technikai>false</technikai>",
            "<technikai/>",
        );
        let transaction = bank_transaction(&body);
        assert_eq!(transaction.technical, None);
        assert_eq!(
            transaction.validate().expect_err("the XSD requires it"),
            ValidationError::MissingRequired {
                path: "bank transaction technikai".to_owned()
            }
        );

        // A token that is not a boolean is shape: the element is not what
        // the document says it is.
        let body = with(
            BANK_TRANSACTION,
            "<technikai>false</technikai>",
            "<technikai>maybe</technikai>",
        );
        assert!(Document::parse(body.as_bytes()).is_err());
    }

    #[test]
    fn xs_date_timezone_suffix_is_discarded() {
        // xs:date permits an optional timezone suffix; the offset is
        // discarded, the civil date kept.
        for value in [
            "2026-07-03",
            "2026-07-03Z",
            "2026-07-03+02:00",
            "2026-07-03-05:00",
        ] {
            let body = with(
                BANK_TRANSACTION,
                "<erteknap>2026-07-03</erteknap>",
                &format!("<erteknap>{value}</erteknap>"),
            );
            assert_eq!(
                bank_transaction(&body).value_date,
                Some(jiff::civil::date(2026, 7, 3)),
                "{value}"
            );
        }

        let body = with(
            OUTGOING_INVOICE,
            "<kelt>2015-12-01</kelt>",
            "<kelt>2015-12-01+01:00</kelt>",
        );
        assert_eq!(
            outgoing(&body).info.issue_date,
            Some(jiff::civil::date(2015, 12, 1))
        );

        // Garbage after the date is content: the date reads as absent, the
        // text stays in the raw XML, and only the strict parse minds.
        let body = with(
            OUTGOING_INVOICE,
            "<kelt>2015-12-01</kelt>",
            "<kelt>2015-12-01junk</kelt>",
        );
        let invoice = outgoing(&body);
        assert_eq!(invoice.info.issue_date, None);
        assert!(
            invoice
                .raw_xml()
                .is_some_and(|xml| xml.contains("<kelt>2015-12-01junk</kelt>"))
        );
        assert_eq!(
            invoice
                .validate(InvoiceDirection::Outgoing)
                .expect_err("the XSD requires a date")
                .to_string(),
            "missing required invoice alap/kelt"
        );
    }

    /// Feeds `text` through the optional-date deserializer as the content of
    /// one element, the way a pushed `<kelt>` reaches it.
    fn read_opt_date(text: &str) -> Result<Option<Date>, quick_xml::DeError> {
        #[derive(serde::Deserialize)]
        struct Wrapper {
            #[serde(default, deserialize_with = "de::opt_xs_date")]
            date: Option<Date>,
        }
        quick_xml::de::from_str::<Wrapper>(&format!("<w><date>{text}</date></w>"))
            .map(|wrapper| wrapper.date)
    }

    #[test]
    fn xs_date_that_is_not_a_date_reads_as_absent_whatever_its_bytes() {
        // The timezone strip once split the text six bytes from its end;
        // `str::split_at` panics when that index is inside a multi-byte
        // character. Every text here is seven or more bytes long, most with
        // a non-ASCII character in the way of that split: none may panic,
        // and none is a date.
        for text in [
            // Seven bytes; the split at byte 1 falls inside `é`.
            "é12345",
            // Seven bytes ending in a multi-byte character; the split at
            // byte 1 falls inside the first `é`.
            "éé€",
            // Seven bytes ending in a multi-byte character; the split lands
            // on a boundary, the suffix is not an offset.
            "12345é",
            // A well-formed date with a multi-byte character where the
            // offset's last digit would be.
            "2024-01-01+01:0é",
            // A multi-byte character in front of a well-formed offset.
            "é+01:00",
            // Every byte is a boundary; the suffix is not an offset.
            "2024-01-01junk",
        ] {
            assert_eq!(
                read_opt_date(text).unwrap_or_else(|error| panic!("{text:?}: {error}")),
                None,
                "{text:?}"
            );
        }

        assert_eq!(
            read_opt_date("2024-01-01+01:00").expect("schema-valid xs:date"),
            Some(jiff::civil::date(2024, 1, 1))
        );
        assert_eq!(
            read_opt_date("2024-01-01").expect("plain xs:date"),
            Some(jiff::civil::date(2024, 1, 1))
        );
        assert_eq!(read_opt_date("").expect("empty is absent"), None);
    }

    #[test]
    fn xs_date_timezone_offset_is_the_schemas_range() {
        // XML Schema bounds the offset to -14:00..=+14:00, the hour 14 with
        // the minute 00 only. Inside it the suffix is the schema's timezone
        // and is discarded; outside it the text is not a date.
        for text in [
            "2024-01-01+14:00",
            "2024-01-01-14:00",
            "2024-01-01+13:59",
            "2024-01-01-00:00",
            "2024-01-01+00:00",
        ] {
            assert_eq!(
                read_opt_date(text).unwrap_or_else(|error| panic!("{text:?}: {error}")),
                Some(jiff::civil::date(2024, 1, 1)),
                "{text:?}"
            );
        }
        for text in [
            "2024-01-01+14:01",
            "2024-01-01-14:30",
            "2024-01-01+15:00",
            "2024-01-01+99:99",
            "2024-01-01+01:60",
        ] {
            assert_eq!(
                read_opt_date(text).unwrap_or_else(|error| panic!("{text:?}: {error}")),
                None,
                "{text:?}"
            );
        }
    }
}
