//! Invoice XML query (`xmlszamlaxml`): fetch the full data of a previously
//! issued invoice, optionally with its PDF.
//!
//! The public response types carry Rust field names and plain `serde`
//! derives, so a fetched [`InvoiceDocument`] round-trips through JSON (for
//! journaling or caching) independently of the XML schema. The wire mapping —
//! Hungarian element names, list wrappers, lenient empty-element handling —
//! lives in the private `*Xml` structs after the [`AgentRequest`] impl.

use jiff::civil::Date;
use rust_decimal::Decimal;

use super::query_pdf::InvoiceSelector;
use crate::credentials::Credentials;
use crate::error::{ParseError, ResponseError};
use crate::types::{InvoiceNumber, Pdf, VatRate};
use crate::wire::{AgentRequest, RawResponse};
use crate::xml;

/// The invoice XML query (`xmlszamlaxml`, `action-szamla_agent_xml`).
///
/// Like the PDF query, this request document has no `beallitasok` block: the
/// credentials sit directly under the root element. The response is the
/// invoice's full data as an [`InvoiceDocument`].
#[doc(alias = "xmlszamlaxml")]
#[doc(alias = "számla adatainak lekérdezése")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct QueryInvoiceXml {
    /// Which invoice to fetch.
    pub selector: InvoiceSelector,
    /// Also return the invoice PDF (`pdf`); it arrives decoded in
    /// [`InvoiceDocument::pdf`].
    pub include_pdf: bool,
}

impl QueryInvoiceXml {
    /// A data query for the invoice named by `selector`; no PDF is requested.
    #[must_use]
    pub fn new(selector: InvoiceSelector) -> Self {
        Self {
            selector,
            include_pdf: false,
        }
    }
}

/// A fetched invoice: the `szamla` response document.
#[doc(alias = "számla")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct InvoiceDocument {
    /// The issuing party (`szallito`).
    pub supplier: Supplier,
    /// Core invoice data (`alap`).
    pub info: InvoiceInfo,
    /// The buyer (`vevo`).
    pub buyer: BuyerInfo,
    /// Line items (`tetelek`).
    pub items: Vec<DocumentItem>,
    /// Financial items (`qutetek`).
    pub financial_items: Vec<FinancialItem>,
    /// Invoice-level labels (`cimkek`).
    pub labels: Vec<String>,
    /// Totals (`osszegek`).
    pub totals: Totals,
    /// Payments recorded against the invoice (`kifizetesek`).
    pub payments: Vec<RecordedPayment>,
    /// The invoice PDF, when requested via [`QueryInvoiceXml::include_pdf`].
    pub pdf: Option<Pdf>,
}

/// An address block (`cim`) on a fetched invoice.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct Address {
    /// Country (`orszag`).
    pub country: Option<String>,
    /// ZIP code (`irsz`).
    pub zip: String,
    /// City (`telepules`).
    pub city: String,
    /// Street address (`cim`).
    pub address: String,
}

/// Bank details of the supplier (`bank`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct Bank {
    /// Bank name (`nev`).
    pub name: Option<String>,
    /// Bank account number (`bankszamla`).
    pub account: Option<String>,
}

/// The issuing party (`szallito`) as recorded on the invoice.
#[doc(alias = "szállító")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct Supplier {
    /// Internal szamlazz.hu identifier (`id`).
    pub id: Option<u64>,
    /// Name (`nev`).
    pub name: String,
    /// Billing address (`cim`).
    pub address: Address,
    /// Postal address (`postacim`).
    pub postal_address: Option<Address>,
    /// Hungarian tax number (`adoszam`).
    #[doc(alias = "adószám")]
    pub tax_number: Option<String>,
    /// VAT-group identifier (`csoportazonosito`).
    pub group_id: Option<String>,
    /// EU tax number (`adoszameu`).
    pub eu_tax_number: Option<String>,
    /// Bank details (`bank`).
    pub bank: Option<Bank>,
}

/// The documented integer value of `<eszamla>` in queried invoice XML.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum InvoiceAppearance {
    /// `0`: the document is not an invoice, for example a proforma.
    NotInvoice,
    /// `1`: paper invoice.
    Paper,
    /// `2` or `3`: e-invoice, retaining the exact code.
    Electronic(i32),
    /// Any future integer code.
    Unknown(i32),
}

impl InvoiceAppearance {
    /// Returns the exact integer received from szamlazz.hu.
    #[must_use]
    pub fn code(self) -> i32 {
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
impl From<i32> for InvoiceAppearance {
    fn from(code: i32) -> Self {
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
        serializer.serialize_str(&self.code().to_string())
    }
}

impl<'de> serde::Deserialize<'de> for InvoiceAppearance {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code: i32 = <String as serde::Deserialize>::deserialize(deserializer)?
            .trim()
            .parse()
            .map_err(serde::de::Error::custom)?;
        Ok(Self::from(code))
    }
}

/// Core invoice data (`alap`).
#[doc(alias = "alap")]
// These booleans mirror independent protocol fields, not a single state.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct InvoiceInfo {
    /// Internal szamlazz.hu identifier (`id`).
    pub id: u64,
    /// The invoice number (`szamlaszam`).
    pub invoice_number: InvoiceNumber,
    /// Economic-event identifier (`gazdEsemAzon`).
    pub economic_event_id: Option<u64>,
    /// Source system code (`forras`) for externally issued invoices.
    pub source: Option<u32>,
    /// Registration number (`iktatoszam`).
    pub registration_number: Option<String>,
    /// Document type code (`tipus`), e.g. `SZ` for an invoice or `D` for a
    /// proforma; kept verbatim as the code set is not documented exhaustively.
    pub document_type: String,
    /// Document appearance (`eszamla`): not an invoice, paper, or electronic.
    #[doc(alias = "e-számla")]
    pub e_invoice: InvoiceAppearance,
    /// Referenced invoice number (`hivszamlaszam`).
    pub referenced_invoice_number: Option<InvoiceNumber>,
    /// Referenced proforma number (`hivdijbekszam`).
    pub referenced_proforma_number: Option<InvoiceNumber>,
    /// Issue date (`kelt`).
    pub issue_date: Option<Date>,
    /// Fulfillment date (`telj`).
    #[doc(alias = "teljesítés dátum")]
    pub fulfillment_date: Option<Date>,
    /// Payment due date (`fizh`).
    #[doc(alias = "fizetési határidő")]
    pub due_date: Option<Date>,
    /// Payment method as recorded (`fizmod`), free text.
    pub payment_method: Option<String>,
    /// Payment method normalized to szamlazz.hu's unified set
    /// (`fizmodunified`).
    pub unified_payment_method: Option<String>,
    /// Whether the payment method is cash (`keszpenz`).
    pub cash_payment: bool,
    /// Order number (`rendelesszam`).
    pub order_number: Option<String>,
    /// Document language (`nyelv`).
    pub language: Option<String>,
    /// Currency (`devizanem`).
    #[doc(alias = "pénznem")]
    pub currency: Option<String>,
    /// Foreign-currency quoting bank (`devizabank`).
    pub exchange_bank: Option<String>,
    /// Exchange rate (`devizaarf`).
    #[doc(alias = "árfolyam")]
    pub exchange_rate: Option<Decimal>,
    /// Comment shown on the document (`megjegyzes`).
    pub comment: Option<String>,
    /// Invoice-level VAT category (`afatipus`).
    pub vat_type: Option<String>,
    /// Issued under cash accounting (`penzforg`).
    #[doc(alias = "pénzforgalmi elszámolás")]
    pub cash_accounting: bool,
    /// Issued under KATA taxation (`kata`).
    pub kata: bool,
    /// Whether KATA ledger handling applies (`katafokonyv`).
    pub kata_ledger: bool,
    /// Buyer email the document was sent to (`email`).
    pub email: Option<String>,
    /// Issued from a test account (`teszt`).
    pub test: bool,
    /// Whether the invoice has been reversed (`sztornozott`).
    ///
    /// Mirrors the wire, where the element is optional and never spelled
    /// `false`:
    ///
    /// - `None` — the element is absent: the invoice has not been reversed,
    ///   or the document is itself a storno invoice (the marker never appears
    ///   on the storno invoice; it references its original through
    ///   [`referenced_invoice_number`](Self::referenced_invoice_number)).
    /// - `Some(true)` — `<sztornozott>true</sztornozott>`: this invoice has
    ///   been reversed by a storno invoice. Reversal also removes its recorded
    ///   [`payments`](InvoiceDocument::payments) from the response.
    /// - `Some(false)` — accepted for schema completeness; not observed.
    ///
    /// Breaking change in 0.x: this was a `bool` defaulting to `false` when the
    /// element was absent. Treat `reversed != Some(true)` as "live".
    #[doc(alias = "sztornózott")]
    pub reversed: Option<bool>,
}

