//! Receipt operations (`nyugta`): creation (`xmlnyugtacreate`), storno
//! (`xmlnyugtast`), query (`xmlnyugtaget`), and email sending
//! (`xmlnyugtasend`).
//!
//! See the [operation recovery table](crate::error#recovery) for lost answers.

use jiff::civil::Date;
use rust_decimal::Decimal;

use crate::credentials::Credentials;
use crate::error::{ParseError, RequestError, ResponseError};
use crate::item::{LineItem, LineItemLedger};
use crate::types::{
    Currency, ExchangeRate, PaymentMethod, Pdf, ReceiptNumber, ReceiptType, Totals, VatRate,
};
use crate::wire::{AgentRequest, RawResponse};
use crate::xml;
use crate::xml::totals::OsszegekXml;

/// The `xmlnyugtavalasz` envelope: the reply of the create, storno and query
/// operations.
const VALASZ_ROOT: &str = "xmlnyugtavalasz";
const VALASZ_NAMESPACE: &str = "http://www.szamlazz.hu/xmlnyugtavalasz";

/// The PDF template a receipt is rendered with (`pdfSablon`).
///
/// The set is open like every wire token set: a token the crate does not
/// know is [`ReceiptTemplate::Other`]. szamlazz.hu renders an empty or
/// unknown token with the default A4 template.
#[doc(alias = "pdfSablon")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum ReceiptTemplate {
    /// `A`: the default A4 page.
    A4Default,
    /// `J`: ticket format.
    Ticket,
    /// `L`: ticket format with logo.
    TicketWithLogo,
    /// `N`: 80 mm roll (receipt printer).
    Roll80mm,
    /// A future or account-specific template token.
    Other(String),
}

impl ReceiptTemplate {
    /// The exact wire token.
    #[must_use]
    pub fn as_wire(&self) -> &str {
        match self {
            Self::A4Default => "A",
            Self::Ticket => "J",
            Self::TicketWithLogo => "L",
            Self::Roll80mm => "N",
            Self::Other(token) => token,
        }
    }
}

