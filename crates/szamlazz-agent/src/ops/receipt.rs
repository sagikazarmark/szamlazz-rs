//! Receipt operations (`nyugta`): creation (`xmlnyugtacreate`), storno
//! (`xmlnyugtast`), query (`xmlnyugtaget`), and email sending
//! (`xmlnyugtasend`).

use jiff::civil::Date;
use rust_decimal::Decimal;

use crate::credentials::Credentials;
use crate::error::{ApiError, ParseError, RequestError, ResponseError};
use crate::item::LineItem;
use crate::ops::invoice::ExchangeRate;
use crate::types::{Currency, PaymentMethod, Pdf, ReceiptNumber, VatRate};
use crate::wire::{AgentRequest, RawResponse};
use crate::xml;

/// The PDF template a receipt is rendered with (`pdfSablon`).
///
/// An empty or unknown value on the wire falls back to the default A4
/// template.
#[doc(alias = "pdfSablon")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum ReceiptTemplate {
    /// `A` — the default A4 page.
    A4Default,
    /// `J` — ticket format.
    Ticket,
    /// `L` — ticket format with logo.
    TicketWithLogo,
    /// `N` — 80 mm roll (receipt printer).
    Roll80mm,
}

impl ReceiptTemplate {
    /// The exact wire token.
    #[must_use]
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::A4Default => "A",
            Self::Ticket => "J",
            Self::TicketWithLogo => "L",
            Self::Roll80mm => "N",
        }
    }
}

/// One payment recorded on a receipt (`kifizetes`).
#[doc(alias = "kifizetés")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct ReceiptPayment {
    /// The legal tender used (`fizetoeszkoz`); free text, e.g. `készpénz`.
    #[doc(alias = "fizetőeszköz")]
    pub method: String,
    /// Amount paid with this tender (`osszeg`).
    pub amount: Decimal,
    /// Free-text description of the tender (`leiras`).
    pub description: Option<String>,
}

impl ReceiptPayment {
    /// A payment of `amount` via `method`, with no description.
    pub fn new(method: impl Into<String>, amount: Decimal) -> Self {
        Self {
            method: method.into(),
            amount,
            description: None,
        }
    }
}

/// The receipt-creation operation (`xmlnyugtacreate`,
/// `action-szamla_agent_nyugta_create`).
///
/// [`CreateReceipt::call_id`] prevents duplicate issuance by making a repeated
/// identifier fail with error 338. It is not replay-success idempotency: a
/// retry does not return the original success. The PDF, when
/// [`CreateReceipt::download_pdf`] is set, arrives decoded in
/// [`ReceiptResult::pdf`].
#[doc(alias = "xmlnyugtacreate")]
#[doc(alias = "nyugta készítés")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct CreateReceipt {
    /// Unique call identifier (`hivasAzonosito`). Reusing it returns error 338,
    /// which prevents duplicate issuance but does not replay the prior result.
    #[doc(alias = "hivasAzonosito")]
    pub call_id: Option<String>,
    /// Receipt number prefix (`elotag`), e.g. `NYGTA` → `NYGTA-2026-111`.
    #[doc(alias = "előtag")]
    pub prefix: String,
    /// Payment method (`fizmod`).
    pub payment_method: PaymentMethod,
    /// Currency (`penznem`).
    pub currency: Currency,
    /// Exchange rate; required when the currency is not HUF. Written as
    /// `devizabank` + `devizaarf` (the invoice operation spells these
    /// `arfolyamBank` + `arfolyam`).
    pub exchange_rate: Option<ExchangeRate>,
    /// Free-text comment shown on the receipt (`megjegyzes`).
    pub comment: Option<String>,
    /// PDF template (`pdfSablon`).
    pub pdf_template: Option<ReceiptTemplate>,
    /// General-ledger identifier of the customer (`fokonyvVevo`).
    #[doc(alias = "fokonyvVevo")]
    pub ledger_customer: Option<String>,
    /// Order number shown on the receipt (`rendelesSzam`).
    #[doc(alias = "rendelésszám")]
    pub order_number: Option<String>,
    /// Return the PDF in the response (`pdfLetoltes`).
    #[serde(default)]
    pub download_pdf: bool,
    /// Line items (`tetelek`); at least one is required.
    pub items: Vec<LineItem>,
    /// Payment breakdown (`kifizetesek`); optional, but when present the docs
    /// require the amounts to sum to the receipt total. This crate does not
    /// validate that — the server is the authority.
    #[serde(default)]
    pub payments: Vec<ReceiptPayment>,
}

impl CreateReceipt {
    /// A receipt-creation request with the required fields; optional fields
    /// default to absent and can be set on the returned value.
    pub fn new(
        prefix: impl Into<String>,
        payment_method: PaymentMethod,
        currency: Currency,
        items: Vec<LineItem>,
    ) -> Self {
        Self {
            call_id: None,
            prefix: prefix.into(),
            payment_method,
            currency,
            exchange_rate: None,
            comment: None,
            pdf_template: None,
            ledger_customer: None,
            order_number: None,
            download_pdf: false,
            items,
            payments: Vec::new(),
        }
    }
}

impl AgentRequest for CreateReceipt {
    const ACTION: &'static str = "action-szamla_agent_nyugta_create";
    type Response = ReceiptResult;