/// Postal address returned for a buyer (`postacim`); every component is
/// optional in the response schema.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct BuyerPostalAddress {
    /// Recipient name (`nev`).
    pub name: Option<String>,
    /// Country (`orszag`).
    pub country: Option<String>,
    /// ZIP code (`irsz`).
    pub zip: Option<String>,
    /// City (`telepules`).
    pub city: Option<String>,
    /// Street address (`cim`).
    pub address: Option<String>,
}

/// Buyer ledger data returned under `vevo/fokonyv`.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct BuyerLedgerInfo {
    /// Buyer general-ledger account (`vevo`).
    pub account: Option<String>,
    /// Buyer identifier (`vevoazon`).
    pub buyer_id: Option<String>,
    /// Accounting date (`datum`).
    pub date: Option<Date>,
    /// Continuous fulfillment (`folyamatostelj`).
    pub continuous_fulfillment: Option<bool>,
    /// Settlement period start (`elszDatTol`).
    pub settlement_from: Option<Date>,
    /// Settlement period end (`elszDatIg`).
    pub settlement_to: Option<Date>,
}

/// The buyer (`vevo`) as recorded on the invoice.
#[doc(alias = "vevő")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct BuyerInfo {
    /// Internal szamlazz.hu identifier (`id`).
    pub id: Option<u64>,
    /// Name (`nev`).
    pub name: String,
    /// Partner identifier (`azonosito`).
    pub identifier: Option<String>,
    /// Billing address (`cim`).
    pub address: Option<Address>,
    /// Postal address (`postacim`).
    pub postal_address: Option<BuyerPostalAddress>,
    /// Email address (`email`).
    pub email: Option<String>,
    /// Hungarian tax number (`adoszam`).
    #[doc(alias = "adószám")]
    pub tax_number: Option<String>,
    /// VAT-group identifier (`csoportazonosito`).
    pub group_id: Option<String>,
    /// EU tax number (`adoszameu`).
    pub eu_tax_number: Option<String>,
    /// Buyer-location classification (`lokacio`): `1` domestic, `2` EU, `3`
    /// outside the EU, or `-1` unknown.
    pub location: Option<i64>,
    /// NAV private-person indicator (`privatePersonIndicator`).
    pub private_person: bool,
    /// Buyer ledger metadata (`fokonyv`).
    pub ledger: Option<BuyerLedgerInfo>,
}

/// One row of the fetched invoice (`tetel`).
#[doc(alias = "tétel")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct DocumentItem {
    /// Item name (`nev`).
    pub name: String,
    /// Item identifier (`azonosito`).
    pub id: Option<String>,
    /// Quantity (`mennyiseg`).
    pub quantity: Decimal,
    /// Unit of measure (`mennyisegiegyseg`).
    pub unit: String,
    /// Net unit price (`nettoegysegar`).
    pub unit_price: Decimal,
    /// VAT category (`afatipus`), when the row uses a special VAT code.
    #[doc(alias = "áfatípus")]
    pub vat_type: Option<String>,
    /// Numeric VAT rate wire token (`afakulcs`); see
    /// [`DocumentItem::vat_rate`].
    #[doc(alias = "áfakulcs")]
    pub vat_rate_code: String,
    /// Net value (`netto`).
    pub net_value: Decimal,
    /// Margin-scheme VAT base (`arresafaalap`).
    pub margin_vat_base: Option<Decimal>,
    /// VAT value (`afa`).
    pub vat_value: Decimal,
    /// Gross value (`brutto`).
    pub gross_value: Decimal,
    /// Row comment (`megjegyzes`).
    pub comment: Option<String>,
    /// Stable item ordering (`sztetordering`).
    pub ordering: Option<u32>,
    /// Item ledger metadata (`fokonyv`).
    pub ledger: Option<DocumentItemLedger>,
}

/// Item ledger data returned under `tetel/fokonyv`.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct DocumentItemLedger {
    /// Revenue account (`arbevetel`).
    pub revenue_account: Option<String>,
    /// VAT account (`afa`).
    pub vat_account: Option<String>,
    /// Economic event (`gazdasagiesemeny`).
    pub economic_event: Option<String>,
    /// VAT economic event (`gazdasagiesemenyafa`).
    pub vat_economic_event: Option<String>,
    /// Settlement period start (`elszdattol`).
    pub settlement_from: Option<Date>,
    /// Settlement period end (`elszdatig`).
    pub settlement_to: Option<Date>,
}

impl DocumentItem {
    /// The VAT type (`afatipus`) when present, otherwise the numeric rate
    /// (`afakulcs`), parsed into a [`VatRate`].
    #[must_use]
    pub fn vat_rate(&self) -> VatRate {
        VatRate::from(self.vat_type.as_deref().unwrap_or(&self.vat_rate_code))
    }
}

/// Invoice totals (`osszegek`).
#[doc(alias = "összegek")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct Totals {
    /// Per-VAT-rate subtotals (`afakulcsossz`).
    pub by_vat_rate: Vec<VatTotal>,
    /// Grand total (`totalossz`).
    pub total: GrandTotal,
}

/// Subtotal for one VAT rate (`afakulcsossz`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct VatTotal {
    /// VAT category (`afatipus`), when this subtotal uses a special VAT code.
    #[doc(alias = "áfatípus")]
    pub vat_type: Option<String>,
    /// Numeric VAT rate wire token (`afakulcs`); see [`VatTotal::vat_rate`].
    #[doc(alias = "áfakulcs")]
    pub vat_rate_code: String,
    /// Net subtotal (`netto`).
    pub net: Decimal,
    /// VAT subtotal (`afa`).
    pub vat: Decimal,
    /// Gross subtotal (`brutto`).
    pub gross: Decimal,
}

impl VatTotal {
    /// The VAT type (`afatipus`) when present, otherwise the numeric rate
    /// (`afakulcs`), parsed into a [`VatRate`].
    #[must_use]
    pub fn vat_rate(&self) -> VatRate {
        VatRate::from(self.vat_type.as_deref().unwrap_or(&self.vat_rate_code))
    }
}