/// One payment recorded on a receipt (`kifizetes`).
///
/// Sent in [`CreateReceipt::payments`] and read back in [`Receipt::payments`].
/// Because szamlazz.hu may grow the block it reports, this type stays
/// `#[non_exhaustive]` like every response type; build it with
/// [`ReceiptPayment::new`].
#[doc(alias = "kifizetés")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct ReceiptPayment {
    /// The legal tender used (`fizetoeszkoz`); free text, e.g. `készpénz`.
    #[doc(alias = "fizetőeszköz")]
    pub method: String,
    /// Amount paid with this tender (`osszeg`).
    #[serde(deserialize_with = "crate::number::de::required")]
    #[serde(serialize_with = "rust_decimal::serde::str::serialize")]
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
/// retry does not return the original success. The response is the issued
/// [`Receipt`]; its PDF, when [`CreateReceipt::download_pdf`] is set, arrives
/// decoded in [`Receipt::pdf`].
/// Persist the unique call id before the first send and keep it for the logical
/// issuance. Recover by known number or a deliberately managed order, checking
/// identity, type and reversal data; unresolved recovery never justifies a new id.
/// See [recovery](crate::error#recovery).
///
/// HUF/Ft receipt items require whole gross, net/VAT with at most two decimal
/// places, and exact net + VAT = gross ([documented rules](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/item-amounts)).
/// `787.40 / 212.60 / 1000` is valid under those rules. `Rounding::Scale(2)`
/// alone does not ensure whole gross; HUF minor-unit rounding is a stricter
/// local choice. The calculator's arithmetic is not a server-acceptance check.
///
/// A receipt row carries fewer fields than an invoice row: a [`LineItem`]
/// with a `margin_vat_base`, or a ledger with an economic event or a
/// settlement period, is refused by [`validate`](AgentRequest::validate)
/// ([`RequestError::UnsupportedOnReceipt`]) rather than sent without them.
#[doc(alias = "xmlnyugtacreate")]
#[doc(alias = "nyugta készítés")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateReceipt {
    /// Unique call identifier (`hivasAzonosito`). Reusing it returns error 338,
    /// which prevents duplicate issuance but does not replay the prior result.
    /// Persist before first send; keep the same id throughout recovery.
    #[doc(alias = "hivasAzonosito")]
    pub call_id: Option<String>,
    /// Receipt number prefix (`elotag`), e.g. `NYGTA` → `NYGTA-2026-111`.
    ///
    /// On the test account (2026-09-11), code 337 also required at most five
    /// uppercase letters/digits; a new five-letter prefix was accepted without
    /// prior UI registration.
    #[doc(alias = "előtag")]
    pub prefix: String,
    /// Payment method (`fizmod`).
    pub payment_method: PaymentMethod,
    /// Currency (`penznem`).
    pub currency: Currency,
    /// Exchange rate; required when the currency is not HUF. Written as
    /// `devizabank` + `devizaarf` (the invoice operation spells these
    /// `arfolyamBank` + `arfolyam`). Automatic MNB lookup may omit the numeric
    /// rate; see [`ExchangeRate::automatic_mnb`] for receipt-specific provenance.
    pub exchange_rate: Option<ExchangeRate>,
    /// Free-text comment shown on the receipt (`megjegyzes`).
    pub comment: Option<String>,
    /// PDF template (`pdfSablon`).
    pub template: Option<ReceiptTemplate>,
    /// General-ledger identifier of the customer (`fokonyvVevo`).
    #[doc(alias = "fokonyvVevo")]
    pub ledger_customer: Option<String>,
    /// Order number shown on the receipt (`rendelesSzam`).
    /// Receipts have a [separate repetition toggle](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number)
    /// in account settings, independent of invoices. With restriction enabled,
    /// a previously used receipt order number is refused; when repetition is
    /// allowed, order queries return the last match. This is separate from
    /// creation call-ID duplicate prevention.
    #[doc(alias = "rendelésszám")]
    pub order_number: Option<String>,
    /// Return the PDF in the response (`pdfLetoltes`).
    #[serde(default)]
    pub download_pdf: bool,
    /// Line items (`tetelek`); at least one is required, and none may carry
    /// an invoice-only field (see the type's docs).
    pub items: Vec<LineItem>,
    /// How the buyer paid (`kifizetesek`), by tender; optional, but when
    /// present the docs require the amounts to sum to the receipt total. This
    /// crate does not validate that: the server is the authority.
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
            template: None,
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
    type Response = Receipt;

    fn validate(&self) -> Result<(), RequestError> {
        if self.items.is_empty() {
            return Err(RequestError::MissingLineItems);
        }
        if let Some(field) = self.items.iter().find_map(unsupported_on_receipt) {
            return Err(RequestError::UnsupportedOnReceipt(field));
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
                    if let Some(template) = &self.template {
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
/// `action-szamla_agent_nyugta_storno`): reverses an issued receipt.
///
/// The response is the newly issued storno receipt (`SN`,
/// [`ReceiptType::Storno`]), which names the reversed receipt in
/// [`Receipt::reversed_receipt_number`].
/// After a lost answer, query the known original's reversal state; this alone
/// does not recover the `SN` number/PDF. Keep the logical call identity. A
/// storno-specific 338 guarantee has not been established. The
/// [vendor documents refusals](https://docs.szamlazz.hu/agent/reversing_receipt/response)
/// for an already reversed receipt and for a target that is itself a storno
/// receipt, rather than invoice-style successful replay. That refusal does not
/// recover the `SN` number or identify who reversed it; see [recovery](crate::error#recovery).
#[doc(alias = "xmlnyugtast")]
#[doc(alias = "nyugta sztornó")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StornoReceipt {
    /// The receipt to reverse (`nyugtaszam`).
    pub receipt_number: ReceiptNumber,
    /// Return the storno receipt PDF in the response (`pdfLetoltes`).
    #[serde(default)]
    pub download_pdf: bool,
    /// PDF template (`pdfSablon`).
    pub template: Option<ReceiptTemplate>,
    /// Unique call identifier for the storno operation (`hivasAzonosito`).
    /// Keep it stable for the logical call; repeat semantics are not established
    /// as they are for receipt creation.
    pub call_id: Option<String>,
}

impl StornoReceipt {
    /// A reversal of the given receipt; no PDF is requested.
    pub fn new(receipt_number: impl Into<ReceiptNumber>) -> Self {
        Self {
            receipt_number: receipt_number.into(),
            download_pdf: false,
            template: None,
            call_id: None,
        }
    }
}

impl AgentRequest for StornoReceipt {
    const ACTION: &'static str = "action-szamla_agent_nyugta_storno";
    type Response = Receipt;

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
                    if let Some(template) = &self.template {
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
    /// Look up by order number (`rendelesSzam`): the last matching document,
    /// according to the [PHP docs](https://docs.szamlazz.hu/php/nyugta-lekerdezes).
    /// The exact “last” criterion and selection of `SN` remain unresolved;
    /// verify returned identity, type and reversal data before adopting it.
    /// See [`CreateReceipt::order_number`] for the receipt-specific repetition setting.
    #[doc(alias = "rendelésszám")]
    OrderNumber(String),
}

/// The receipt query operation (`xmlnyugtaget`,
/// `action-szamla_agent_nyugta_get`): fetches an issued receipt by receipt
/// number or order number.
/// Omit `call_id` on ordinary lookups; no call-ID-only lookup is offered.
/// See [receipt recovery](crate::error#recovery).
#[doc(alias = "xmlnyugtaget")]
#[doc(alias = "nyugta lekérdezés")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QueryReceipt {
    /// Which receipt to fetch.
    pub selector: ReceiptSelector,
    /// Return the PDF in the response (`pdfLetoltes`).
    #[serde(default)]
    pub download_pdf: bool,
    /// PDF template for the returned PDF (`pdfSablon`).
    pub template: Option<ReceiptTemplate>,
    /// Optional wire call identifier (`hivasAzonosito`), whose query behavior
    /// is unspecified. Omit for normal number/order lookups; it is not a selector.
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
            template: None,
            call_id: None,
        }
    }
}