    fn validate(&self) -> Result<(), RequestError> {
        if self.items.is_empty() {
            return Err(RequestError::MissingLineItems);
        }
        if let Some(count) = self
            .items
            .iter()
            .filter_map(|item| item.erasure_code_count)
            .find(|&count| count > crate::item::MAX_ERASURE_CODE_COUNT)
        {
            return Err(RequestError::ErasureCodeCountOutOfRange(count));
        }
        if !self.currency.is_huf() {
            let rate = self
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

    fn write_xml(&self, credentials: &Credentials) -> Vec<u8> {
        xml::document(
            "xmlnyugtacreate",
            "http://www.szamlazz.hu/xmlnyugtacreate",
            |root| {
                root.node("beallitasok", |s| {
                    s.credentials(credentials);
                    s.bool("pdfLetoltes", self.download_pdf);
                });
                root.node("fejlec", |f| {
                    f.text_opt("hivasAzonosito", self.call_id.as_deref());
                    f.text("elotag", &self.prefix);
                    f.text("fizmod", self.payment_method.as_wire());
                    f.text("penznem", self.currency.as_str());
                    if let Some(rate) = &self.exchange_rate {
                        f.text("devizabank", &rate.bank);
                        if let Some(rate) = rate.rate {
                            f.decimal("devizaarf", rate);
                        }
                    }
                    f.text_opt("megjegyzes", self.comment.as_deref());
                    if let Some(template) = self.pdf_template {
                        f.text("pdfSablon", template.as_wire());
                    }
                    f.text_opt("fokonyvVevo", self.ledger_customer.as_deref());
                    f.text_opt("rendelesSzam", self.order_number.as_deref());
                });
                root.node("tetelek", |t| {
                    for item in &self.items {
                        t.node("tetel", |i| {
                            // Receipt rows spell the value elements netto/afa/
                            // brutto, unlike the invoice's nettoErtek/afaErtek/
                            // bruttoErtek.
                            i.text("megnevezes", &item.name);
                            i.text_opt("azonosito", item.id.as_deref());
                            i.decimal("mennyiseg", item.quantity);
                            i.text("mennyisegiEgyseg", &item.unit);
                            i.decimal("nettoEgysegar", item.unit_price);
                            i.text("afakulcs", &item.vat_rate.as_wire());
                            i.decimal("netto", item.net_value);
                            i.decimal("afa", item.vat_value);
                            i.decimal("brutto", item.gross_value);
                            if let Some(ledger) = &item.ledger {
                                i.node("fokonyv", |l| {
                                    l.text_opt("arbevetel", ledger.revenue_account.as_deref());
                                    l.text_opt("afa", ledger.vat_account.as_deref());
                                });
                            }
                            i.text_opt("megjegyzes", item.comment.as_deref());
                            if let Some(count) = item.erasure_code_count {
                                i.text("torloKod", &count.to_string());
                            }
                        });
                    }
                });
                if !self.payments.is_empty() {
                    root.node("kifizetesek", |k| {
                        for payment in &self.payments {
                            k.node("kifizetes", |p| {
                                p.text("fizetoeszkoz", &payment.method);
                                p.decimal("osszeg", payment.amount);
                                p.text_opt("leiras", payment.description.as_deref());
                            });
                        }
                    });
                }
            },
        )
    }

    fn parse(&self, response: &RawResponse) -> Result<Self::Response, ResponseError> {
        parse_receipt(response)
    }
}

/// The receipt storno operation (`xmlnyugtast`,
/// `action-szamla_agent_nyugta_storno`): cancels an issued receipt.
///
/// The response carries the newly created storno (`SN`) receipt.
#[doc(alias = "xmlnyugtast")]
#[doc(alias = "nyugta sztornó")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct StornoReceipt {
    /// The receipt to cancel (`nyugtaszam`).
    pub receipt_number: ReceiptNumber,
    /// Return the storno receipt PDF in the response (`pdfLetoltes`).
    #[serde(default)]
    pub download_pdf: bool,
    /// PDF template (`pdfSablon`).
    pub pdf_template: Option<ReceiptTemplate>,
    /// Unique call identifier for the storno operation (`hivasAzonosito`).
    /// Reusing it returns error 338.
    pub call_id: Option<String>,
}

impl StornoReceipt {
    /// A cancellation of the given receipt; no PDF is requested.
    pub fn new(receipt_number: impl Into<ReceiptNumber>) -> Self {
        Self {
            receipt_number: receipt_number.into(),
            download_pdf: false,
            pdf_template: None,
            call_id: None,
        }
    }
}

impl AgentRequest for StornoReceipt {
    const ACTION: &'static str = "action-szamla_agent_nyugta_storno";
    type Response = ReceiptResult;

    fn write_xml(&self, credentials: &Credentials) -> Vec<u8> {
        xml::document(
            "xmlnyugtast",
            "http://www.szamlazz.hu/xmlnyugtast",
            |root| {
                root.node("beallitasok", |s| {
                    s.credentials(credentials);
                    s.bool("pdfLetoltes", self.download_pdf);
                });
                root.node("fejlec", |f| {
                    f.text("nyugtaszam", self.receipt_number.as_str());
                    if let Some(template) = self.pdf_template {
                        f.text("pdfSablon", template.as_wire());
                    }
                    f.text_opt("hivasAzonosito", self.call_id.as_deref());
                });
            },
        )
    }

    fn parse(&self, response: &RawResponse) -> Result<Self::Response, ResponseError> {
        parse_receipt(response)
    }
}

/// The key a receipt is looked up by in [`QueryReceipt`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ReceiptSelector {
    /// Look up by receipt number (`nyugtaszam`).
    ReceiptNumber(ReceiptNumber),
    /// Look up by order number (`rendelesSzam`).
    #[doc(alias = "rendelésszám")]
    OrderNumber(String),
}

/// The receipt query operation (`xmlnyugtaget`,
/// `action-szamla_agent_nyugta_get`): fetches an issued receipt by receipt
/// number or order number.
#[doc(alias = "xmlnyugtaget")]
#[doc(alias = "nyugta lekérdezés")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct QueryReceipt {
    /// Which receipt to fetch.
    pub selector: ReceiptSelector,
    /// Return the PDF in the response (`pdfLetoltes`).
    #[serde(default)]
    pub download_pdf: bool,
    /// PDF template for the returned PDF (`pdfSablon`).
    pub pdf_template: Option<ReceiptTemplate>,
    /// Call identifier (`hivasAzonosito`), as supplied at creation.
    #[doc(alias = "hivasAzonosito")]
    pub call_id: Option<String>,
}