/// A financial item (`qutet`) returned alongside invoice line items.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct FinancialItem {
    /// Name (`nev`).
    pub name: String,
    /// VAT category (`afatipus`).
    pub vat_type: Option<String>,
    /// Numeric VAT rate (`afakulcs`).
    pub vat_rate_code: String,
    /// Net amount (`netto`).
    pub net: Decimal,
    /// VAT amount (`afa`).
    pub vat: Decimal,
    /// Gross amount (`brutto`).
    pub gross: Decimal,
    /// Settlement period start (`elszdattol`).
    pub settlement_from: Option<Date>,
    /// Settlement period end (`elszdatig`).
    pub settlement_to: Option<Date>,
    /// Deductible VAT percentage (`afalevon`).
    pub deductible_vat: i32,
    /// Labels (`cimkek`).
    pub labels: Vec<String>,
}

impl FinancialItem {
    /// The VAT type (`afatipus`) when present, otherwise `afakulcs`.
    #[must_use]
    pub fn vat_rate(&self) -> VatRate {
        VatRate::from(self.vat_type.as_deref().unwrap_or(&self.vat_rate_code))
    }
}

/// The invoice grand total (`totalossz`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct GrandTotal {
    /// Net total (`netto`).
    pub net: Decimal,
    /// VAT total (`afa`).
    pub vat: Decimal,
    /// Gross total (`brutto`).
    pub gross: Decimal,
}

/// A payment recorded against the invoice (`kifizetes`).
#[doc(alias = "kifizetés")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct RecordedPayment {
    /// Payment date (`datum`).
    pub date: Date,
    /// Payment title (`jogcim`), e.g. `transfer`.
    #[doc(alias = "jogcím")]
    pub title: String,
    /// Amount (`osszeg`).
    pub amount: Decimal,
    /// Comment (`megjegyzes`).
    pub comment: Option<String>,
    /// Bank account the payment arrived on (`bankszamlaszam`).
    pub bank_account: Option<String>,
    /// Bank transaction identifier (`banktranzid`).
    pub bank_transaction_id: Option<u64>,
    /// Exchange rate used for the payment (`devizaarf`).
    pub exchange_rate: Option<Decimal>,
}

impl AgentRequest for QueryInvoiceXml {
    const ACTION: &'static str = "action-szamla_agent_xml";
    type Response = InvoiceDocument;

    fn write_xml(&self, credentials: &Credentials) -> Vec<u8> {
        xml::document(
            "xmlszamlaxml",
            "http://www.szamlazz.hu/xmlszamlaxml",
            |root| {
                root.credentials(credentials);
                match &self.selector {
                    InvoiceSelector::InvoiceNumber(number) => {
                        root.text("szamlaszam", number.as_str());
                    }
                    InvoiceSelector::OrderNumber(number) => root.text("rendelesSzam", number),
                    InvoiceSelector::ExternalId(_) => {}
                }
                root.bool("pdf", self.include_pdf);
                if let InvoiceSelector::ExternalId(id) = &self.selector {
                    root.text("szamlaKulsoAzon", id);
                }
            },
        )
    }

    fn parse(&self, response: &RawResponse) -> Result<Self::Response, ResponseError> {
        response.check()?;
        match response_root(response.body())? {
            ResponseRoot::AgentResponse => {
                crate::ops::invoice::InvoiceResponse::from_body(response.body())?.into_success()?;
                return Err(ParseError::UnexpectedBody(
                    "successful xmlszamlavalasz for an XML query".to_owned(),
                )
                .into());
            }
            ResponseRoot::Invoice => {}
        }
        let text = std::str::from_utf8(response.body()).map_err(|error| ParseError::Invalid {
            field: "response body",
            message: error.to_string(),
        })?;
        let szamla: SzamlaXml = quick_xml::de::from_str(text).map_err(ParseError::from)?;

        Ok(InvoiceDocument {
            supplier: szamla.szallito.into(),
            info: szamla.alap.into(),
            buyer: szamla.vevo.into(),
            items: szamla.tetelek.tetel.into_iter().map(Into::into).collect(),
            financial_items: szamla.qutetek.qutet.into_iter().map(Into::into).collect(),
            labels: szamla.cimkek.cimke,
            totals: szamla.osszegek.into(),
            payments: szamla
                .kifizetesek
                .kifizetes
                .into_iter()
                .map(Into::into)
                .collect(),
            pdf: match szamla.pdf.filter(|content| !content.trim().is_empty()) {
                Some(encoded) => Some(Pdf::from_base64(&encoded)?),
                None => None,
            },
        })
    }
}

#[derive(Clone, Copy)]
enum ResponseRoot {
    AgentResponse,
    Invoice,
}

fn response_root(body: &[u8]) -> Result<ResponseRoot, ParseError> {
    use quick_xml::name::{Namespace, ResolveResult};

    let mut reader = quick_xml::reader::NsReader::from_reader(body);

    loop {
        let (namespace, event) = reader
            .read_resolved_event()
            .map_err(quick_xml::DeError::from)?;

        match event {
            quick_xml::events::Event::Start(start) | quick_xml::events::Event::Empty(start) => {
                let local = start.local_name();
                let local = local.as_ref();
                let (root, expected_namespace) = match local {
                    "xmlszamlavalasz" => (
                        ResponseRoot::AgentResponse,
                        "http://www.szamlazz.hu/xmlszamlavalasz",
                    ),
                    "szamla" => (ResponseRoot::Invoice, "http://www.szamlazz.hu/szamla"),
                    other => {
                        return Err(ParseError::UnexpectedBody(format!(
                            "unexpected XML query response root {other}"
                        )));
                    }
                };

                if namespace != ResolveResult::Bound(Namespace(expected_namespace)) {
                    return Err(ParseError::UnexpectedBody(format!(
                        "wrong namespace for XML query response root {local}"
                    )));
                }
                return Ok(root);
            }
            quick_xml::events::Event::Eof => {
                let text = String::from_utf8_lossy(body);
                let text = text.trim();
                return Err(ParseError::UnexpectedBody(if text.is_empty() {
                    "empty response".to_owned()
                } else {
                    text.to_owned()
                }));
            }
            _ => {}
        }
    }
}

// ----- wire structs ----------------------------------------------------------
//
// The `szamla` response document as it appears on the wire: Hungarian element
// names, list wrappers still in place, empty elements standing for absent
// values, and the PDF still base64. Each converts into its public counterpart.