impl AgentRequest for QueryReceipt {
    const ACTION: &'static str = "action-szamla_agent_nyugta_get";
    type Response = Receipt;

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
                    if let Some(template) = &self.template {
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
/// `None` omits a child, while `Some("")` writes an empty child. The documented
/// resend uses a present empty `emailKuldes` block (all details absent); independent
/// merging of partially supplied fields with previous details is not established.
/// Supply all fields and one recipient for a first send.
#[doc(alias = "email küldés")]
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct ReceiptEmail {
    /// Recipient address (`email`). Multi-recipient syntax is not established.
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
/// A lost acknowledgement leaves delivery unresolved: querying receipt existence
/// does not establish that email landed, and another send can duplicate it.
#[doc(alias = "xmlnyugtasend")]
#[doc(alias = "nyugta küldés")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SendReceipt {
    /// The receipt to email (`nyugtaszam`).
    pub receipt_number: ReceiptNumber,
    /// Email details; when `None`, a present empty `emailKuldes` block requests a
    /// resend using the previous email details.
    pub email: Option<ReceiptEmail>,
}

impl SendReceipt {
    /// A request to resend the previously used email for the given receipt.
    /// Set [`SendReceipt::email`] with all details for a first send.
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
        xml::verdict(
            response,
            "xmlnyugtasendvalasz",
            "http://www.szamlazz.hu/xmlnyugtasendvalasz",
        )
    }
}

/// A receipt as szamlazz.hu returns it (`xmlnyugtavalasz`): the `nyugta`
/// block, and the PDF beside it when one was requested. The reply of the
/// create, storno and query operations.
#[doc(alias = "nyugta")]
#[doc(alias = "xmlnyugtavalasz")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct Receipt {
    /// Internal szamlazz.hu identifier (`id`).
    pub id: i64,
    /// The call identifier supplied at creation (`hivasAzonosito`), if any.
    #[doc(alias = "hivasAzonosito")]
    pub call_id: Option<String>,
    /// The receipt number (`nyugtaszam`).
    #[doc(alias = "nyugtaszám")]
    pub receipt_number: ReceiptNumber,
    /// Document type (`tipus`): a receipt (`NY`) or a storno receipt (`SN`).
    #[doc(alias = "típus")]
    pub document_type: ReceiptType,
    /// Whether this receipt has been reversed (`stornozott`); meaningful for
    /// `NY` receipts.
    #[doc(alias = "stornózott")]
    pub reversed: bool,
    /// For `SN` receipts, the number of the receipt being reversed
    /// (`stornozottNyugtaszam`).
    pub reversed_receipt_number: Option<ReceiptNumber>,
    /// Issue date (`kelt`).
    pub issue_date: Date,
    /// Payment method (`fizmod`).
    pub payment_method: PaymentMethod,
    /// Currency (`penznem`).
    pub currency: Currency,
    /// Quoting bank for foreign-currency receipts (`devizabank`).
    pub exchange_bank: Option<String>,
    /// Exchange rate for foreign-currency receipts (`devizaarf`).
    #[serde(default, deserialize_with = "crate::number::de::optional")]
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
    pub exchange_rate: Option<Decimal>,
    /// Free-text comment (`megjegyzes`).
    pub comment: Option<String>,
    /// General-ledger identifier of the customer (`fokonyvVevo`).
    #[doc(alias = "fokonyvVevo")]
    pub ledger_customer: Option<String>,
    /// Whether a test account issued the receipt (`teszt`).
    ///
    /// Mirrors the wire: the schema has the element mandatory, so `None`
    /// (absent or empty) is a document that does not say, not a live one.
    pub test: Option<bool>,
    /// Order number (`rendelesSzam`).
    #[doc(alias = "rendelésszám")]
    pub order_number: Option<String>,
    /// Line items (`tetelek`).
    #[doc(alias = "tételek")]
    pub items: Vec<ReceiptItem>,
    /// How the buyer paid (`kifizetesek`), by tender.
    #[doc(alias = "kifizetések")]
    #[serde(default)]
    pub payments: Vec<ReceiptPayment>,
    /// Totals per VAT rate and overall (`osszegek`).
    #[doc(alias = "összegek")]
    pub totals: Totals,
    /// The receipt PDF (`nyugtaPdf`), when requested.
    #[serde(default)]
    pub pdf: Option<Pdf>,
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
    #[serde(deserialize_with = "crate::number::de::required")]
    #[serde(serialize_with = "rust_decimal::serde::str::serialize")]
    pub quantity: Decimal,
    /// Unit of measure (`mennyisegiEgyseg`), e.g. `db`.
    pub unit: String,
    /// Net unit price (`nettoEgysegar`).
    #[serde(deserialize_with = "crate::number::de::required")]
    #[serde(serialize_with = "rust_decimal::serde::str::serialize")]
    pub unit_price: Decimal,
    /// VAT category code (`afatipus`), set when a special code (AAM, EUT, …)
    /// applies.
    #[doc(alias = "áfatípus")]
    pub vat_type: Option<String>,
    /// Raw VAT rate token (`afakulcs`).
    #[doc(alias = "áfakulcs")]
    pub vat_rate_code: String,
    /// Net value (`netto`).
    #[serde(deserialize_with = "crate::number::de::required")]
    #[serde(serialize_with = "rust_decimal::serde::str::serialize")]
    pub net_value: Decimal,
    /// VAT value (`afa`).
    #[serde(deserialize_with = "crate::number::de::required")]
    #[serde(serialize_with = "rust_decimal::serde::str::serialize")]
    pub vat_value: Decimal,
    /// Gross value (`brutto`).
    #[serde(deserialize_with = "crate::number::de::required")]
    #[serde(serialize_with = "rust_decimal::serde::str::serialize")]
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
    /// [`ReceiptItem::vat_rate_code`].
    #[must_use]
    pub fn vat_rate(&self) -> VatRate {
        VatRate::from(self.vat_type.as_deref().unwrap_or(&self.vat_rate_code))
    }
}