impl QueryReceipt {
    /// A query for the receipt named by `selector`; no PDF is requested.
    #[must_use]
    pub fn new(selector: ReceiptSelector) -> Self {
        Self {
            selector,
            download_pdf: false,
            pdf_template: None,
            call_id: None,
        }
    }
}

impl AgentRequest for QueryReceipt {
    const ACTION: &'static str = "action-szamla_agent_nyugta_get";
    type Response = ReceiptResult;

    fn write_xml(&self, credentials: &Credentials) -> Vec<u8> {
        xml::document(
            "xmlnyugtaget",
            "http://www.szamlazz.hu/xmlnyugtaget",
            |root| {
                root.node("beallitasok", |s| {
                    s.credentials(credentials);
                    s.bool("pdfLetoltes", self.download_pdf);
                });
                root.node("fejlec", |f| {
                    match &self.selector {
                        ReceiptSelector::ReceiptNumber(number) => {
                            f.text("nyugtaszam", number.as_str());
                        }
                        ReceiptSelector::OrderNumber(number) => f.text("rendelesSzam", number),
                    }
                    f.text_opt("hivasAzonosito", self.call_id.as_deref());
                    if let Some(template) = self.pdf_template {
                        f.text("pdfSablon", template.as_wire());
                    }
                });
            },
        )
    }

    fn parse(&self, response: &RawResponse) -> Result<Self::Response, ResponseError> {
        parse_receipt(response)
    }
}

/// Email settings for [`SendReceipt`] (`emailKuldes`).
///
/// Fields left `None` fall back to the values used the last time the receipt
/// was emailed.
#[doc(alias = "email küldés")]
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct ReceiptEmail {
    /// Recipient address (`email`); multiple recipients may be
    /// comma-separated.
    pub to: Option<String>,
    /// Reply-to address (`emailReplyto`).
    pub reply_to: Option<String>,
    /// Subject (`emailTargy`).
    pub subject: Option<String>,
    /// Body (`emailSzoveg`).
    pub body: Option<String>,
}

/// The receipt email-sending operation (`xmlnyugtasend`,
/// `action-szamla_agent_nyugta_send`): emails an already issued receipt.
///
/// The success response is a plain acknowledgement, so the parsed payload is
/// `()`.
#[doc(alias = "xmlnyugtasend")]
#[doc(alias = "nyugta küldés")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct SendReceipt {
    /// The receipt to email (`nyugtaszam`).
    pub receipt_number: ReceiptNumber,
    /// Email overrides; when `None`, an empty `emailKuldes` block requests a
    /// resend using the previous email details.
    pub email: Option<ReceiptEmail>,
}

impl SendReceipt {
    /// A request to resend the previously used email for the given receipt.
    /// Set [`SendReceipt::email`] on the returned value to override it.
    pub fn new(receipt_number: impl Into<ReceiptNumber>) -> Self {
        Self {
            receipt_number: receipt_number.into(),
            email: None,
        }
    }
}

impl AgentRequest for SendReceipt {
    const ACTION: &'static str = "action-szamla_agent_nyugta_send";
    type Response = ();

    fn write_xml(&self, credentials: &Credentials) -> Vec<u8> {
        xml::document(
            "xmlnyugtasend",
            "http://www.szamlazz.hu/xmlnyugtasend",
            |root| {
                root.node("beallitasok", |s| s.credentials(credentials));
                root.node("fejlec", |f| {
                    f.text("nyugtaszam", self.receipt_number.as_str());
                });
                root.node("emailKuldes", |e| {
                    if let Some(email) = &self.email {
                        e.text_opt("email", email.to.as_deref());
                        e.text_opt("emailReplyto", email.reply_to.as_deref());
                        e.text_opt("emailTargy", email.subject.as_deref());
                        e.text_opt("emailSzoveg", email.body.as_deref());
                    }
                });
            },
        )
    }

    fn parse(&self, response: &RawResponse) -> Result<Self::Response, ResponseError> {
        response.check()?;
        ReceiptSendResponse::from_body(response.body())?.into_success()
    }
}

/// A successfully created, cancelled, or queried receipt
/// (`xmlnyugtavalasz`).
#[doc(alias = "xmlnyugtavalasz")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct ReceiptResult {
    /// The receipt data.
    pub receipt: Receipt,
    /// The receipt PDF (`nyugtaPdf`), when requested.
    pub pdf: Option<Pdf>,
}

/// A receipt as returned by szamlazz.hu (`nyugta`).
#[doc(alias = "nyugta")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct Receipt {
    /// Internal szamlazz.hu identifier (`id`).
    pub id: u64,
    /// The call identifier supplied at creation (`hivasAzonosito`), if any.
    #[doc(alias = "hivasAzonosito")]
    pub call_id: Option<String>,
    /// The receipt number (`nyugtaszam`).
    #[doc(alias = "nyugtaszám")]
    pub receipt_number: ReceiptNumber,
    /// Receipt type (`tipus`): `NY` for a receipt, `SN` for a storno
    /// (cancellation) receipt.
    #[doc(alias = "típus")]
    pub kind: String,
    /// Whether this receipt has been cancelled (`stornozott`); meaningful for
    /// `NY` receipts.
    #[doc(alias = "stornózott")]
    pub cancelled: bool,
    /// For `SN` receipts, the number of the receipt being cancelled
    /// (`stornozottNyugtaszam`).
    pub cancelled_receipt_number: Option<ReceiptNumber>,
    /// Issue date (`kelt`).
    pub issue_date: Date,
    /// Payment method (`fizmod`).
    pub payment_method: PaymentMethod,
    /// Currency (`penznem`).
    pub currency: Currency,
    /// Quoting bank for foreign-currency receipts (`devizabank`).
    pub exchange_bank: Option<String>,
    /// Exchange rate for foreign-currency receipts (`devizaarf`).
    pub exchange_rate: Option<Decimal>,
    /// Free-text comment (`megjegyzes`).
    pub comment: Option<String>,
    /// General-ledger identifier of the customer (`fokonyvVevo`).
    #[doc(alias = "fokonyvVevo")]
    pub ledger_customer: Option<String>,
    /// Whether a test account issued the receipt (`teszt`).
    pub test: bool,
    /// Order number (`rendelesSzam`).
    #[doc(alias = "rendelésszám")]
    pub order_number: Option<String>,
    /// Line items (`tetelek`).
    #[doc(alias = "tételek")]
    pub items: Vec<ReceiptItem>,
    /// Payments (`kifizetesek`).
    #[doc(alias = "kifizetések")]
    #[serde(default)]
    pub payments: Vec<ReceiptPayment>,
    /// Totals per VAT rate and overall (`osszegek`).
    #[doc(alias = "összegek")]
    pub totals: ReceiptTotals,
}