#[derive(Debug, serde::Deserialize)]
struct SzamlaXml {
    szallito: SzallitoXml,
    alap: AlapXml,
    vevo: VevoXml,
    tetelek: TetelekXml,
    #[serde(default)]
    qutetek: QutetekXml,
    #[serde(default)]
    cimkek: CimkekXml,
    osszegek: OsszegekXml,
    #[serde(default)]
    kifizetesek: KifizetesekXml,
    #[serde(default)]
    pdf: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct CimXml {
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    orszag: Option<String>,
    irsz: String,
    telepules: String,
    cim: String,
}

impl From<CimXml> for Address {
    fn from(cim: CimXml) -> Self {
        Self {
            country: cim.orszag,
            zip: cim.irsz,
            city: cim.telepules,
            address: cim.cim,
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct BankXml {
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    nev: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    bankszamla: Option<String>,
}

impl From<BankXml> for Bank {
    fn from(bank: BankXml) -> Self {
        Self {
            name: bank.nev,
            account: bank.bankszamla,
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct SzallitoXml {
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    id: Option<u64>,
    nev: String,
    cim: CimXml,
    #[serde(default)]
    postacim: Option<CimXml>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    adoszam: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    csoportazonosito: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    adoszameu: Option<String>,
    #[serde(default)]
    bank: Option<BankXml>,
}

impl From<SzallitoXml> for Supplier {
    fn from(szallito: SzallitoXml) -> Self {
        Self {
            id: szallito.id,
            name: szallito.nev,
            address: szallito.cim.into(),
            postal_address: szallito.postacim.map(Into::into),
            tax_number: szallito.adoszam,
            group_id: szallito.csoportazonosito,
            eu_tax_number: szallito.adoszameu,
            bank: szallito.bank.map(Into::into),
        }
    }
}

// These booleans mirror independent protocol fields, not a single state.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, serde::Deserialize)]
struct AlapXml {
    id: u64,
    szamlaszam: InvoiceNumber,
    #[serde(
        rename(deserialize = "gazdEsemAzon"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    gazd_esem_azon: Option<u64>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    forras: Option<u32>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    iktatoszam: Option<String>,
    tipus: String,
    eszamla: InvoiceAppearance,
    #[serde(default, deserialize_with = "empty_invoice_number")]
    hivszamlaszam: Option<InvoiceNumber>,
    #[serde(default, deserialize_with = "empty_invoice_number")]
    hivdijbekszam: Option<InvoiceNumber>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    kelt: Option<Date>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    telj: Option<Date>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    fizh: Option<Date>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    fizmod: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    fizmodunified: Option<String>,
    #[serde(default, deserialize_with = "xml::de::flexible_bool")]
    keszpenz: bool,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    rendelesszam: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    nyelv: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    devizanem: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    devizabank: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    devizaarf: Option<Decimal>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    megjegyzes: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    afatipus: Option<String>,
    #[serde(default, deserialize_with = "xml::de::flexible_bool")]
    penzforg: bool,
    #[serde(default, deserialize_with = "xml::de::flexible_bool")]
    kata: bool,
    #[serde(default, deserialize_with = "xml::de::flexible_bool")]
    katafokonyv: bool,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    email: Option<String>,
    #[serde(default, deserialize_with = "xml::de::flexible_bool")]
    teszt: bool,
    #[serde(default, deserialize_with = "xml::de::optional_flexible_bool")]
    sztornozott: Option<bool>,
}

impl From<AlapXml> for InvoiceInfo {
    fn from(alap: AlapXml) -> Self {
        Self {
            id: alap.id,
            invoice_number: alap.szamlaszam,
            economic_event_id: alap.gazd_esem_azon,
            source: alap.forras,
            registration_number: alap.iktatoszam,
            document_type: alap.tipus,
            e_invoice: alap.eszamla,
            referenced_invoice_number: alap.hivszamlaszam,
            referenced_proforma_number: alap.hivdijbekszam,
            issue_date: alap.kelt,
            fulfillment_date: alap.telj,
            due_date: alap.fizh,
            payment_method: alap.fizmod,
            unified_payment_method: alap.fizmodunified,
            cash_payment: alap.keszpenz,
            order_number: alap.rendelesszam,
            language: alap.nyelv,
            currency: alap.devizanem,
            exchange_bank: alap.devizabank,
            exchange_rate: alap.devizaarf,
            comment: alap.megjegyzes,
            vat_type: alap.afatipus,
            cash_accounting: alap.penzforg,
            kata: alap.kata,
            kata_ledger: alap.katafokonyv,
            email: alap.email,
            test: alap.teszt,
            reversed: alap.sztornozott,
        }
    }
}

#[derive(Debug, Default, serde::Deserialize)]
struct PostacimXml {
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    nev: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    orszag: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    irsz: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    telepules: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    cim: Option<String>,
}

impl From<PostacimXml> for BuyerPostalAddress {
    fn from(postacim: PostacimXml) -> Self {
        Self {
            name: postacim.nev,
            country: postacim.orszag,
            zip: postacim.irsz,
            city: postacim.telepules,
            address: postacim.cim,
        }
    }
}

#[derive(Debug, Default, serde::Deserialize)]
struct VevoFokonyvXml {
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    vevo: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    vevoazon: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    datum: Option<Date>,
    #[serde(default, deserialize_with = "xml::de::optional_flexible_bool")]
    folyamatostelj: Option<bool>,
    #[serde(
        rename(deserialize = "elszDatTol"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    elsz_dat_tol: Option<Date>,
    #[serde(
        rename(deserialize = "elszDatIg"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    elsz_dat_ig: Option<Date>,
}

impl From<VevoFokonyvXml> for BuyerLedgerInfo {
    fn from(fokonyv: VevoFokonyvXml) -> Self {
        Self {
            account: fokonyv.vevo,
            buyer_id: fokonyv.vevoazon,
            date: fokonyv.datum,
            continuous_fulfillment: fokonyv.folyamatostelj,
            settlement_from: fokonyv.elsz_dat_tol,
            settlement_to: fokonyv.elsz_dat_ig,
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct VevoXml {
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    id: Option<u64>,
    nev: String,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    azonosito: Option<String>,
    #[serde(default)]
    cim: Option<CimXml>,
    #[serde(default)]
    postacim: Option<PostacimXml>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    email: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    adoszam: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    csoportazonosito: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    adoszameu: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    lokacio: Option<i64>,
    #[serde(
        rename(deserialize = "privatePersonIndicator"),
        default,
        deserialize_with = "xml::de::flexible_bool"
    )]
    private_person_indicator: bool,
    #[serde(default)]
    fokonyv: Option<VevoFokonyvXml>,
}

impl From<VevoXml> for BuyerInfo {
    fn from(vevo: VevoXml) -> Self {
        Self {
            id: vevo.id,
            name: vevo.nev,
            identifier: vevo.azonosito,
            address: vevo.cim.map(Into::into),
            postal_address: vevo.postacim.map(Into::into),
            email: vevo.email,
            tax_number: vevo.adoszam,
            group_id: vevo.csoportazonosito,
            eu_tax_number: vevo.adoszameu,
            location: vevo.lokacio,
            private_person: vevo.private_person_indicator,
            ledger: vevo.fokonyv.map(Into::into),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct TetelekXml {
    #[serde(default)]
    tetel: Vec<TetelXml>,
}

#[derive(Debug, serde::Deserialize)]
struct TetelXml {
    nev: String,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    azonosito: Option<String>,
    #[serde(deserialize_with = "xml::de::from_text")]
    mennyiseg: Decimal,
    mennyisegiegyseg: String,
    #[serde(deserialize_with = "xml::de::from_text")]
    nettoegysegar: Decimal,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    afatipus: Option<String>,
    afakulcs: String,
    #[serde(deserialize_with = "xml::de::from_text")]
    netto: Decimal,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    arresafaalap: Option<Decimal>,
    #[serde(deserialize_with = "xml::de::from_text")]
    afa: Decimal,
    #[serde(deserialize_with = "xml::de::from_text")]
    brutto: Decimal,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    megjegyzes: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    sztetordering: Option<u32>,
    #[serde(default)]
    fokonyv: Option<TetelFokonyvXml>,
}

impl From<TetelXml> for DocumentItem {
    fn from(tetel: TetelXml) -> Self {
        Self {
            name: tetel.nev,
            id: tetel.azonosito,
            quantity: tetel.mennyiseg,
            unit: tetel.mennyisegiegyseg,
            unit_price: tetel.nettoegysegar,
            vat_type: tetel.afatipus,
            vat_rate_code: tetel.afakulcs,
            net_value: tetel.netto,
            margin_vat_base: tetel.arresafaalap,
            vat_value: tetel.afa,
            gross_value: tetel.brutto,
            comment: tetel.megjegyzes,
            ordering: tetel.sztetordering,
            ledger: tetel.fokonyv.map(Into::into),
        }
    }
}

#[derive(Debug, Default, serde::Deserialize)]
struct TetelFokonyvXml {
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    arbevetel: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    afa: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    gazdasagiesemeny: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    gazdasagiesemenyafa: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    elszdattol: Option<Date>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    elszdatig: Option<Date>,
}

impl From<TetelFokonyvXml> for DocumentItemLedger {
    fn from(fokonyv: TetelFokonyvXml) -> Self {
        Self {
            revenue_account: fokonyv.arbevetel,
            vat_account: fokonyv.afa,
            economic_event: fokonyv.gazdasagiesemeny,
            vat_economic_event: fokonyv.gazdasagiesemenyafa,
            settlement_from: fokonyv.elszdattol,
            settlement_to: fokonyv.elszdatig,
        }
    }
}

#[derive(Debug, Default, serde::Deserialize)]
struct QutetekXml {
    #[serde(default)]
    qutet: Vec<QutetXml>,
}

#[derive(Debug, serde::Deserialize)]
struct QutetXml {
    nev: String,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    afatipus: Option<String>,
    afakulcs: String,
    #[serde(deserialize_with = "xml::de::from_text")]
    netto: Decimal,
    #[serde(deserialize_with = "xml::de::from_text")]
    afa: Decimal,
    #[serde(deserialize_with = "xml::de::from_text")]
    brutto: Decimal,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    elszdattol: Option<Date>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    elszdatig: Option<Date>,
    #[serde(deserialize_with = "xml::de::from_text")]
    afalevon: i32,
    #[serde(default)]
    cimkek: CimkekXml,
}

impl From<QutetXml> for FinancialItem {
    fn from(qutet: QutetXml) -> Self {
        Self {
            name: qutet.nev,
            vat_type: qutet.afatipus,
            vat_rate_code: qutet.afakulcs,
            net: qutet.netto,
            vat: qutet.afa,
            gross: qutet.brutto,
            settlement_from: qutet.elszdattol,
            settlement_to: qutet.elszdatig,
            deductible_vat: qutet.afalevon,
            labels: qutet.cimkek.cimke,
        }
    }
}

/// The `cimkek`/`cimke` label list, on the invoice and on financial items.
#[derive(Debug, Default, serde::Deserialize)]
struct CimkekXml {
    #[serde(default)]
    cimke: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct OsszegekXml {
    #[serde(default)]
    afakulcsossz: Vec<AfakulcsosszXml>,
    totalossz: TotalosszXml,
}

impl From<OsszegekXml> for Totals {
    fn from(osszegek: OsszegekXml) -> Self {
        Self {
            by_vat_rate: osszegek.afakulcsossz.into_iter().map(Into::into).collect(),
            total: osszegek.totalossz.into(),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct AfakulcsosszXml {
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    afatipus: Option<String>,
    afakulcs: String,
    #[serde(deserialize_with = "xml::de::from_text")]
    netto: Decimal,
    #[serde(deserialize_with = "xml::de::from_text")]
    afa: Decimal,
    #[serde(deserialize_with = "xml::de::from_text")]
    brutto: Decimal,
}

impl From<AfakulcsosszXml> for VatTotal {
    fn from(ossz: AfakulcsosszXml) -> Self {
        Self {
            vat_type: ossz.afatipus,
            vat_rate_code: ossz.afakulcs,
            net: ossz.netto,
            vat: ossz.afa,
            gross: ossz.brutto,
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct TotalosszXml {
    #[serde(deserialize_with = "xml::de::from_text")]
    netto: Decimal,
    #[serde(deserialize_with = "xml::de::from_text")]
    afa: Decimal,
    #[serde(deserialize_with = "xml::de::from_text")]
    brutto: Decimal,
}

impl From<TotalosszXml> for GrandTotal {
    fn from(total: TotalosszXml) -> Self {
        Self {
            net: total.netto,
            vat: total.afa,
            gross: total.brutto,
        }
    }
}

#[derive(Debug, Default, serde::Deserialize)]
struct KifizetesekXml {
    #[serde(default)]
    kifizetes: Vec<KifizetesXml>,
}

#[derive(Debug, serde::Deserialize)]
struct KifizetesXml {
    datum: Date,
    jogcim: String,
    #[serde(deserialize_with = "xml::de::from_text")]
    osszeg: Decimal,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    megjegyzes: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    bankszamlaszam: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    banktranzid: Option<u64>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    devizaarf: Option<Decimal>,
}

impl From<KifizetesXml> for RecordedPayment {
    fn from(kifizetes: KifizetesXml) -> Self {
        Self {
            date: kifizetes.datum,
            title: kifizetes.jogcim,
            amount: kifizetes.osszeg,
            comment: kifizetes.megjegyzes,
            bank_account: kifizetes.bankszamlaszam,
            bank_transaction_id: kifizetes.banktranzid,
            exchange_rate: kifizetes.devizaarf,
        }
    }
}

fn empty_invoice_number<'de, D>(deserializer: D) -> Result<Option<InvoiceNumber>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize as _;
    let value = String::deserialize(deserializer)?;
    Ok((!value.trim().is_empty()).then(|| InvoiceNumber::new(value)))
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use rust_decimal::dec;

    use super::*;

    fn sample() -> QueryInvoiceXml {
        QueryInvoiceXml {
            selector: InvoiceSelector::InvoiceNumber(InvoiceNumber::new("E-TST-2026-1")),
            include_pdf: false,
        }
    }

    #[test]
    fn writes_canonical_query_xml() {
        let xml = sample().write_xml(&Credentials::agent_key("key"));
        let expected = include_str!("../../tests/golden/xmlszamlaxml.xml").trim_end();
        assert_eq!(String::from_utf8(xml).expect("utf-8"), expected);
    }

    #[test]
    fn order_number_replaces_invoice_number() {
        let query = QueryInvoiceXml {
            selector: InvoiceSelector::OrderNumber("ORDER-123".into()),
            include_pdf: true,
        };
        let xml =
            String::from_utf8(query.write_xml(&Credentials::agent_key("key"))).expect("utf-8");
        assert!(xml.contains("<rendelesSzam>ORDER-123</rendelesSzam>"));
        assert!(xml.contains("<pdf>true</pdf>"));
        assert!(!xml.contains("<szamlaszam>"));
    }

    #[test]
    fn external_id_is_the_only_serialized_selector() {
        let query = QueryInvoiceXml::new(InvoiceSelector::ExternalId("EXT-42".into()));
        let xml =
            String::from_utf8(query.write_xml(&Credentials::agent_key("key"))).expect("utf-8");
        assert!(xml.contains("<szamlaKulsoAzon>EXT-42</szamlaKulsoAzon>"));
        assert!(!xml.contains("<szamlaszam>"));
        assert!(!xml.contains("<rendelesSzam>"));
    }

    #[test]
    fn parses_documented_xml_error_response() {
        let body = include_bytes!("../../tests/synthetic/xmlszamlavalasz_error.xml");
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let error = sample().parse(&response).expect_err("error");
        match error {
            ResponseError::Api(api) => assert_eq!(api.code, crate::ErrorCode::InvalidCredentials),
            other => panic!("expected api error, got {other:?}"),
        }
    }

    #[test]
    fn vat_type_takes_precedence_over_numeric_rate() {
        let body = "<szamla xmlns=\"http://www.szamlazz.hu/szamla\">\
             <szallito><nev>Seller</nev><cim><irsz>1</irsz><telepules>B</telepules><cim>C</cim></cim></szallito>\
             <alap><id>1</id><szamlaszam>X-1</szamlaszam><tipus>E</tipus><eszamla>0</eszamla></alap>\
             <vevo><nev>Buyer</nev></vevo><tetelek><tetel><nev>Item</nev><mennyiseg>1</mennyiseg>\
             <mennyisegiegyseg>db</mennyisegiegyseg><nettoegysegar>100</nettoegysegar><afatipus>AAM</afatipus>\
             <afakulcs>0</afakulcs><netto>100</netto><afa>0</afa><brutto>100</brutto></tetel></tetelek>\
             <osszegek><afakulcsossz><afatipus>AAM</afatipus><afakulcs>0</afakulcs><netto>100</netto><afa>0</afa><brutto>100</brutto></afakulcsossz>\
             <totalossz><netto>100</netto><afa>0</afa><brutto>100</brutto></totalossz></osszegek></szamla>";
        let response = RawResponse::new::<&str, &str>([], body.as_bytes().to_vec());
        let document = sample().parse(&response).expect("success");
        assert_eq!(document.items[0].vat_type.as_deref(), Some("AAM"));
        assert_eq!(document.items[0].vat_rate(), VatRate::Aam);
        assert_eq!(
            document.totals.by_vat_rate[0].vat_type.as_deref(),
            Some("AAM")
        );
        assert_eq!(document.totals.by_vat_rate[0].vat_rate(), VatRate::Aam);
    }

    #[test]
    fn parses_invoice_document() {
        let body = include_bytes!("../../tests/synthetic/szamla_query.xml");
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let document = sample().parse(&response).expect("success");

        assert_eq!(document.supplier.id, Some(42));
        assert_eq!(document.supplier.name, "Synthetic Supplier Kft.");
        assert_eq!(document.supplier.address.city, "Testvaros");
        assert!(document.supplier.postal_address.is_none());
        assert!(document.supplier.group_id.is_none());
        assert_eq!(
            document
                .supplier
                .bank
                .as_ref()
                .and_then(|bank| bank.name.as_deref()),
            Some("Test Bank")
        );
        assert_eq!(
            document
                .supplier
                .bank
                .as_ref()
                .and_then(|bank| bank.account.as_deref()),
            None
        );

        assert_eq!(document.info.invoice_number.as_str(), "E-TST-2026-66");
        assert_eq!(document.info.document_type, "D");
        assert_eq!(document.info.economic_event_id, None);
        assert_eq!(document.info.e_invoice, InvoiceAppearance::NotInvoice);
        assert_eq!(document.info.issue_date, Some(date(2026, 1, 9)));
        assert_eq!(document.info.payment_method.as_deref(), Some("credit_card"));
        assert_eq!(document.info.exchange_rate, Some(dec!(0)));
        assert_eq!(document.info.comment, None);
        assert!(!document.info.cash_accounting);
        assert!(document.info.kata);
        assert!(!document.info.test);
        assert_eq!(document.info.reversed, None);

        assert_eq!(document.buyer.name, "Synthetic Buyer");
        assert_eq!(
            document.buyer.address.as_ref().map(|a| a.city.as_str()),
            Some("Mintavaros")
        );
        assert_eq!(document.buyer.tax_number, None);

        assert_eq!(document.items.len(), 1);
        assert_eq!(document.items[0].name, "Synthetic service");
        assert_eq!(document.items[0].unit, "db");
        assert_eq!(document.items[0].vat_rate(), VatRate::percent(20));
        assert_eq!(document.items[0].unit_price, dec!(380));
        assert_eq!(document.items[0].gross_value, dec!(456));
        assert_eq!(
            document.items[0].comment.as_deref(),
            Some("Synthetic item comment")
        );
        assert!(document.items[0].ledger.is_some());
        assert!(document.financial_items.is_empty());
        assert!(document.labels.is_empty());

        assert_eq!(document.totals.by_vat_rate.len(), 1);
        assert_eq!(
            document.totals.by_vat_rate[0].vat_rate(),
            VatRate::percent(20)
        );
        assert_eq!(document.totals.by_vat_rate[0].net, dec!(464));
        assert_eq!(document.totals.total.vat, dec!(93));
        assert_eq!(document.totals.total.gross, dec!(557));

        assert_eq!(document.payments.len(), 1);
        assert_eq!(document.payments[0].date, date(2026, 1, 22));
        assert_eq!(document.payments[0].title, "transfer");
        assert_eq!(document.payments[0].amount, dec!(15));

        assert!(document.pdf.is_none());
    }

    /// A `szamla` response exercising every section and field the schema
    /// currently defines.
    const ALL_SECTIONS: &str = r#"<szamla xmlns="http://www.szamlazz.hu/szamla">
          <szallito><id>1</id><nev>Seller</nev><cim><irsz>1</irsz><telepules>B</telepules><cim>C</cim></cim>
            <postacim><orszag>HU</orszag><irsz>2</irsz><telepules>P</telepules><cim>Post</cim></postacim>
            <adoszam>12345678-2-42</adoszam><csoportazonosito>GROUP-S</csoportazonosito><adoszameu>HU123</adoszameu>
            <bank><nev>Test Bank</nev><bankszamla>1234-5678</bankszamla></bank></szallito>
          <alap><id>2</id><szamlaszam>INV-1</szamlaszam><gazdEsemAzon>9</gazdEsemAzon><forras>34</forras>
            <iktatoszam>REG-1</iktatoszam><tipus>E</tipus><eszamla>3</eszamla><hivszamlaszam>INV-0</hivszamlaszam>
            <hivdijbekszam>PRO-1</hivdijbekszam><kelt>2026-07-01</kelt><telj>2026-07-02</telj><fizh>2026-07-10</fizh>
            <fizmod>transfer</fizmod><fizmodunified>átutalás</fizmodunified><keszpenz>false</keszpenz>
            <rendelesszam>ORDER-1</rendelesszam><nyelv>hu</nyelv><devizanem>EUR</devizanem><devizabank>MNB</devizabank>
            <devizaarf>400</devizaarf><megjegyzes>Note</megjegyzes><afatipus>EUT</afatipus><penzforg>true</penzforg><kata>false</kata>
            <katafokonyv>true</katafokonyv><email>x@example.com</email><teszt>false</teszt><sztornozott>true</sztornozott></alap>
          <vevo><id>3</id><nev>Buyer</nev><azonosito>BUY-1</azonosito><cim><irsz>3</irsz><telepules>V</telepules><cim>Main</cim></cim>
            <postacim><nev>Receiver</nev><orszag>HU</orszag><irsz>4</irsz><telepules>Q</telepules><cim>Ship</cim></postacim>
            <email>b@example.com</email><adoszam>87654321-1-42</adoszam><csoportazonosito>GROUP-B</csoportazonosito>
            <adoszameu>HU876</adoszameu><lokacio>7</lokacio><privatePersonIndicator>true</privatePersonIndicator>
            <fokonyv><vevo>311</vevo><vevoazon>B-7</vevoazon><datum>2026-07-03</datum><folyamatostelj>true</folyamatostelj>
              <elszDatTol>2026-07-01</elszDatTol><elszDatIg>2026-07-31</elszDatIg></fokonyv></vevo>
          <tetelek><tetel><nev>Item</nev><azonosito>I-1</azonosito><mennyiseg>1</mennyiseg><mennyisegiegyseg>db</mennyisegiegyseg>
            <nettoegysegar>100</nettoegysegar><afatipus>AAM</afatipus><afakulcs>0</afakulcs><netto>100</netto><arresafaalap>5</arresafaalap><afa>0</afa><brutto>100</brutto>
            <megjegyzes>Row</megjegyzes><sztetordering>5</sztetordering><fokonyv><arbevetel>911</arbevetel><afa>467</afa><gazdasagiesemeny>SALE</gazdasagiesemeny>
              <gazdasagiesemenyafa>VAT</gazdasagiesemenyafa><elszdattol>2026-07-01</elszdattol><elszdatig>2026-07-31</elszdatig></fokonyv></tetel></tetelek>
          <qutetek><qutet><nev>Fee</nev><afatipus>TAM</afatipus><afakulcs>0</afakulcs><netto>10</netto><afa>0</afa><brutto>10</brutto>
            <elszdattol>2026-07-01</elszdattol><elszdatig>2026-07-31</elszdatig><afalevon>50</afalevon><cimkek><cimke>fee</cimke></cimkek></qutet></qutetek>
          <cimkek><cimke>invoice</cimke></cimkek>
          <osszegek><afakulcsossz><afatipus>AAM</afatipus><afakulcs>0</afakulcs><netto>110</netto><afa>0</afa><brutto>110</brutto></afakulcsossz>
            <totalossz><netto>110</netto><afa>0</afa><brutto>110</brutto></totalossz></osszegek>
          <kifizetesek><kifizetes><datum>2026-07-04</datum><jogcim>transfer</jogcim><osszeg>10</osszeg><megjegyzes>Paid</megjegyzes>
            <bankszamlaszam>ACC</bankszamlaszam><banktranzid>99</banktranzid><devizaarf>401</devizaarf></kifizetes></kifizetesek>
          <pdf>JVBERi0=</pdf></szamla>"#;

    #[test]
    #[allow(clippy::too_many_lines)]
    fn parses_all_current_invoice_response_sections() {
        let response = RawResponse::new::<&str, &str>([], ALL_SECTIONS.as_bytes().to_vec());
        let document = sample().parse(&response).expect("success");
        assert_eq!(
            document
                .supplier
                .postal_address
                .as_ref()
                .map(|a| a.city.as_str()),
            Some("P")
        );
        assert_eq!(document.supplier.group_id.as_deref(), Some("GROUP-S"));
        assert_eq!(document.info.economic_event_id, Some(9));
        assert_eq!(document.info.source, Some(34));
        assert_eq!(document.info.registration_number.as_deref(), Some("REG-1"));
        assert_eq!(
            document
                .info
                .referenced_invoice_number
                .as_ref()
                .map(InvoiceNumber::as_str),
            Some("INV-0")
        );
        assert_eq!(
            document
                .info
                .referenced_proforma_number
                .as_ref()
                .map(InvoiceNumber::as_str),
            Some("PRO-1")
        );
        assert!(!document.info.cash_payment);
        assert_eq!(document.info.order_number.as_deref(), Some("ORDER-1"));
        assert_eq!(document.info.exchange_bank.as_deref(), Some("MNB"));
        assert_eq!(document.info.vat_type.as_deref(), Some("EUT"));
        assert!(document.info.kata_ledger);
        assert_eq!(document.info.reversed, Some(true));
        assert_eq!(document.buyer.identifier.as_deref(), Some("BUY-1"));
        assert_eq!(
            document
                .buyer
                .postal_address
                .as_ref()
                .and_then(|a| a.name.as_deref()),
            Some("Receiver")
        );
        assert_eq!(document.buyer.group_id.as_deref(), Some("GROUP-B"));
        assert_eq!(document.buyer.eu_tax_number.as_deref(), Some("HU876"));
        assert_eq!(document.buyer.location, Some(7));
        assert!(document.buyer.private_person);
        assert_eq!(
            document
                .buyer
                .ledger
                .as_ref()
                .and_then(|l| l.account.as_deref()),
            Some("311")
        );
        assert_eq!(document.items[0].id.as_deref(), Some("I-1"));
        assert_eq!(document.items[0].ordering, Some(5));
        assert_eq!(
            document.items[0]
                .ledger
                .as_ref()
                .and_then(|l| l.revenue_account.as_deref()),
            Some("911")
        );
        assert_eq!(document.financial_items[0].vat_rate(), VatRate::Tam);
        assert_eq!(document.financial_items[0].labels, ["fee"]);
        assert_eq!(document.labels, ["invoice"]);
        assert_eq!(document.payments[0].bank_transaction_id, Some(99));
        assert_eq!(document.payments[0].exchange_rate, Some(dec!(401)));
        let json = serde_json::to_value(&document).expect("serialize");
        assert_eq!(json["financial_items"][0]["labels"][0], "fee");
        assert_eq!(json["labels"][0], "invoice");
    }

    /// The fetched document is journal-safe: it serialises under its Rust
    /// field names and deserialises back to the same value.
    #[test]
    fn invoice_document_round_trips_through_json() {
        let response = RawResponse::new::<&str, &str>([], ALL_SECTIONS.as_bytes().to_vec());
        let document = sample().parse(&response).expect("success");
        assert!(document.pdf.is_some(), "the fixture carries a PDF");

        let json = serde_json::to_value(&document).expect("serialize");
        assert_eq!(json["info"]["invoice_number"], "INV-1");
        assert_eq!(json["info"]["e_invoice"], "3");
        assert_eq!(json["supplier"]["address"]["city"], "B");
        assert_eq!(json["items"][0]["vat_rate_code"], "0");
        assert_eq!(json["totals"]["total"]["gross"], "110");
        assert_eq!(json["pdf"], "JVBERi0=");
        assert!(json["info"].get("szamlaszam").is_none(), "no wire names");

        let restored: InvoiceDocument = serde_json::from_value(json).expect("deserialize");
        assert_eq!(restored, document);
    }

    #[test]
    fn parses_all_xml_schema_boolean_forms_in_buyer_ledger() {
        for (value, expected) in [("true", true), ("1", true), ("false", false), ("0", false)] {
            let body = format!(
                "<szamla xmlns=\"http://www.szamlazz.hu/szamla\">\
                 <szallito><nev>Seller</nev><cim><irsz>1</irsz><telepules>B</telepules><cim>C</cim></cim></szallito>\
                 <alap><id>1</id><szamlaszam>X-1</szamlaszam><tipus>E</tipus><eszamla>0</eszamla></alap>\
                 <vevo><nev>Buyer</nev><fokonyv><folyamatostelj>{value}</folyamatostelj></fokonyv></vevo>\
                 <tetelek></tetelek><osszegek><totalossz><netto>0</netto><afa>0</afa><brutto>0</brutto></totalossz></osszegek>\
                 </szamla>"
            );
            let response = RawResponse::new::<&str, &str>([], body.into_bytes());
            let document = sample().parse(&response).expect("success");
            assert_eq!(
                document
                    .buyer
                    .ledger
                    .and_then(|ledger| ledger.continuous_fulfillment),
                Some(expected),
                "value {value}"
            );
        }
    }

    #[test]
    fn rejects_wrong_response_namespace_and_invalid_utf8() {
        let wrong_namespace = RawResponse::new::<&str, &str>(
            [],
            br#"<szamla xmlns="https://wrong.example"/>"#.to_vec(),
        );
        assert!(sample().parse(&wrong_namespace).is_err());

        let invalid_utf8 = RawResponse::new::<&str, &str>(
            [],
            b"<szamla xmlns=\"http://www.szamlazz.hu/szamla\">\xff</szamla>".to_vec(),
        );
        assert!(sample().parse(&invalid_utf8).is_err());
    }

    #[test]
    fn preserves_invoice_appearance_codes() {
        let body = "<szamla xmlns=\"http://www.szamlazz.hu/szamla\">\
             <szallito><nev>Seller</nev>\
             <cim><irsz>1111</irsz><telepules>Budapest</telepules><cim>Fő u. 1.</cim></cim>\
             </szallito>\
             <alap><id>1</id><szamlaszam>E-TST-2026-1</szamlaszam><tipus>SZ</tipus><eszamla>3</eszamla></alap>\
             <vevo><nev>Buyer</nev></vevo>\
             <tetelek></tetelek>\
             <osszegek><totalossz><netto>0</netto><afa>0</afa><brutto>0</brutto></totalossz></osszegek>\
             </szamla>";
        let response = RawResponse::new::<&str, &str>([], body.as_bytes().to_vec());
        let document = sample().parse(&response).expect("success");
        assert_eq!(document.info.e_invoice, InvoiceAppearance::Electronic(3));
        assert!(document.info.e_invoice.is_e_invoice());

        let paper = body.replace("<eszamla>3</eszamla>", "<eszamla>1</eszamla>");
        let response = RawResponse::new::<&str, &str>([], paper.into_bytes());
        let document = sample().parse(&response).expect("success");
        assert_eq!(document.info.e_invoice, InvoiceAppearance::Paper);
        assert!(!document.info.e_invoice.is_e_invoice());
    }

    #[test]
    fn invoice_appearance_round_trips_as_json_code() {
        let appearance = InvoiceAppearance::Electronic(2);
        let json = serde_json::to_string(&appearance).expect("serialize");
        assert_eq!(json, "\"2\"");
        assert_eq!(
            serde_json::from_str::<InvoiceAppearance>(&json).expect("deserialize"),
            appearance
        );
    }

    /// `<sztornozott>` is absent on a live invoice and on the storno invoice
    /// itself, and appears as `true` on the original once it is reversed.
    #[test]
    fn reversed_marker_mirrors_the_wire() {
        let live = "<szamla xmlns=\"http://www.szamlazz.hu/szamla\">\
             <szallito><nev>Seller</nev>\
             <cim><irsz>1111</irsz><telepules>Budapest</telepules><cim>Fő u. 1.</cim></cim>\
             </szallito>\
             <alap><id>924307338</id><szamlaszam>CTEST-2026-40</szamlaszam>\
             <gazdEsemAzon>924307338</gazdEsemAzon><tipus>SZ</tipus><eszamla>1</eszamla>\
             <teszt>true</teszt></alap>\
             <vevo><nev>Buyer</nev></vevo>\
             <tetelek></tetelek>\
             <osszegek><totalossz><netto>1000</netto><afa>270</afa><brutto>1270</brutto></totalossz></osszegek>\
             </szamla>";
        let response = RawResponse::new::<&str, &str>([], live.as_bytes().to_vec());
        let document = sample().parse(&response).expect("success");
        assert_eq!(document.info.reversed, None);

        let reversed = live.replace(
            "<teszt>true</teszt>",
            "<teszt>true</teszt><sztornozott>true</sztornozott>",
        );
        let response = RawResponse::new::<&str, &str>([], reversed.into_bytes());
        let document = sample().parse(&response).expect("success");
        assert_eq!(document.info.reversed, Some(true));

        let storno = live
            .replace(
                "<szamlaszam>CTEST-2026-40</szamlaszam>",
                "<szamlaszam>CTEST-2026-42</szamlaszam>",
            )
            .replace(
                "<tipus>SZ</tipus>",
                "<tipus>SS</tipus><hivszamlaszam>CTEST-2026-40</hivszamlaszam>",
            );
        let response = RawResponse::new::<&str, &str>([], storno.into_bytes());
        let document = sample().parse(&response).expect("success");
        assert_eq!(document.info.document_type, "SS");
        assert_eq!(document.info.reversed, None);
        assert_eq!(
            document
                .info
                .referenced_invoice_number
                .as_ref()
                .map(InvoiceNumber::as_str),
            Some("CTEST-2026-40")
        );

        for (value, expected) in [("1", Some(true)), ("false", Some(false)), ("", None)] {
            let body = live.replace(
                "<teszt>true</teszt>",
                &format!("<teszt>true</teszt><sztornozott>{value}</sztornozott>"),
            );
            let response = RawResponse::new::<&str, &str>([], body.into_bytes());
            let document = sample().parse(&response).expect("success");
            assert_eq!(document.info.reversed, expected, "value {value:?}");
        }
    }

    /// The XML query reports an unknown number, order number, or external
    /// identifier as code 7 in the body only — no `szlahu_error_code` header.
    #[test]
    fn body_only_missing_data_error_is_typed() {
        let body = r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod><![CDATA[7]]></hibakod><hibauzenet><![CDATA[Hiányzó adat: számla xml (ismeretlen számlaszám, rendelésszám vagy külső azonosító).]]></hibauzenet></xmlszamlavalasz>"#;
        let response = RawResponse::new::<&str, &str>([], body.as_bytes().to_vec());
        let error = sample().parse(&response).expect_err("error");
        match error {
            ResponseError::Api(api) => {
                assert_eq!(api.code, crate::ErrorCode::MissingData);
                assert!(api.message.starts_with("Hiányzó adat"));
            }
            other => panic!("expected api error, got {other:?}"),
        }
    }

    #[test]
    fn decodes_embedded_pdf() {
        let body = "<szamla xmlns=\"http://www.szamlazz.hu/szamla\">\
             <szallito><nev>Seller</nev>\
             <cim><irsz>1111</irsz><telepules>Budapest</telepules><cim>Fő u. 1.</cim></cim>\
             </szallito>\
             <alap><id>1</id><szamlaszam>E-TST-2026-1</szamlaszam><tipus>E</tipus><eszamla>1</eszamla></alap>\
             <vevo><nev>Buyer</nev></vevo>\
             <tetelek></tetelek>\
             <osszegek><totalossz><netto>0</netto><afa>0</afa><brutto>0</brutto></totalossz></osszegek>\
             <pdf>JVBERi0=</pdf>\
             </szamla>";
        let response = RawResponse::new::<&str, &str>([], body.as_bytes().to_vec());
        let document = sample().parse(&response).expect("success");
        assert_eq!(document.info.e_invoice, InvoiceAppearance::Paper);
        assert!(document.items.is_empty());
        assert!(document.payments.is_empty());
        assert_eq!(document.pdf.expect("pdf").as_bytes(), b"%PDF-");
    }
}