/// The invoice-only [`LineItem`] field a receipt row cannot carry, if any:
/// the receipt writer has no element for it, and a field that never reaches
/// the wire is refused rather than dropped.
fn unsupported_on_receipt(item: &LineItem) -> Option<&'static str> {
    if item.margin_vat_base.is_some() {
        return Some("margin_vat_base");
    }
    let Some(LineItemLedger {
        economic_event,
        vat_economic_event,
        revenue_account: _,
        vat_account: _,
        settlement_from,
        settlement_to,
    }) = &item.ledger
    else {
        return None;
    };
    if economic_event.is_some() {
        return Some("ledger.economic_event");
    }
    if vat_economic_event.is_some() {
        return Some("ledger.vat_economic_event");
    }
    if settlement_from.is_some() {
        return Some("ledger.settlement_from");
    }
    if settlement_to.is_some() {
        return Some("ledger.settlement_to");
    }
    None
}

/// Parses an `xmlnyugtavalasz` body into a [`Receipt`]. Shared by the
/// create, storno, and query operations.
fn parse_receipt(response: &RawResponse) -> Result<Receipt, ResponseError> {
    let body: ReceiptBody = xml::valasz(response, VALASZ_ROOT, VALASZ_NAMESPACE)?;
    let nyugta = body.nyugta.ok_or(ParseError::Missing("nyugta"))?;
    let pdf = match body.nyugta_pdf.filter(|s| !s.trim().is_empty()) {
        Some(encoded) => Some(Pdf::from_base64(&encoded)?),
        None => None,
    };

    Ok(Receipt {
        pdf,
        ..nyugta.into()
    })
}