/// One row of a returned receipt (`tetel`).
///
/// Response rows carry the raw VAT rate token (`afakulcs`) plus an optional
/// VAT category code (`afatipus`); [`ReceiptItem::vat_rate`] combines them
/// into a typed rate.
#[doc(alias = "tétel")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct ReceiptItem {
    /// Item name (`megnevezes`).
    pub name: String,
    /// Item identifier (`azonosito`).
    pub id: Option<String>,
    /// Quantity (`mennyiseg`).
    pub quantity: Decimal,
    /// Unit of measure (`mennyisegiEgyseg`), e.g. `db`.
    pub unit: String,
    /// Net unit price (`nettoEgysegar`).
    pub unit_price: Decimal,
    /// VAT category code (`afatipus`), set when a special code (AAM, EUT, …)
    /// applies.
    #[doc(alias = "áfatípus")]
    pub vat_type: Option<String>,
    /// Raw VAT rate token (`afakulcs`).
    #[doc(alias = "áfakulcs")]
    pub vat_code: String,
    /// Net value (`netto`).
    pub net_value: Decimal,
    /// VAT value (`afa`).
    pub vat_value: Decimal,
    /// Gross value (`brutto`).
    pub gross_value: Decimal,
    /// General-ledger metadata (`fokonyv`).
    pub ledger: Option<ReceiptItemLedger>,
}

/// General-ledger metadata returned for a receipt item (`fokonyv`).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct ReceiptItemLedger {
    /// Revenue general-ledger account (`arbevetel`).
    pub revenue_account: Option<String>,
    /// VAT general-ledger account (`afa`).
    pub vat_account: Option<String>,
}

impl ReceiptItem {
    /// The typed VAT rate: [`ReceiptItem::vat_type`] when present, otherwise
    /// [`ReceiptItem::vat_code`].
    #[must_use]
    pub fn vat_rate(&self) -> VatRate {
        match self.vat_type.as_deref() {
            Some(code) => VatRate::from(code),
            None => VatRate::from(self.vat_code.as_str()),
        }
    }
}

/// Receipt totals (`osszegek`): per-VAT-rate subtotals and the grand total.
#[doc(alias = "összegek")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct ReceiptTotals {
    /// Subtotals per VAT rate (`afakulcsossz`).
    pub by_rate: Vec<VatRateTotal>,
    /// Grand totals (`totalossz`).
    pub total: TotalAmounts,
}

/// The subtotal for one VAT rate (`afakulcsossz`).
#[doc(alias = "áfakulcs összesítés")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct VatRateTotal {
    /// VAT category code (`afatipus`), set when a special code applies.
    #[doc(alias = "áfatípus")]
    pub vat_type: Option<String>,
    /// Raw VAT rate token (`afakulcs`).
    #[doc(alias = "áfakulcs")]
    pub vat_code: String,
    /// Net subtotal (`netto`).
    pub net: Decimal,
    /// VAT subtotal (`afa`).
    pub vat: Decimal,
    /// Gross subtotal (`brutto`).
    pub gross: Decimal,
}

impl VatRateTotal {
    /// The typed VAT rate: [`VatRateTotal::vat_type`] when present, otherwise
    /// [`VatRateTotal::vat_code`].
    #[must_use]
    pub fn vat_rate(&self) -> VatRate {
        match self.vat_type.as_deref() {
            Some(code) => VatRate::from(code),
            None => VatRate::from(self.vat_code.as_str()),
        }
    }
}

/// The grand total of a receipt (`totalossz`).
#[doc(alias = "totál összesítés")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct TotalAmounts {
    /// Net total (`netto`).
    pub net: Decimal,
    /// VAT total (`afa`).
    pub vat: Decimal,
    /// Gross total (`brutto`).
    pub gross: Decimal,
}

/// Parses an `xmlnyugtavalasz` body into a [`ReceiptResult`]. Shared by the
/// create, storno, and query operations.
fn parse_receipt(response: &RawResponse) -> Result<ReceiptResult, ResponseError> {
    response.check()?;
    let valasz = ReceiptResponse::from_body(response.body())?.into_success()?;
    let nyugta = valasz.nyugta.ok_or(ParseError::Missing("nyugta"))?;

    Ok(ReceiptResult {
        receipt: nyugta.into(),
        pdf: match valasz.nyugta_pdf.filter(|s| !s.is_empty()) {
            Some(encoded) => Some(Pdf::from_base64(&encoded)?),
            None => None,
        },
    })
}

/// Builds the [`ApiError`] reported in a response body.
fn api_error(hibakod: Option<String>, hibauzenet: Option<String>) -> ResponseError {
    ApiError {
        code: hibakod.map_or_else(|| crate::ErrorCode::Unknown("0".to_owned()), Into::into),
        message: hibauzenet.unwrap_or_default(),
    }
    .into()
}

/// The `xmlnyugtavalasz` response document.
#[derive(Debug, serde::Deserialize)]
struct ReceiptResponse {
    #[serde(deserialize_with = "xml::de::flexible_bool")]
    sikeres: bool,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    hibakod: Option<String>,
    #[serde(default)]
    hibauzenet: Option<String>,
    #[serde(default, rename(deserialize = "nyugtaPdf"))]
    nyugta_pdf: Option<String>,
    #[serde(default)]
    nyugta: Option<NyugtaXml>,
}

