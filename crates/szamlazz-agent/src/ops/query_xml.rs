//! Invoice XML query (`xmlszamlaxml`): fetch the full data of a previously
//! issued invoice, optionally with its PDF.

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
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct Address {
    /// Country (`orszag`).
    #[serde(
        rename(deserialize = "orszag"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub country: Option<String>,
    /// ZIP code (`irsz`).
    #[serde(rename(deserialize = "irsz"))]
    pub zip: String,
    /// City (`telepules`).
    #[serde(rename(deserialize = "telepules"))]
    pub city: String,
    /// Street address (`cim`).
    #[serde(rename(deserialize = "cim"))]
    pub address: String,
}

/// Bank details of the supplier (`bank`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct Bank {
    /// Bank name (`nev`).
    #[serde(
        rename(deserialize = "nev"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub name: Option<String>,
    /// Bank account number (`bankszamla`).
    #[serde(
        rename(deserialize = "bankszamla"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub account: Option<String>,
}

/// The issuing party (`szallito`) as recorded on the invoice.
#[doc(alias = "szállító")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct Supplier {
    /// Internal szamlazz.hu identifier (`id`).
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    pub id: Option<u64>,
    /// Name (`nev`).
    #[serde(rename(deserialize = "nev"))]
    pub name: String,
    /// Billing address (`cim`).
    #[serde(rename(deserialize = "cim"))]
    pub address: Address,
    /// Postal address (`postacim`).
    #[serde(rename(deserialize = "postacim"), default)]
    pub postal_address: Option<Address>,
    /// Hungarian tax number (`adoszam`).
    #[doc(alias = "adószám")]
    #[serde(
        rename(deserialize = "adoszam"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub tax_number: Option<String>,
    /// VAT-group identifier (`csoportazonosito`).
    #[serde(
        rename(deserialize = "csoportazonosito"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub group_id: Option<String>,
    /// EU tax number (`adoszameu`).
    #[serde(
        rename(deserialize = "adoszameu"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub eu_tax_number: Option<String>,
    /// Bank details (`bank`).
    #[serde(rename(deserialize = "bank"), default)]
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
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct InvoiceInfo {
    /// Internal szamlazz.hu identifier (`id`).
    pub id: u64,
    /// The invoice number (`szamlaszam`).
    #[serde(rename(deserialize = "szamlaszam"))]
    pub invoice_number: InvoiceNumber,
    /// Economic-event identifier (`gazdEsemAzon`).
    #[serde(
        rename(deserialize = "gazdEsemAzon"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub economic_event_id: Option<u64>,
    /// Source system code (`forras`) for externally issued invoices.
    #[serde(
        rename(deserialize = "forras"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub source: Option<u32>,
    /// Registration number (`iktatoszam`).
    #[serde(
        rename(deserialize = "iktatoszam"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub registration_number: Option<String>,
    /// Document type code (`tipus`), e.g. `SZ` for an invoice or `D` for a
    /// proforma; kept verbatim as the code set is not documented exhaustively.
    #[serde(rename(deserialize = "tipus"))]
    pub document_type: String,
    /// Document appearance (`eszamla`): not an invoice, paper, or electronic.
    #[doc(alias = "e-számla")]
    #[serde(rename(deserialize = "eszamla"))]
    pub e_invoice: InvoiceAppearance,
    /// Referenced invoice number (`hivszamlaszam`).
    #[serde(
        rename(deserialize = "hivszamlaszam"),
        default,
        deserialize_with = "empty_invoice_number"
    )]
    pub referenced_invoice_number: Option<InvoiceNumber>,
    /// Referenced proforma number (`hivdijbekszam`).
    #[serde(
        rename(deserialize = "hivdijbekszam"),
        default,
        deserialize_with = "empty_invoice_number"
    )]
    pub referenced_proforma_number: Option<InvoiceNumber>,
    /// Issue date (`kelt`).
    #[serde(
        rename(deserialize = "kelt"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub issue_date: Option<Date>,
    /// Fulfillment date (`telj`).
    #[doc(alias = "teljesítés dátum")]
    #[serde(
        rename(deserialize = "telj"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub fulfillment_date: Option<Date>,
    /// Payment due date (`fizh`).
    #[doc(alias = "fizetési határidő")]
    #[serde(
        rename(deserialize = "fizh"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub due_date: Option<Date>,
    /// Payment method as recorded (`fizmod`), free text.
    #[serde(
        rename(deserialize = "fizmod"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub payment_method: Option<String>,
    /// Payment method normalized to szamlazz.hu's unified set
    /// (`fizmodunified`).
    #[serde(
        rename(deserialize = "fizmodunified"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub unified_payment_method: Option<String>,
    /// Whether the payment method is cash (`keszpenz`).
    #[serde(
        rename(deserialize = "keszpenz"),
        default,
        deserialize_with = "xml::de::flexible_bool"
    )]
    pub cash_payment: bool,
    /// Order number (`rendelesszam`).
    #[serde(
        rename(deserialize = "rendelesszam"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub order_number: Option<String>,
    /// Document language (`nyelv`).
    #[serde(
        rename(deserialize = "nyelv"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub language: Option<String>,
    /// Currency (`devizanem`).
    #[doc(alias = "pénznem")]
    #[serde(
        rename(deserialize = "devizanem"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub currency: Option<String>,
    /// Foreign-currency quoting bank (`devizabank`).
    #[serde(
        rename(deserialize = "devizabank"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub exchange_bank: Option<String>,
    /// Exchange rate (`devizaarf`).
    #[doc(alias = "árfolyam")]
    #[serde(
        rename(deserialize = "devizaarf"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub exchange_rate: Option<Decimal>,
    /// Comment shown on the document (`megjegyzes`).
    #[serde(
        rename(deserialize = "megjegyzes"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub comment: Option<String>,
    /// Invoice-level VAT category (`afatipus`).
    #[serde(
        rename(deserialize = "afatipus"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub vat_type: Option<String>,
    /// Issued under cash accounting (`penzforg`).
    #[doc(alias = "pénzforgalmi elszámolás")]
    #[serde(
        rename(deserialize = "penzforg"),
        default,
        deserialize_with = "xml::de::flexible_bool"
    )]
    pub cash_accounting: bool,
    /// Issued under KATA taxation (`kata`).
    #[serde(default, deserialize_with = "xml::de::flexible_bool")]
    pub kata: bool,
    /// Whether KATA ledger handling applies (`katafokonyv`).
    #[serde(
        rename(deserialize = "katafokonyv"),
        default,
        deserialize_with = "xml::de::flexible_bool"
    )]
    pub kata_ledger: bool,
    /// Buyer email the document was sent to (`email`).
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    pub email: Option<String>,
    /// Issued from a test account (`teszt`).
    #[serde(
        rename(deserialize = "teszt"),
        default,
        deserialize_with = "xml::de::flexible_bool"
    )]
    pub test: bool,
    /// Whether the invoice has been reversed (`sztornozott`).
    #[serde(
        rename(deserialize = "sztornozott"),
        default,
        deserialize_with = "xml::de::flexible_bool"
    )]
    pub reversed: bool,
}

/// Postal address returned for a buyer (`postacim`); every component is
/// optional in the response schema.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct BuyerPostalAddress {
    /// Recipient name (`nev`).
    #[serde(
        rename(deserialize = "nev"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub name: Option<String>,
    /// Country (`orszag`).
    #[serde(
        rename(deserialize = "orszag"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub country: Option<String>,
    /// ZIP code (`irsz`).
    #[serde(
        rename(deserialize = "irsz"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub zip: Option<String>,
    /// City (`telepules`).
    #[serde(
        rename(deserialize = "telepules"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub city: Option<String>,
    /// Street address (`cim`).
    #[serde(
        rename(deserialize = "cim"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub address: Option<String>,
}

/// Buyer ledger data returned under `vevo/fokonyv`.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct BuyerLedgerInfo {
    /// Buyer general-ledger account (`vevo`).
    #[serde(
        rename(deserialize = "vevo"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub account: Option<String>,
    /// Buyer identifier (`vevoazon`).
    #[serde(
        rename(deserialize = "vevoazon"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub buyer_id: Option<String>,
    /// Accounting date (`datum`).
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    pub date: Option<Date>,
    /// Continuous fulfillment (`folyamatostelj`).
    #[serde(
        rename(deserialize = "folyamatostelj"),
        default,
        deserialize_with = "xml::de::optional_flexible_bool"
    )]
    pub continuous_fulfillment: Option<bool>,
    /// Settlement period start (`elszDatTol`).
    #[serde(
        rename(deserialize = "elszDatTol"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub settlement_from: Option<Date>,
    /// Settlement period end (`elszDatIg`).
    #[serde(
        rename(deserialize = "elszDatIg"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub settlement_to: Option<Date>,
}

/// The buyer (`vevo`) as recorded on the invoice.
#[doc(alias = "vevő")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct BuyerInfo {
    /// Internal szamlazz.hu identifier (`id`).
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    pub id: Option<u64>,
    /// Name (`nev`).
    #[serde(rename(deserialize = "nev"))]
    pub name: String,
    /// Partner identifier (`azonosito`).
    #[serde(
        rename(deserialize = "azonosito"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub identifier: Option<String>,
    /// Billing address (`cim`).
    #[serde(rename(deserialize = "cim"), default)]
    pub address: Option<Address>,
    /// Postal address (`postacim`).
    #[serde(rename(deserialize = "postacim"), default)]
    pub postal_address: Option<BuyerPostalAddress>,
    /// Email address (`email`).
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    pub email: Option<String>,
    /// Hungarian tax number (`adoszam`).
    #[doc(alias = "adószám")]
    #[serde(
        rename(deserialize = "adoszam"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub tax_number: Option<String>,
    /// VAT-group identifier (`csoportazonosito`).
    #[serde(
        rename(deserialize = "csoportazonosito"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub group_id: Option<String>,
    /// EU tax number (`adoszameu`).
    #[serde(
        rename(deserialize = "adoszameu"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub eu_tax_number: Option<String>,
    /// Buyer-location classification (`lokacio`): `1` domestic, `2` EU, `3`
    /// outside the EU, or `-1` unknown.
    #[serde(
        rename(deserialize = "lokacio"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub location: Option<i64>,
    /// NAV private-person indicator (`privatePersonIndicator`).
    #[serde(
        rename(deserialize = "privatePersonIndicator"),
        default,
        deserialize_with = "xml::de::flexible_bool"
    )]
    pub private_person: bool,
    /// Buyer ledger metadata (`fokonyv`).
    #[serde(rename(deserialize = "fokonyv"), default)]
    pub ledger: Option<BuyerLedgerInfo>,
}

/// One row of the fetched invoice (`tetel`).
#[doc(alias = "tétel")]
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct DocumentItem {
    /// Item name (`nev`).
    #[serde(rename(deserialize = "nev"))]
    pub name: String,
    /// Item identifier (`azonosito`).
    #[serde(
        rename(deserialize = "azonosito"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub id: Option<String>,
    /// Quantity (`mennyiseg`).
    #[serde(
        rename(deserialize = "mennyiseg"),
        deserialize_with = "xml::de::from_text"
    )]
    pub quantity: Decimal,
    /// Unit of measure (`mennyisegiegyseg`).
    #[serde(rename(deserialize = "mennyisegiegyseg"))]
    pub unit: String,
    /// Net unit price (`nettoegysegar`).
    #[serde(
        rename(deserialize = "nettoegysegar"),
        deserialize_with = "xml::de::from_text"
    )]
    pub unit_price: Decimal,
    /// VAT category (`afatipus`), when the row uses a special VAT code.
    #[doc(alias = "áfatípus")]
    #[serde(
        rename(deserialize = "afatipus"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub vat_type: Option<String>,
    /// Numeric VAT rate wire token (`afakulcs`); see
    /// [`DocumentItem::vat_rate`].
    #[doc(alias = "áfakulcs")]
    #[serde(rename(deserialize = "afakulcs"))]
    pub vat_rate_code: String,
    /// Net value (`netto`).
    #[serde(rename(deserialize = "netto"), deserialize_with = "xml::de::from_text")]
    pub net_value: Decimal,
    /// Margin-scheme VAT base (`arresafaalap`).
    #[serde(
        rename(deserialize = "arresafaalap"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub margin_vat_base: Option<Decimal>,
    /// VAT value (`afa`).
    #[serde(rename(deserialize = "afa"), deserialize_with = "xml::de::from_text")]
    pub vat_value: Decimal,
    /// Gross value (`brutto`).
    #[serde(
        rename(deserialize = "brutto"),
        deserialize_with = "xml::de::from_text"
    )]
    pub gross_value: Decimal,
    /// Row comment (`megjegyzes`).
    #[serde(
        rename(deserialize = "megjegyzes"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub comment: Option<String>,
    /// Stable item ordering (`sztetordering`).
    #[serde(
        rename(deserialize = "sztetordering"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub ordering: Option<u32>,
    /// Item ledger metadata (`fokonyv`).
    #[serde(rename(deserialize = "fokonyv"), default)]
    pub ledger: Option<DocumentItemLedger>,
}

/// Item ledger data returned under `tetel/fokonyv`.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct DocumentItemLedger {
    /// Revenue account (`arbevetel`).
    #[serde(
        rename(deserialize = "arbevetel"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub revenue_account: Option<String>,
    /// VAT account (`afa`).
    #[serde(
        rename(deserialize = "afa"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub vat_account: Option<String>,
    /// Economic event (`gazdasagiesemeny`).
    #[serde(
        rename(deserialize = "gazdasagiesemeny"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub economic_event: Option<String>,
    /// VAT economic event (`gazdasagiesemenyafa`).
    #[serde(
        rename(deserialize = "gazdasagiesemenyafa"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub vat_economic_event: Option<String>,
    /// Settlement period start (`elszdattol`).
    #[serde(
        rename(deserialize = "elszdattol"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub settlement_from: Option<Date>,
    /// Settlement period end (`elszdatig`).
    #[serde(
        rename(deserialize = "elszdatig"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
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
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct Totals {
    /// Per-VAT-rate subtotals (`afakulcsossz`).
    #[serde(rename(deserialize = "afakulcsossz"), default)]
    pub by_vat_rate: Vec<VatTotal>,
    /// Grand total (`totalossz`).
    #[serde(rename(deserialize = "totalossz"))]
    pub total: GrandTotal,
}

/// Subtotal for one VAT rate (`afakulcsossz`).
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct VatTotal {
    /// VAT category (`afatipus`), when this subtotal uses a special VAT code.
    #[doc(alias = "áfatípus")]
    #[serde(
        rename(deserialize = "afatipus"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub vat_type: Option<String>,
    /// Numeric VAT rate wire token (`afakulcs`); see [`VatTotal::vat_rate`].
    #[doc(alias = "áfakulcs")]
    #[serde(rename(deserialize = "afakulcs"))]
    pub vat_rate_code: String,
    /// Net subtotal (`netto`).
    #[serde(rename(deserialize = "netto"), deserialize_with = "xml::de::from_text")]
    pub net: Decimal,
    /// VAT subtotal (`afa`).
    #[serde(rename(deserialize = "afa"), deserialize_with = "xml::de::from_text")]
    pub vat: Decimal,
    /// Gross subtotal (`brutto`).
    #[serde(
        rename(deserialize = "brutto"),
        deserialize_with = "xml::de::from_text"
    )]
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
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct FinancialItem {
    /// Name (`nev`).
    #[serde(rename(deserialize = "nev"))]
    pub name: String,
    /// VAT category (`afatipus`).
    #[serde(
        rename(deserialize = "afatipus"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub vat_type: Option<String>,
    /// Numeric VAT rate (`afakulcs`).
    #[serde(rename(deserialize = "afakulcs"))]
    pub vat_rate_code: String,
    /// Net amount (`netto`).
    #[serde(rename(deserialize = "netto"), deserialize_with = "xml::de::from_text")]
    pub net: Decimal,
    /// VAT amount (`afa`).
    #[serde(rename(deserialize = "afa"), deserialize_with = "xml::de::from_text")]
    pub vat: Decimal,
    /// Gross amount (`brutto`).
    #[serde(
        rename(deserialize = "brutto"),
        deserialize_with = "xml::de::from_text"
    )]
    pub gross: Decimal,
    /// Settlement period start (`elszdattol`).
    #[serde(
        rename(deserialize = "elszdattol"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub settlement_from: Option<Date>,
    /// Settlement period end (`elszdatig`).
    #[serde(
        rename(deserialize = "elszdatig"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub settlement_to: Option<Date>,
    /// Deductible VAT percentage (`afalevon`).
    #[serde(
        rename(deserialize = "afalevon"),
        deserialize_with = "xml::de::from_text"
    )]
    pub deductible_vat: i32,
    /// Labels (`cimkek`).
    #[serde(
        rename(deserialize = "cimkek"),
        default,
        deserialize_with = "deserialize_labels"
    )]
    pub labels: Vec<String>,
}

impl FinancialItem {
    /// The VAT type (`afatipus`) when present, otherwise `afakulcs`.
    #[must_use]
    pub fn vat_rate(&self) -> VatRate {
        VatRate::from(self.vat_type.as_deref().unwrap_or(&self.vat_rate_code))
    }
}

#[derive(Debug, Default, serde::Deserialize)]
struct Labels {
    #[serde(rename(deserialize = "cimke"), default)]
    values: Vec<String>,
}

fn deserialize_labels<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<String>, D::Error> {
    <Labels as serde::Deserialize>::deserialize(deserializer).map(|labels| labels.values)
}

/// The invoice grand total (`totalossz`).
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct GrandTotal {
    /// Net total (`netto`).
    #[serde(rename(deserialize = "netto"), deserialize_with = "xml::de::from_text")]
    pub net: Decimal,
    /// VAT total (`afa`).
    #[serde(rename(deserialize = "afa"), deserialize_with = "xml::de::from_text")]
    pub vat: Decimal,
    /// Gross total (`brutto`).
    #[serde(
        rename(deserialize = "brutto"),
        deserialize_with = "xml::de::from_text"
    )]
    pub gross: Decimal,
}

/// A payment recorded against the invoice (`kifizetes`).
#[doc(alias = "kifizetés")]
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct RecordedPayment {
    /// Payment date (`datum`).
    #[serde(rename(deserialize = "datum"))]
    pub date: Date,
    /// Payment title (`jogcim`), e.g. `transfer`.
    #[doc(alias = "jogcím")]
    #[serde(rename(deserialize = "jogcim"))]
    pub title: String,
    /// Amount (`osszeg`).
    #[serde(
        rename(deserialize = "osszeg"),
        deserialize_with = "xml::de::from_text"
    )]
    pub amount: Decimal,
    /// Comment (`megjegyzes`).
    #[serde(
        rename(deserialize = "megjegyzes"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub comment: Option<String>,
    /// Bank account the payment arrived on (`bankszamlaszam`).
    #[serde(
        rename(deserialize = "bankszamlaszam"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub bank_account: Option<String>,
    /// Bank transaction identifier (`banktranzid`).
    #[serde(
        rename(deserialize = "banktranzid"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
    pub bank_transaction_id: Option<u64>,
    /// Exchange rate used for the payment (`devizaarf`).
    #[serde(
        rename(deserialize = "devizaarf"),
        default,
        deserialize_with = "xml::de::empty_as_none"
    )]
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
        let document: SzamlaDocument = quick_xml::de::from_str(text).map_err(ParseError::from)?;

        Ok(InvoiceDocument {
            supplier: document.szallito,
            info: document.alap,
            buyer: document.vevo,
            items: document.tetelek.tetel,
            financial_items: document.qutetek.unwrap_or_default().qutet,
            labels: document.cimkek.unwrap_or_default().values,
            totals: document.osszegek,
            payments: document.kifizetesek.unwrap_or_default().kifizetes,
            pdf: match document.pdf.filter(|content| !content.trim().is_empty()) {
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

/// The `szamla` response document as it appears on the wire: list wrappers
/// still in place and the PDF still base64.
#[derive(Debug, serde::Deserialize)]
struct SzamlaDocument {
    szallito: Supplier,
    alap: InvoiceInfo,
    vevo: BuyerInfo,
    tetelek: Items,
    #[serde(default)]
    qutetek: Option<FinancialItems>,
    #[serde(default)]
    cimkek: Option<Labels>,
    osszegek: Totals,
    #[serde(default)]
    kifizetesek: Option<Payments>,
    #[serde(default)]
    pdf: Option<String>,
}

/// Wrapper for the `tetelek`/`tetel` list.
#[derive(Debug, serde::Deserialize)]
struct Items {
    #[serde(default)]
    tetel: Vec<DocumentItem>,
}

/// Wrapper for the `qutetek`/`qutet` list.
#[derive(Debug, Default, serde::Deserialize)]
struct FinancialItems {
    #[serde(default)]
    qutet: Vec<FinancialItem>,
}

/// Wrapper for the `kifizetesek`/`kifizetes` list.
#[derive(Debug, Default, serde::Deserialize)]
struct Payments {
    #[serde(default)]
    kifizetes: Vec<RecordedPayment>,
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

    #[test]
    #[allow(clippy::too_many_lines)]
    fn parses_all_current_invoice_response_sections() {
        let body = r#"<szamla xmlns="http://www.szamlazz.hu/szamla">
          <szallito><id>1</id><nev>Seller</nev><cim><irsz>1</irsz><telepules>B</telepules><cim>C</cim></cim>
            <postacim><orszag>HU</orszag><irsz>2</irsz><telepules>P</telepules><cim>Post</cim></postacim>
            <adoszam>12345678-2-42</adoszam><csoportazonosito>GROUP-S</csoportazonosito><adoszameu>HU123</adoszameu></szallito>
          <alap><id>2</id><szamlaszam>INV-1</szamlaszam><gazdEsemAzon>9</gazdEsemAzon><forras>34</forras>
            <iktatoszam>REG-1</iktatoszam><tipus>E</tipus><eszamla>3</eszamla><hivszamlaszam>INV-0</hivszamlaszam>
            <hivdijbekszam>PRO-1</hivdijbekszam><kelt>2026-07-01</kelt><telj>2026-07-02</telj><fizh>2026-07-10</fizh>
            <fizmod>transfer</fizmod><fizmodunified>átutalás</fizmodunified><keszpenz>false</keszpenz>
            <rendelesszam>ORDER-1</rendelesszam><nyelv>hu</nyelv><devizanem>EUR</devizanem><devizabank>MNB</devizabank>
            <devizaarf>400</devizaarf><afatipus>EUT</afatipus><penzforg>true</penzforg><kata>false</kata>
            <katafokonyv>true</katafokonyv><teszt>false</teszt><sztornozott>true</sztornozott></alap>
          <vevo><id>3</id><nev>Buyer</nev><azonosito>BUY-1</azonosito><cim><irsz>3</irsz><telepules>V</telepules><cim>Main</cim></cim>
            <postacim><nev>Receiver</nev><orszag>HU</orszag><irsz>4</irsz><telepules>Q</telepules><cim>Ship</cim></postacim>
            <email>b@example.com</email><adoszam>87654321-1-42</adoszam><csoportazonosito>GROUP-B</csoportazonosito>
            <adoszameu>HU876</adoszameu><lokacio>7</lokacio><privatePersonIndicator>true</privatePersonIndicator>
            <fokonyv><vevo>311</vevo><vevoazon>B-7</vevoazon><datum>2026-07-03</datum><folyamatostelj>true</folyamatostelj>
              <elszDatTol>2026-07-01</elszDatTol><elszDatIg>2026-07-31</elszDatIg></fokonyv></vevo>
          <tetelek><tetel><nev>Item</nev><azonosito>I-1</azonosito><mennyiseg>1</mennyiseg><mennyisegiegyseg>db</mennyisegiegyseg>
            <nettoegysegar>100</nettoegysegar><afatipus>AAM</afatipus><afakulcs>0</afakulcs><netto>100</netto><afa>0</afa><brutto>100</brutto>
            <sztetordering>5</sztetordering><fokonyv><arbevetel>911</arbevetel><afa>467</afa><gazdasagiesemeny>SALE</gazdasagiesemeny>
              <gazdasagiesemenyafa>VAT</gazdasagiesemenyafa><elszdattol>2026-07-01</elszdattol><elszdatig>2026-07-31</elszdatig></fokonyv></tetel></tetelek>
          <qutetek><qutet><nev>Fee</nev><afatipus>TAM</afatipus><afakulcs>0</afakulcs><netto>10</netto><afa>0</afa><brutto>10</brutto>
            <elszdattol>2026-07-01</elszdattol><elszdatig>2026-07-31</elszdatig><afalevon>50</afalevon><cimkek><cimke>fee</cimke></cimkek></qutet></qutetek>
          <cimkek><cimke>invoice</cimke></cimkek>
          <osszegek><afakulcsossz><afatipus>AAM</afatipus><afakulcs>0</afakulcs><netto>110</netto><afa>0</afa><brutto>110</brutto></afakulcsossz>
            <totalossz><netto>110</netto><afa>0</afa><brutto>110</brutto></totalossz></osszegek>
          <kifizetesek><kifizetes><datum>2026-07-04</datum><jogcim>transfer</jogcim><osszeg>10</osszeg>
            <bankszamlaszam>ACC</bankszamlaszam><banktranzid>99</banktranzid><devizaarf>401</devizaarf></kifizetes></kifizetesek></szamla>"#;
        let response = RawResponse::new::<&str, &str>([], body.as_bytes().to_vec());
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
        assert!(document.info.reversed);
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