/// The payload of the `xmlnyugtavalasz` envelope after the verdict.
#[derive(Debug, serde::Deserialize)]
struct ReceiptBody {
    #[serde(default, rename(deserialize = "nyugtaPdf"))]
    nyugta_pdf: Option<String>,
    #[serde(default)]
    nyugta: Option<NyugtaXml>,
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
            document_type: alap.tipus,
            reversed: alap.stornozott,
            reversed_receipt_number: alap.stornozott_nyugtaszam.map(ReceiptNumber::new),
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
            pdf: None,
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct AlapXml {
    id: i64,
    #[serde(
        default,
        rename(deserialize = "hivasAzonosito"),
        deserialize_with = "xml::de::business_text"
    )]
    hivas_azonosito: Option<String>,
    nyugtaszam: String,
    tipus: ReceiptType,
    #[serde(deserialize_with = "xml::de::required_bool")]
    stornozott: bool,
    #[serde(
        default,
        rename(deserialize = "stornozottNyugtaszam"),
        deserialize_with = "xml::de::business_text"
    )]
    stornozott_nyugtaszam: Option<String>,
    #[serde(deserialize_with = "xml::de::date")]
    kelt: Date,
    fizmod: String,
    penznem: String,
    #[serde(default, deserialize_with = "xml::de::business_text")]
    devizabank: Option<String>,
    #[serde(default, deserialize_with = "xml::de::optional_decimal")]
    devizaarf: Option<Decimal>,
    #[serde(default, deserialize_with = "xml::de::business_text")]
    megjegyzes: Option<String>,
    #[serde(
        default,
        rename(deserialize = "fokonyvVevo"),
        deserialize_with = "xml::de::business_text"
    )]
    fokonyv_vevo: Option<String>,
    #[serde(default, deserialize_with = "xml::de::optional_flexible_bool")]
    teszt: Option<bool>,
    #[serde(
        default,
        rename(deserialize = "rendelesSzam"),
        deserialize_with = "xml::de::business_text"
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
    #[serde(default, deserialize_with = "xml::de::business_text")]
    azonosito: Option<String>,
    #[serde(deserialize_with = "xml::de::decimal")]
    mennyiseg: Decimal,
    #[serde(rename(deserialize = "mennyisegiEgyseg"))]
    mennyisegi_egyseg: String,
    #[serde(
        rename(deserialize = "nettoEgysegar"),
        deserialize_with = "xml::de::decimal"
    )]
    netto_egysegar: Decimal,
    #[serde(default, deserialize_with = "xml::de::business_text")]
    afatipus: Option<String>,
    afakulcs: String,
    #[serde(alias = "nettoErtek", deserialize_with = "xml::de::decimal")]
    netto: Decimal,
    #[serde(alias = "afaErtek", deserialize_with = "xml::de::decimal")]
    afa: Decimal,
    #[serde(alias = "bruttoErtek", deserialize_with = "xml::de::decimal")]
    brutto: Decimal,
    #[serde(default)]
    fokonyv: Option<TetelFokonyvXml>,
}

#[derive(Debug, serde::Deserialize)]
struct TetelFokonyvXml {
    #[serde(default, deserialize_with = "xml::de::business_text")]
    arbevetel: Option<String>,
    #[serde(default, deserialize_with = "xml::de::business_text")]
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
            vat_rate_code: tetel.afakulcs,
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
    #[serde(deserialize_with = "xml::de::decimal")]
    osszeg: Decimal,
    #[serde(default, deserialize_with = "xml::de::business_text")]
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
            template: None,
            ledger_customer: None,
            order_number: None,
            download_pdf: true,
            items: vec![
                LineItem::try_calculated(
                    "Kitten doormat",
                    dec!(2.0),
                    "db",
                    dec!(10000),
                    VatRate::percent(27),
                    crate::Rounding::minor_unit(&Currency::HUF),
                )
                .expect("fits"),
            ],
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