impl ReceiptResponse {
    fn from_body(body: &[u8]) -> Result<Self, ParseError> {
        let text = xml::response_text(
            body,
            "xmlnyugtavalasz",
            "http://www.szamlazz.hu/xmlnyugtavalasz",
        )?;

        Ok(quick_xml::de::from_str(text)?)
    }

    /// Converts a `sikeres=false` response into the reported [`ApiError`].
    fn into_success(self) -> Result<Self, ResponseError> {
        if self.sikeres {
            Ok(self)
        } else {
            Err(api_error(self.hibakod, self.hibauzenet))
        }
    }
}

/// The `xmlnyugtasendvalasz` response document.
#[derive(Debug, serde::Deserialize)]
struct ReceiptSendResponse {
    #[serde(deserialize_with = "xml::de::flexible_bool")]
    sikeres: bool,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    hibakod: Option<String>,
    #[serde(default)]
    hibauzenet: Option<String>,
}

impl ReceiptSendResponse {
    fn from_body(body: &[u8]) -> Result<Self, ParseError> {
        let text = xml::response_text(
            body,
            "xmlnyugtasendvalasz",
            "http://www.szamlazz.hu/xmlnyugtasendvalasz",
        )?;

        Ok(quick_xml::de::from_str(text)?)
    }

    /// Converts a `sikeres=false` response into the reported [`ApiError`].
    fn into_success(self) -> Result<(), ResponseError> {
        if self.sikeres {
            Ok(())
        } else {
            Err(api_error(self.hibakod, self.hibauzenet))
        }
    }
}

/// The `nyugta` element of `xmlnyugtavalasz`.
#[derive(Debug, serde::Deserialize)]
struct NyugtaXml {
    alap: AlapXml,
    tetelek: TetelekXml,
    #[serde(default)]
    kifizetesek: Option<KifizetesekXml>,
    osszegek: OsszegekXml,
}

impl From<NyugtaXml> for Receipt {
    fn from(nyugta: NyugtaXml) -> Self {
        let alap = nyugta.alap;
        Self {
            id: alap.id,
            call_id: alap.hivas_azonosito,
            receipt_number: ReceiptNumber::new(alap.nyugtaszam),
            kind: alap.tipus,
            cancelled: alap.stornozott,
            cancelled_receipt_number: alap.stornozott_nyugtaszam.map(ReceiptNumber::new),
            issue_date: alap.kelt,
            payment_method: PaymentMethod::from(alap.fizmod),
            currency: Currency::new(alap.penznem),
            exchange_bank: alap.devizabank,
            exchange_rate: alap.devizaarf,
            comment: alap.megjegyzes,
            ledger_customer: alap.fokonyv_vevo,
            test: alap.teszt,
            order_number: alap.rendeles_szam,
            items: nyugta.tetelek.tetel.into_iter().map(Into::into).collect(),
            payments: nyugta
                .kifizetesek
                .map(|k| k.kifizetes.into_iter().map(Into::into).collect())
                .unwrap_or_default(),
            totals: nyugta.osszegek.into(),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct AlapXml {
    id: u64,
    #[serde(
        default,
        rename(deserialize = "hivasAzonosito"),
        deserialize_with = "xml::de::empty_as_none"
    )]
    hivas_azonosito: Option<String>,
    nyugtaszam: String,
    tipus: String,
    #[serde(deserialize_with = "xml::de::flexible_bool")]
    stornozott: bool,
    #[serde(
        default,
        rename(deserialize = "stornozottNyugtaszam"),
        deserialize_with = "xml::de::empty_as_none"
    )]
    stornozott_nyugtaszam: Option<String>,
    kelt: Date,
    fizmod: String,
    penznem: String,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    devizabank: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    devizaarf: Option<Decimal>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    megjegyzes: Option<String>,
    #[serde(
        default,
        rename(deserialize = "fokonyvVevo"),
        deserialize_with = "xml::de::empty_as_none"
    )]
    fokonyv_vevo: Option<String>,
    #[serde(deserialize_with = "xml::de::flexible_bool")]
    teszt: bool,
    #[serde(
        default,
        rename(deserialize = "rendelesSzam"),
        deserialize_with = "xml::de::empty_as_none"
    )]
    rendeles_szam: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct TetelekXml {
    #[serde(default)]
    tetel: Vec<TetelXml>,
}

#[derive(Debug, serde::Deserialize)]
struct TetelXml {
    megnevezes: String,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    azonosito: Option<String>,
    #[serde(deserialize_with = "xml::de::from_text")]
    mennyiseg: Decimal,
    #[serde(rename(deserialize = "mennyisegiEgyseg"))]
    mennyisegi_egyseg: String,
    #[serde(
        rename(deserialize = "nettoEgysegar"),
        deserialize_with = "xml::de::from_text"
    )]
    netto_egysegar: Decimal,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    afatipus: Option<String>,
    afakulcs: String,
    #[serde(alias = "nettoErtek", deserialize_with = "xml::de::from_text")]
    netto: Decimal,
    #[serde(alias = "afaErtek", deserialize_with = "xml::de::from_text")]
    afa: Decimal,
    #[serde(alias = "bruttoErtek", deserialize_with = "xml::de::from_text")]
    brutto: Decimal,
    #[serde(default)]
    fokonyv: Option<TetelFokonyvXml>,
}

#[derive(Debug, serde::Deserialize)]
struct TetelFokonyvXml {
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    arbevetel: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    afa: Option<String>,
}