    #[test]
    fn receipt_operations_read_civil_dates() {
        for spelling in [
            "2024-02-29",
            "2024-02-29Z",
            "2024-02-29+01:30",
            "2024-02-29-02:00",
            "2024-02-29+00:00",
            "2024-02-29-00:00",
            "2024-02-29+14:00",
            "2024-02-29-14:00",
            " \t2024-02-29Z\n",
            "0000-02-29",
            "-000001-02-28",
            "20240229",
            "2023-02-29",
            "2024-02-29+14:01",
            "2024-02-29-15:00",
            "2024-02-29+01:60",
            "2024-02-29junk",
            "é123456789",
            "é",
            "",
        ] {
            let body = include_str!("../../tests/synthetic/xmlnyugtavalasz.xml")
                .replace("2026-01-01", spelling);
            let response = RawResponse::new::<&str, &str>([], body.into_bytes());
            for result in [
                create_sample().parse(&response),
                StornoReceipt::new("R-1").parse(&response),
                query_sample().parse(&response),
            ] {
                let expected = match spelling {
                    "0000-02-29" => Some(date(0, 2, 29)),
                    "-000001-02-28" => Some(date(-1, 2, 28)),
                    "2023-02-29" | "2024-02-29+14:01" | "2024-02-29-15:00" | "2024-02-29+01:60"
                    | "2024-02-29junk" | "é123456789" | "é" | "" => None,
                    _ => Some(date(2024, 2, 29)),
                };
                if let Some(expected) = expected {
                    assert_eq!(
                        result
                            .unwrap_or_else(|e| panic!("{spelling:?}: {e}"))
                            .issue_date,
                        expected
                    );
                } else {
                    assert!(result.is_err(), "{spelling}");
                }
            }
        }
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

    /// The forint in any letter case is not a foreign currency on a receipt
    /// either.
    #[test]
    fn lower_case_huf_receipt_needs_no_exchange_rate() {
        let mut receipt = create_sample();
        receipt.currency = Currency::new("ft");
        receipt.exchange_rate = None;
        assert!(receipt.to_wire(&Credentials::agent_key("key")).is_ok());
    }

    #[test]
    fn accepts_automatic_mnb_receipt_exchange_rate() {
        let mut receipt = create_sample();
        receipt.currency = Currency::EUR;
        receipt.exchange_rate = Some(ExchangeRate::automatic_mnb());
        let wire = receipt
            .to_wire(&Credentials::agent_key("key"))
            .expect("crate supports emitting automatic MNB lookup");
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

    /// A receipt row has no element for the invoice-only line item fields;
    /// a request carrying one is refused before the wire, naming the field,
    /// rather than sent without it.
    #[test]
    fn invoice_only_line_item_fields_are_refused() {
        let credentials = Credentials::agent_key("key");
        let refused = |item: LineItem| {
            let mut receipt = create_sample();
            receipt.items = vec![item];
            receipt.to_wire(&credentials).expect_err("refused")
        };
        let base = create_sample().items.remove(0);

        assert_eq!(
            refused(LineItem {
                margin_vat_base: Some(dec!(100)),
                ..base.clone()
            }),
            RequestError::UnsupportedOnReceipt("margin_vat_base")
        );
        for (field, ledger) in [
            (
                "ledger.economic_event",
                LineItemLedger {
                    economic_event: Some("SALE".into()),
                    ..LineItemLedger::default()
                },
            ),
            (
                "ledger.vat_economic_event",
                LineItemLedger {
                    vat_economic_event: Some("VAT".into()),
                    ..LineItemLedger::default()
                },
            ),
            (
                "ledger.settlement_from",
                LineItemLedger {
                    settlement_from: Some(date(2026, 7, 1)),
                    ..LineItemLedger::default()
                },
            ),
            (
                "ledger.settlement_to",
                LineItemLedger {
                    settlement_to: Some(date(2026, 7, 31)),
                    ..LineItemLedger::default()
                },
            ),
        ] {
            assert_eq!(
                refused(LineItem {
                    ledger: Some(ledger),
                    ..base.clone()
                }),
                RequestError::UnsupportedOnReceipt(field),
                "{field}"
            );
        }
        // The two ledger fields a receipt row does carry are accepted.
        let mut receipt = create_sample();
        receipt.items[0].ledger = Some(LineItemLedger {
            revenue_account: Some("911".into()),
            vat_account: Some("467".into()),
            ..LineItemLedger::default()
        });
        assert!(receipt.to_wire(&credentials).is_ok());
    }

    #[test]
    fn writes_canonical_storno_xml() {
        let storno = StornoReceipt {
            receipt_number: ReceiptNumber::new("NYGT-2026-1"),
            download_pdf: true,
            template: None,
            call_id: None,
        };
        let xml = storno.write_xml(&Credentials::agent_key("key"));
        let expected = include_str!("../../tests/golden/xmlnyugtast.xml").trim_end();
        assert_eq!(String::from_utf8(xml).expect("utf-8"), expected);
    }

    #[test]
    fn storno_writes_template() {
        let storno = StornoReceipt {
            receipt_number: ReceiptNumber::new("NYGT-2026-1"),
            download_pdf: false,
            template: Some(ReceiptTemplate::Ticket),
            call_id: None,
        };
        let xml =
            String::from_utf8(storno.write_xml(&Credentials::agent_key("key"))).expect("utf-8");
        assert!(xml.contains("<nyugtaszam>NYGT-2026-1</nyugtaszam><pdfSablon>J</pdfSablon>"));

        // An unknown token goes out verbatim: the set is open.
        let storno = StornoReceipt {
            template: Some(ReceiptTemplate::Other("Z".into())),
            ..StornoReceipt::new("NYGT-2026-1")
        };
        let xml =
            String::from_utf8(storno.write_xml(&Credentials::agent_key("key"))).expect("utf-8");
        assert!(xml.contains("<pdfSablon>Z</pdfSablon>"));
    }

    #[test]
    fn storno_writes_call_id_after_template() {
        let mut storno = StornoReceipt::new("NYGT-2026-1");
        storno.template = Some(ReceiptTemplate::Ticket);
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
            template: None,
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
            template: None,
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
        let receipt = query_sample().parse(&response).expect("success");
        assert_eq!(receipt.id, 123_456);
        assert_eq!(receipt.call_id, None);
        assert_eq!(receipt.receipt_number.as_str(), "NYGT-TST-2026-123");
        assert_eq!(receipt.document_type, ReceiptType::Receipt);
        assert!(!receipt.reversed);
        assert_eq!(
            receipt
                .reversed_receipt_number
                .as_ref()
                .map(ReceiptNumber::as_str),
            Some("NYGT-TST-2026-100")
        );
        assert_eq!(receipt.issue_date, date(2026, 1, 1));
        assert_eq!(receipt.payment_method, PaymentMethod::Other("cash".into()));
        assert_eq!(receipt.currency, Currency::EUR);
        assert_eq!(receipt.exchange_bank, None);
        assert_eq!(receipt.exchange_rate, Some(dec!(210)));
        assert_eq!(receipt.test, Some(false));
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
        assert_eq!(receipt.totals.by_vat_rate.len(), 1);
        assert_eq!(
            receipt.totals.by_vat_rate[0].vat_type.as_deref(),
            Some("ÁKK")
        );
        assert_eq!(receipt.totals.by_vat_rate[0].vat_rate(), VatRate::Akk);
        assert_eq!(receipt.totals.total.gross, dec!(254));
        assert!(receipt.pdf.is_none());
    }

    /// The totals block's lenient forms: an empty `afatipus` is no VAT type,
    /// and an `osszegek` carrying no `afakulcsossz` at all is a receipt with
    /// no per-rate subtotals; the grand total stands on its own.
    #[test]
    fn parses_totals_without_vat_type_or_rate_subtotals() {
        let receipt_with_subtotals = |subtotals: &str| {
            let body = format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
                 <xmlnyugtavalasz xmlns=\"http://www.szamlazz.hu/xmlnyugtavalasz\"><sikeres>true</sikeres>\
                 <nyugta><alap><id>1</id><nyugtaszam>NYGT-2026-1</nyugtaszam><tipus>NY</tipus>\
                 <stornozott>false</stornozott><kelt>2026-07-04</kelt><fizmod>készpénz</fizmod>\
                 <penznem>HUF</penznem><teszt>false</teszt></alap><tetelek></tetelek>\
                 <osszegek>{subtotals}\
                 <totalossz><netto>1000</netto><afa>270</afa><brutto>1270</brutto></totalossz></osszegek>\
                 </nyugta></xmlnyugtavalasz>"
            );
            let response = RawResponse::new::<&str, &str>([], body.into_bytes());
            query_sample().parse(&response).expect("success")
        };

        let receipt = receipt_with_subtotals(
            "<afakulcsossz><afatipus></afatipus><afakulcs>27</afakulcs>\
             <netto>1000</netto><afa>270</afa><brutto>1270</brutto></afakulcsossz>",
        );
        assert_eq!(receipt.totals.by_vat_rate.len(), 1);
        assert_eq!(receipt.totals.by_vat_rate[0].vat_type, None);
        assert_eq!(receipt.totals.by_vat_rate[0].vat_rate_code, "27");
        assert_eq!(
            receipt.totals.by_vat_rate[0].vat_rate(),
            VatRate::percent(27)
        );
        assert_eq!(receipt.totals.by_vat_rate[0].net, dec!(1000));
        assert_eq!(receipt.totals.by_vat_rate[0].vat, dec!(270));
        assert_eq!(receipt.totals.by_vat_rate[0].gross, dec!(1270));
        assert_eq!(receipt.totals.total.net, dec!(1000));
        assert_eq!(receipt.totals.total.vat, dec!(270));
        assert_eq!(receipt.totals.total.gross, dec!(1270));

        let receipt = receipt_with_subtotals("");
        assert!(receipt.totals.by_vat_rate.is_empty());
        assert_eq!(receipt.totals.total.gross, dec!(1270));
    }

    /// `<teszt>` mirrors the wire, as on the invoice query: mandatory in the
    /// schema, so absent or empty is `None` (unknown, never `false` = live).
    #[test]
    fn test_marker_mirrors_the_wire() {
        let receipt_with_teszt = |element: &str| {
            let body = format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
                 <xmlnyugtavalasz xmlns=\"http://www.szamlazz.hu/xmlnyugtavalasz\"><sikeres>true</sikeres>\
                 <nyugta><alap><id>1</id><nyugtaszam>NYGT-2026-1</nyugtaszam><tipus>NY</tipus>\
                 <stornozott>false</stornozott><kelt>2026-07-04</kelt><fizmod>készpénz</fizmod>\
                 <penznem>HUF</penznem>{element}</alap><tetelek></tetelek>\
                 <osszegek>\
                 <totalossz><netto>1000</netto><afa>270</afa><brutto>1270</brutto></totalossz></osszegek>\
                 </nyugta></xmlnyugtavalasz>"
            );
            let response = RawResponse::new::<&str, &str>([], body.into_bytes());
            query_sample().parse(&response).expect("success")
        };

        for (element, expected) in [
            ("<teszt>true</teszt>", Some(true)),
            ("<teszt>0</teszt>", Some(false)),
            ("<teszt></teszt>", None),
            ("", None),
        ] {
            assert_eq!(
                receipt_with_teszt(element).test,
                expected,
                "element {element:?}"
            );
        }
    }

    #[test]
    fn decodes_receipt_pdf() {
        let body = r#"<?xml version="1.0" encoding="UTF-8"?><xmlnyugtavalasz xmlns="http://www.szamlazz.hu/xmlnyugtavalasz"><sikeres>true</sikeres><nyugtaPdf>JVBERi0=</nyugtaPdf><nyugta><alap><id>1</id><nyugtaszam>NYGT-2026-1</nyugtaszam><tipus>NY</tipus><stornozott>false</stornozott><kelt>2026-07-04</kelt><fizmod>készpénz</fizmod><penznem>HUF</penznem><teszt>false</teszt></alap><tetelek><tetel><megnevezes>Kitten doormat</megnevezes><mennyiseg>2.0</mennyiseg><mennyisegiEgyseg>db</mennyisegiEgyseg><nettoEgysegar>10000</nettoEgysegar><afakulcs>27</afakulcs><netto>20000.0</netto><afa>5400.0</afa><brutto>25400.0</brutto></tetel></tetelek><osszegek><afakulcsossz><afakulcs>27</afakulcs><netto>20000.0</netto><afa>5400.0</afa><brutto>25400.0</brutto></afakulcsossz><totalossz><netto>20000.0</netto><afa>5400.0</afa><brutto>25400.0</brutto></totalossz></osszegek></nyugta></xmlnyugtavalasz>"#;
        let response = RawResponse::new::<&str, &str>([], body.as_bytes().to_vec());
        let receipt = create_sample().parse(&response).expect("success");
        assert_eq!(receipt.pdf.expect("pdf").as_bytes(), b"%PDF-");
    }