impl From<TetelXml> for ReceiptItem {
    fn from(tetel: TetelXml) -> Self {
        Self {
            name: tetel.megnevezes,
            id: tetel.azonosito,
            quantity: tetel.mennyiseg,
            unit: tetel.mennyisegi_egyseg,
            unit_price: tetel.netto_egysegar,
            vat_type: tetel.afatipus,
            vat_code: tetel.afakulcs,
            net_value: tetel.netto,
            vat_value: tetel.afa,
            gross_value: tetel.brutto,
            ledger: tetel.fokonyv.map(|ledger| ReceiptItemLedger {
                revenue_account: ledger.arbevetel,
                vat_account: ledger.afa,
            }),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct KifizetesekXml {
    #[serde(default)]
    kifizetes: Vec<KifizetesXml>,
}

#[derive(Debug, serde::Deserialize)]
struct KifizetesXml {
    fizetoeszkoz: String,
    #[serde(deserialize_with = "xml::de::from_text")]
    osszeg: Decimal,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    leiras: Option<String>,
}

impl From<KifizetesXml> for ReceiptPayment {
    fn from(kifizetes: KifizetesXml) -> Self {
        Self {
            method: kifizetes.fizetoeszkoz,
            amount: kifizetes.osszeg,
            description: kifizetes.leiras,
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct OsszegekXml {
    #[serde(default)]
    afakulcsossz: Vec<AfakulcsosszXml>,
    totalossz: TotalosszXml,
}

impl From<OsszegekXml> for ReceiptTotals {
    fn from(osszegek: OsszegekXml) -> Self {
        Self {
            by_rate: osszegek.afakulcsossz.into_iter().map(Into::into).collect(),
            total: TotalAmounts {
                net: osszegek.totalossz.netto,
                vat: osszegek.totalossz.afa,
                gross: osszegek.totalossz.brutto,
            },
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

impl From<AfakulcsosszXml> for VatRateTotal {
    fn from(ossz: AfakulcsosszXml) -> Self {
        Self {
            vat_type: ossz.afatipus,
            vat_code: ossz.afakulcs,
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

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use rust_decimal::dec;

    use super::*;

    fn create_sample() -> CreateReceipt {
        CreateReceipt {
            call_id: None,
            prefix: "NYGTA".into(),
            payment_method: PaymentMethod::Cash,
            currency: Currency::HUF,
            exchange_rate: None,
            comment: None,
            pdf_template: None,
            ledger_customer: None,
            order_number: None,
            download_pdf: true,
            items: vec![LineItem::calculated_for_currency(
                "Kitten doormat",
                dec!(2.0),
                "db",
                dec!(10000),
                VatRate::percent(27),
                &Currency::HUF,
            )],
            payments: vec![ReceiptPayment {
                method: "készpénz".into(),
                amount: dec!(25400),
                description: None,
            }],
        }
    }

    fn query_sample() -> QueryReceipt {
        QueryReceipt::new(ReceiptSelector::ReceiptNumber(ReceiptNumber::new(
            "NYGTA-2026-1",
        )))
    }

    fn send_sample() -> SendReceipt {
        SendReceipt::new("NYGTA-2026-1")
    }

    #[test]
    fn writes_canonical_create_xml() {
        let xml = create_sample().write_xml(&Credentials::agent_key("key"));
        let expected = include_str!("../../tests/golden/xmlnyugtacreate.xml").trim_end();
        assert_eq!(String::from_utf8(xml).expect("utf-8"), expected);
    }

    #[test]
    fn foreign_currency_writes_exchange_rate() {
        let mut receipt = create_sample();
        receipt.currency = Currency::EUR;
        receipt.exchange_rate = Some(ExchangeRate {
            bank: "MNB".into(),
            rate: Some(dec!(410.5)),
        });
        let xml =
            String::from_utf8(receipt.write_xml(&Credentials::agent_key("key"))).expect("utf-8");
        assert!(xml.contains(
            "<penznem>EUR</penznem><devizabank>MNB</devizabank><devizaarf>410.5</devizaarf>"
        ));
    }

    #[test]
    fn rejects_create_without_items() {
        let mut receipt = create_sample();
        receipt.items.clear();
        assert!(matches!(
            receipt.to_wire(&Credentials::agent_key("key")),
            Err(RequestError::MissingLineItems)
        ));
    }

    #[test]
    fn rejects_foreign_currency_without_exchange_rate() {
        let mut receipt = create_sample();
        receipt.currency = Currency::EUR;
        receipt.exchange_rate = None;
        assert!(matches!(
            receipt.to_wire(&Credentials::agent_key("key")),
            Err(RequestError::MissingExchangeRate)
        ));
    }

    #[test]
    fn accepts_automatic_mnb_receipt_exchange_rate() {
        let mut receipt = create_sample();
        receipt.currency = Currency::EUR;
        receipt.exchange_rate = Some(ExchangeRate::automatic_mnb());
        let wire = receipt
            .to_wire(&Credentials::agent_key("key"))
            .expect("MNB automatic lookup is valid for receipts");
        let body = String::from_utf8_lossy(&wire.body);
        assert!(body.contains("<devizabank>MNB</devizabank>"));
        assert!(!body.contains("devizaarf"));
    }

    #[test]
    fn rejects_bankless_receipt_exchange_rate() {
        let mut receipt = create_sample();
        receipt.currency = Currency::EUR;
        receipt.exchange_rate = Some(ExchangeRate::new(" ", dec!(410)));
        assert!(matches!(
            receipt.to_wire(&Credentials::agent_key("key")),
            Err(RequestError::InvalidExchangeRate)
        ));
    }

    #[test]
    fn receipt_writes_shared_item_metadata_in_schema_order() {
        let mut receipt = create_sample();
        let item = &mut receipt.items[0];
        item.id = Some("ITEM-1".into());
        item.ledger = Some(crate::LineItemLedger {
            revenue_account: Some("911".into()),
            vat_account: Some("467".into()),
            ..crate::LineItemLedger::default()
        });
        item.comment = Some("row".into());
        item.erasure_code_count = Some(123);
        let xml =
            String::from_utf8(receipt.write_xml(&Credentials::agent_key("key"))).expect("utf-8");
        assert!(xml.contains("<megnevezes>Kitten doormat</megnevezes><azonosito>ITEM-1</azonosito><mennyiseg>2.0</mennyiseg>"));
        assert!(xml.contains("<brutto>25400</brutto><fokonyv><arbevetel>911</arbevetel><afa>467</afa></fokonyv><megjegyzes>row</megjegyzes><torloKod>123</torloKod>"));
    }

    #[test]
    fn writes_canonical_storno_xml() {
        let storno = StornoReceipt {
            receipt_number: ReceiptNumber::new("NYGT-2026-1"),
            download_pdf: true,
            pdf_template: None,
            call_id: None,
        };
        let xml = storno.write_xml(&Credentials::agent_key("key"));
        let expected = include_str!("../../tests/golden/xmlnyugtast.xml").trim_end();
        assert_eq!(String::from_utf8(xml).expect("utf-8"), expected);
    }

    #[test]
    fn storno_writes_pdf_template() {
        let storno = StornoReceipt {
            receipt_number: ReceiptNumber::new("NYGT-2026-1"),
            download_pdf: false,
            pdf_template: Some(ReceiptTemplate::Ticket),
            call_id: None,
        };
        let xml =
            String::from_utf8(storno.write_xml(&Credentials::agent_key("key"))).expect("utf-8");
        assert!(xml.contains("<nyugtaszam>NYGT-2026-1</nyugtaszam><pdfSablon>J</pdfSablon>"));
    }

    #[test]
    fn storno_writes_call_id_after_template() {
        let mut storno = StornoReceipt::new("NYGT-2026-1");
        storno.pdf_template = Some(ReceiptTemplate::Ticket);
        storno.call_id = Some("STORNO-42".into());
        let xml =
            String::from_utf8(storno.write_xml(&Credentials::agent_key("key"))).expect("utf-8");
        assert!(xml.contains("<pdfSablon>J</pdfSablon><hivasAzonosito>STORNO-42</hivasAzonosito>"));
    }

    #[test]
    fn writes_canonical_query_xml() {
        let query = QueryReceipt {
            selector: ReceiptSelector::ReceiptNumber(ReceiptNumber::new("NYGT-2026-1")),
            download_pdf: true,
            pdf_template: None,
            call_id: None,
        };
        let xml = query.write_xml(&Credentials::agent_key("key"));
        let expected = include_str!("../../tests/golden/xmlnyugtaget.xml").trim_end();
        assert_eq!(String::from_utf8(xml).expect("utf-8"), expected);
    }

    #[test]
    fn queries_by_order_number() {
        let query = QueryReceipt {
            selector: ReceiptSelector::OrderNumber("ORDER-123".into()),
            download_pdf: false,
            pdf_template: None,
            call_id: None,
        };
        let xml =
            String::from_utf8(query.write_xml(&Credentials::agent_key("key"))).expect("utf-8");
        assert!(xml.contains("<rendelesSzam>ORDER-123</rendelesSzam>"));
        assert!(!xml.contains("<nyugtaszam>"));
    }

    #[test]
    fn writes_canonical_send_xml() {
        let send = SendReceipt {
            receipt_number: ReceiptNumber::new("NYGT-2026-1"),
            email: Some(ReceiptEmail {
                to: Some("vevo@example.com".into()),
                reply_to: None,
                subject: Some("Nyugta".into()),
                body: Some("Mellékelten küldjük a nyugtát.".into()),
            }),
        };
        let xml = send.write_xml(&Credentials::agent_key("key"));
        let expected = include_str!("../../tests/golden/xmlnyugtasend.xml").trim_end();
        assert_eq!(String::from_utf8(xml).expect("utf-8"), expected);
    }

    #[test]
    fn send_without_overrides_requests_resend() {
        let send = SendReceipt {
            receipt_number: ReceiptNumber::new("NYGT-2026-1"),
            email: None,
        };
        let xml = String::from_utf8(send.write_xml(&Credentials::agent_key("key"))).expect("utf-8");
        assert!(xml.contains("<emailKuldes></emailKuldes>"));
    }

    #[test]
    fn parses_receipt_response() {
        let body = include_bytes!("../../tests/synthetic/xmlnyugtavalasz.xml");
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let result = query_sample().parse(&response).expect("success");
        let receipt = result.receipt;
        assert_eq!(receipt.id, 123_456);
        assert_eq!(receipt.call_id, None);
        assert_eq!(receipt.receipt_number.as_str(), "NYGT-TST-2026-123");
        assert_eq!(receipt.kind, "NY");
        assert!(!receipt.cancelled);
        assert_eq!(
            receipt
                .cancelled_receipt_number
                .as_ref()
                .map(ReceiptNumber::as_str),
            Some("NYGT-TST-2026-100")
        );
        assert_eq!(receipt.issue_date, date(2026, 1, 1));
        assert_eq!(receipt.payment_method, PaymentMethod::Other("cash".into()));
        assert_eq!(receipt.currency, Currency::EUR);
        assert_eq!(receipt.exchange_bank, None);
        assert_eq!(receipt.exchange_rate, Some(dec!(210)));
        assert!(!receipt.test);
        assert_eq!(receipt.items.len(), 2);
        assert_eq!(receipt.items[0].name, "Synthetic item A");
        assert_eq!(receipt.items[0].id.as_deref(), Some("ITEM-1"));
        assert_eq!(receipt.items[0].vat_rate(), VatRate::percent(27));
        assert_eq!(receipt.items[0].gross_value, dec!(25400.0));
        assert_eq!(receipt.items[1].net_value, dec!(20000.0));
        assert_eq!(receipt.items[1].vat_value, dec!(5400.0));
        assert_eq!(receipt.items[1].gross_value, dec!(25400.0));
        assert_eq!(
            receipt.items[0]
                .ledger
                .as_ref()
                .and_then(|ledger| ledger.revenue_account.as_deref()),
            Some("911")
        );
        assert_eq!(
            receipt.items[0]
                .ledger
                .as_ref()
                .and_then(|ledger| ledger.vat_account.as_deref()),
            Some("467")
        );
        assert_eq!(receipt.payments.len(), 2);
        assert_eq!(
            receipt.payments[0].description.as_deref(),
            Some("Synthetic voucher")
        );
        assert_eq!(receipt.payments[1].amount, dec!(3000.0));
        assert_eq!(receipt.totals.by_rate.len(), 1);
        assert_eq!(receipt.totals.by_rate[0].vat_type.as_deref(), Some("ÁKK"));
        assert_eq!(receipt.totals.by_rate[0].vat_rate(), VatRate::Akk);
        assert_eq!(receipt.totals.total.gross, dec!(254));
        assert!(result.pdf.is_none());
    }

    #[test]
    fn decodes_receipt_pdf() {
        let body = r#"<?xml version="1.0" encoding="UTF-8"?><xmlnyugtavalasz xmlns="http://www.szamlazz.hu/xmlnyugtavalasz"><sikeres>true</sikeres><nyugtaPdf>JVBERi0=</nyugtaPdf><nyugta><alap><id>1</id><nyugtaszam>NYGT-2026-1</nyugtaszam><tipus>NY</tipus><stornozott>false</stornozott><kelt>2026-07-04</kelt><fizmod>készpénz</fizmod><penznem>HUF</penznem><teszt>false</teszt></alap><tetelek><tetel><megnevezes>Kitten doormat</megnevezes><mennyiseg>2.0</mennyiseg><mennyisegiEgyseg>db</mennyisegiEgyseg><nettoEgysegar>10000</nettoEgysegar><afakulcs>27</afakulcs><netto>20000.0</netto><afa>5400.0</afa><brutto>25400.0</brutto></tetel></tetelek><osszegek><afakulcsossz><afakulcs>27</afakulcs><netto>20000.0</netto><afa>5400.0</afa><brutto>25400.0</brutto></afakulcsossz><totalossz><netto>20000.0</netto><afa>5400.0</afa><brutto>25400.0</brutto></totalossz></osszegek></nyugta></xmlnyugtavalasz>"#;
        let response = RawResponse::new::<&str, &str>([], body.as_bytes().to_vec());
        let result = create_sample().parse(&response).expect("success");
        assert_eq!(result.pdf.expect("pdf").as_bytes(), b"%PDF-");
    }

    /// The receipt result is journal-safe: it round-trips through JSON with
    /// the payment method and currency as their wire tokens and the PDF as
    /// base64.
    #[test]
    fn receipt_result_round_trips_through_json() {
        let body = include_str!("../../tests/synthetic/xmlnyugtavalasz.xml").replace(
            "<sikeres>true</sikeres>",
            "<sikeres>true</sikeres><nyugtaPdf>JVBERi0=</nyugtaPdf>",
        );
        let response = RawResponse::new::<&str, &str>([], body.into_bytes());
        let result = query_sample().parse(&response).expect("success");
        assert!(result.pdf.is_some(), "the fixture carries a PDF");

        let json = serde_json::to_value(&result).expect("serialize");
        assert_eq!(json["receipt"]["receipt_number"], "NYGT-TST-2026-123");
        assert_eq!(json["receipt"]["payment_method"], "cash");
        assert_eq!(json["receipt"]["currency"], "EUR");
        assert_eq!(json["receipt"]["items"][0]["vat_code"], "27");
        assert_eq!(json["receipt"]["totals"]["by_rate"][0]["vat_type"], "ÁKK");
        assert_eq!(json["pdf"], "JVBERi0=");

        let restored: ReceiptResult = serde_json::from_value(json).expect("deserialize");
        assert_eq!(restored, result);
    }

    #[test]
    fn parses_receipt_error_response() {
        let body = r#"<?xml version="1.0" encoding="UTF-8"?><xmlnyugtavalasz xmlns="http://www.szamlazz.hu/xmlnyugtavalasz"><sikeres>false</sikeres><hibakod>3</hibakod><hibauzenet>Sikertelen bejelentkezés.</hibauzenet></xmlnyugtavalasz>"#;
        let response = RawResponse::new::<&str, &str>([], body.as_bytes().to_vec());
        let error = create_sample().parse(&response).expect_err("error");
        match error {
            ResponseError::Api(api) => {
                assert_eq!(api.code, crate::ErrorCode::InvalidCredentials);
                assert!(api.message.contains("Sikertelen bejelentkezés"));
            }
            other => panic!("expected api error, got {other:?}"),
        }
    }

    #[test]
    fn preserves_receipt_error_code_above_u16() {
        let body = br#"<xmlnyugtavalasz xmlns="http://www.szamlazz.hu/xmlnyugtavalasz"><sikeres>false</sikeres><hibakod>70000</hibakod><hibauzenet>future</hibauzenet></xmlnyugtavalasz>"#;
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let error = create_sample().parse(&response).expect_err("error");
        match error {
            ResponseError::Api(api) => {
                assert_eq!(api.code, crate::ErrorCode::Unknown("70000".to_owned()));
            }
            other => panic!("expected api error, got {other:?}"),
        }
    }

    #[test]
    fn parses_send_ack() {
        let body = include_bytes!("../../tests/synthetic/xmlnyugtasendvalasz.xml");
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        send_sample().parse(&response).expect("success");
    }

    #[test]
    fn parses_send_error() {
        let body = include_bytes!("../../tests/synthetic/xmlnyugtasendvalasz_error.xml");
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let error = send_sample().parse(&response).expect_err("error");
        match error {
            ResponseError::Api(api) => {
                assert_eq!(api.code, crate::ErrorCode::MissingData);
                assert!(api.message.contains("Missing synthetic email subject"));
            }
            other => panic!("expected api error, got {other:?}"),
        }
    }

    #[test]
    fn header_error_takes_precedence() {
        let response = RawResponse::new(
            [("szlahu_error_code", "3"), ("szlahu_error", "login")],
            Vec::new(),
        );
        let error = query_sample().parse(&response).expect_err("error");
        match error {
            ResponseError::Api(api) => assert_eq!(api.code, crate::ErrorCode::InvalidCredentials),
            other => panic!("expected api error, got {other:?}"),
        }
    }
}