    /// The receipt is journal-safe: it round-trips through JSON with the
    /// payment method, currency and type as their wire tokens and the PDF as
    /// base64.
    #[test]
    fn receipt_round_trips_through_json() {
        let body = include_str!("../../tests/synthetic/xmlnyugtavalasz.xml").replace(
            "<sikeres>true</sikeres>",
            "<sikeres>true</sikeres><nyugtaPdf>JVBERi0=</nyugtaPdf>",
        );
        let response = RawResponse::new::<&str, &str>([], body.into_bytes());
        let receipt = query_sample().parse(&response).expect("success");
        assert!(receipt.pdf.is_some(), "the fixture carries a PDF");

        let json = serde_json::to_value(&receipt).expect("serialize");
        assert_eq!(json["receipt_number"], "NYGT-TST-2026-123");
        assert_eq!(json["document_type"], "NY");
        assert_eq!(json["payment_method"], "cash");
        assert_eq!(json["currency"], "EUR");
        assert_eq!(json["items"][0]["vat_rate_code"], "27");
        assert_eq!(json["totals"]["by_vat_rate"][0]["vat_type"], "ÁKK");
        assert_eq!(json["pdf"], "JVBERi0=");

        let restored: Receipt = serde_json::from_value(json).expect("deserialize");
        assert_eq!(restored, receipt);
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
