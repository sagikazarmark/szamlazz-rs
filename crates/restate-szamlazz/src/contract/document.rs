//! The per-call document input: buyer, line items, payment terms and the few
//! per-call overrides of the configured defaults.
//!
//! These types are a *projection* of the Számla Agent model: account
//! constants live on the resolved [`Account`](crate::account::Account) (its
//! [`Defaults`](crate::config::Defaults) and seller block), line totals are
//! computed here, and the payment method is an English enum. Each type
//! converts into its `szamlazz_agent` counterpart.

use jiff::civil::Date;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use szamlazz_agent::ops::invoice::{Buyer, ExchangeRate, PostalAddress};
use szamlazz_agent::{Currency, LineItem, VatRate};

/// One document to issue: everything the caller decides per call.
///
/// The order number is not part of the input — it is the `Order` key. Account
/// data, the seller block and the defaults come from the configuration;
/// [`DocumentInput::overrides`] can change a subset of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct DocumentInput {
    /// The buyer (`vevő`).
    pub buyer: BuyerInput,
    /// Line items (`tételek`); at least one is required.
    pub items: Vec<LineItemInput>,
    /// Fulfillment date (`teljesítés dátuma`).
    pub fulfillment_date: Date,
    /// Payment due date (`fizetési határidő`).
    pub due_date: Date,
    /// Payment method (`fizetési mód`).
    pub payment_method: PaymentMethod,
    /// Marks the document as already paid (`fizetve`).
    #[serde(default)]
    pub paid: bool,
    /// Free-text comment shown on the document (`megjegyzés`).
    #[serde(default)]
    pub comment: Option<String>,
    /// Issue date (`kelt`). Leave unset to let szamlazz.hu date the document
    /// at issue time; a pinned date is journaled and re-sent unchanged on
    /// every attempt.
    #[serde(default)]
    pub issue_date: Option<Date>,
    /// Per-call overrides of the configured defaults.
    #[serde(default)]
    pub overrides: DocumentOverrides,
}

impl DocumentInput {
    /// A document with the required fields; `paid` is `false` and the optional
    /// fields are absent.
    #[must_use]
    pub fn new(
        buyer: BuyerInput,
        items: Vec<LineItemInput>,
        fulfillment_date: Date,
        due_date: Date,
        payment_method: PaymentMethod,
    ) -> Self {
        Self {
            buyer,
            items,
            fulfillment_date,
            due_date,
            payment_method,
            paid: false,
            comment: None,
            issue_date: None,
            overrides: DocumentOverrides::default(),
        }
    }
}

/// Per-call overrides of the configured [`Defaults`](crate::config::Defaults).
///
/// Every field is optional; an absent field keeps the configured value.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(default)]
pub struct DocumentOverrides {
    /// Document language as a szamlazz.hu code (`hu`, `en`, `de`, …).
    pub language: Option<String>,
    /// Currency code (`HUF`, `EUR`, …). A non-HUF currency needs an exchange
    /// rate, either here or through the configured bank.
    pub currency: Option<String>,
    /// Exchange rate for non-HUF documents.
    pub exchange_rate: Option<ExchangeRateInput>,
    /// PDF template token (`SzlaAlap`, `SzlaMost`, …) or one of the
    /// `szamlazz_agent::ops::invoice::InvoiceTemplate` names.
    pub template: Option<String>,
    /// Whether szamlazz.hu should email the document to the buyer.
    pub send_email: Option<bool>,
    /// Issue an e-invoice (`e-számla`).
    pub e_invoice: Option<bool>,
    /// Invoice number prefix (`számlaszám előtag`), pre-registered on the
    /// account.
    pub number_prefix: Option<String>,
}

/// Exchange rate information (`árfolyam`) for non-HUF documents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ExchangeRateInput {
    /// The quoting bank (`árfolyam bank`), e.g. `MNB`.
    pub bank: String,
    /// The rate. May be omitted only for `MNB`, which selects szamlazz.hu's
    /// automatic current-rate lookup.
    #[serde(default)]
    pub rate: Option<Decimal>,
}

impl From<ExchangeRateInput> for ExchangeRate {
    fn from(input: ExchangeRateInput) -> Self {
        if let Some(rate) = input.rate {
            return Self::new(input.bank, rate);
        }
        let mut rate = Self::automatic_mnb();
        rate.bank = input.bank;
        rate
    }
}

/// The buyer (`vevő`) of a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct BuyerInput {
    /// Name (`név`). Normalised (trimmed, NFC) before issuing so every attempt
    /// sends byte-identical bytes.
    pub name: String,
    /// ZIP code (`irányítószám`).
    pub zip: String,
    /// City (`település`).
    pub city: String,
    /// Street address (`cím`).
    pub address: String,
    /// Country (`ország`).
    #[serde(default)]
    pub country: Option<String>,
    /// Email address; multiple recipients may be comma-separated.
    #[serde(default)]
    pub email: Option<String>,
    /// Hungarian tax number (`adószám`).
    #[serde(default)]
    pub tax_number: Option<String>,
    /// EU tax number (`EU adószám`).
    #[serde(default)]
    pub eu_tax_number: Option<String>,
    /// VAT-group identifier (`csoportazonosító`).
    #[serde(default)]
    pub group_id: Option<String>,
    /// Taxpayer status reported to NAV (`adóalany`).
    #[serde(default)]
    pub taxpayer_status: Option<TaxpayerStatus>,
    /// Phone number.
    #[serde(default)]
    pub phone: Option<String>,
    /// Buyer comment (`megjegyzés`).
    #[serde(default)]
    pub comment: Option<String>,
    /// Postal address, when it differs from the billing address.
    #[serde(default)]
    pub postal_address: Option<PostalAddressInput>,
    /// Partner identifier from the account's partner database (`azonosító`).
    #[serde(default)]
    pub id: Option<String>,
}

impl BuyerInput {
    /// A buyer with the required fields; optional fields default to absent.
    pub fn new(
        name: impl Into<String>,
        zip: impl Into<String>,
        city: impl Into<String>,
        address: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            zip: zip.into(),
            city: city.into(),
            address: address.into(),
            country: None,
            email: None,
            tax_number: None,
            eu_tax_number: None,
            group_id: None,
            taxpayer_status: None,
            phone: None,
            comment: None,
            postal_address: None,
            id: None,
        }
    }
}

impl From<BuyerInput> for Buyer {
    fn from(input: BuyerInput) -> Self {
        let mut buyer = Self::new(input.name, input.zip, input.city, input.address);
        buyer.country = input.country;
        buyer.email = input.email;
        buyer.tax_number = input.tax_number;
        buyer.eu_tax_number = input.eu_tax_number;
        buyer.group_id = input.group_id;
        buyer.taxpayer_status = input.taxpayer_status.map(Into::into);
        buyer.phone = input.phone;
        buyer.comment = input.comment;
        buyer.postal_address = input.postal_address.map(Into::into);
        buyer.id = input.id;
        buyer
    }
}

/// Postal/delivery address of the buyer (`postázási cím`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(default)]
pub struct PostalAddressInput {
    /// Recipient name.
    pub name: Option<String>,
    /// Country.
    pub country: Option<String>,
    /// ZIP code.
    pub zip: Option<String>,
    /// City.
    pub city: Option<String>,
    /// Street address.
    pub address: Option<String>,
}

impl From<PostalAddressInput> for PostalAddress {
    fn from(input: PostalAddressInput) -> Self {
        let mut address = Self::default();
        address.name = input.name;
        address.country = input.country;
        address.zip = input.zip;
        address.city = input.city;
        address.address = input.address;
        address
    }
}

/// The buyer's taxpayer status (`adóalany`), reported to NAV.
///
/// Mirrors [`szamlazz_agent::TaxpayerStatus`] with snake-case JSON tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum TaxpayerStatus {
    /// Business outside the EU (`7`).
    NonEuBusiness,
    /// Business in another EU member state (`6`).
    EuBusiness,
    /// Has a Hungarian tax number (`1`).
    HasTaxNumber,
    /// Unknown (`0`).
    Unknown,
    /// No tax number — a private individual (`-1`).
    NoTaxNumber,
}

impl From<TaxpayerStatus> for szamlazz_agent::TaxpayerStatus {
    fn from(status: TaxpayerStatus) -> Self {
        match status {
            TaxpayerStatus::NonEuBusiness => Self::NonEuBusiness,
            TaxpayerStatus::EuBusiness => Self::EuBusiness,
            TaxpayerStatus::HasTaxNumber => Self::HasTaxNumber,
            TaxpayerStatus::Unknown => Self::Unknown,
            TaxpayerStatus::NoTaxNumber => Self::NoTaxNumber,
        }
    }
}

/// One row of a document (`tétel`).
///
/// Net, VAT and gross values are not part of the input: the service computes
/// them with [`LineItem::calculated_for_currency`] so that the arithmetic
/// szamlazz.hu verifies server-side always holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct LineItemInput {
    /// Item name (`megnevezés`).
    pub name: String,
    /// Quantity (`mennyiség`).
    pub quantity: Decimal,
    /// Unit of measure (`mennyiségi egység`), e.g. `db`.
    pub unit: String,
    /// Net unit price (`nettó egységár`).
    pub unit_price: Decimal,
    /// VAT rate (`áfakulcs`): a numeric percentage such as `27` or a
    /// NAV-defined code such as `AAM`. The code set is open.
    pub vat_rate: String,
    /// Account-side item identifier (`azonosító`).
    #[serde(default)]
    pub id: Option<String>,
    /// Free-text comment for the row (`megjegyzés`).
    #[serde(default)]
    pub comment: Option<String>,
}

impl LineItemInput {
    /// A line item with the required fields; `id` and `comment` are absent.
    pub fn new(
        name: impl Into<String>,
        quantity: Decimal,
        unit: impl Into<String>,
        unit_price: Decimal,
        vat_rate: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            quantity,
            unit: unit.into(),
            unit_price,
            vat_rate: vat_rate.into(),
            id: None,
            comment: None,
        }
    }

    /// The VAT rate as the Agent's open enum.
    #[must_use]
    pub fn vat_rate(&self) -> VatRate {
        VatRate::from(self.vat_rate.as_str())
    }

    /// The Agent line item with net, VAT and gross computed for `currency`
    /// (whole forints for HUF, exact decimals otherwise).
    #[must_use]
    pub fn to_line_item(&self, currency: &Currency) -> LineItem {
        let mut item = LineItem::calculated_for_currency(
            self.name.clone(),
            self.quantity,
            self.unit.clone(),
            self.unit_price,
            self.vat_rate(),
            currency,
        );
        item.id.clone_from(&self.id);
        item.comment.clone_from(&self.comment);
        item
    }
}

/// A payment method (`fizetési mód`) as an English enum.
///
/// Serialises as a snake-case token (`"transfer"`, `"cash_on_delivery"`, …);
/// a free-text method szamlazz.hu does not map for NAV is
/// `{"other": "…"}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum PaymentMethod {
    /// Bank transfer (`átutalás`).
    Transfer,
    /// Cash (`készpénz`).
    Cash,
    /// Card payment (`bankkártya`).
    Card,
    /// Check (`csekk`).
    Check,
    /// Cash on delivery (`utánvét`).
    CashOnDelivery,
    /// `PayPal`.
    PayPal,
    /// SZÉP card (`SZÉP kártya`).
    SzepCard,
    /// Any other free-text payment method, sent to szamlazz.hu verbatim.
    Other(String),
}

impl From<PaymentMethod> for szamlazz_agent::PaymentMethod {
    fn from(method: PaymentMethod) -> Self {
        match method {
            PaymentMethod::Transfer => Self::Transfer,
            PaymentMethod::Cash => Self::Cash,
            PaymentMethod::Card => Self::Card,
            PaymentMethod::Check => Self::Check,
            PaymentMethod::CashOnDelivery => Self::CashOnDelivery,
            PaymentMethod::PayPal => Self::PayPal,
            PaymentMethod::SzepCard => Self::SzepCard,
            PaymentMethod::Other(method) => Self::Other(method),
        }
    }
}

/// Maps the Agent's known methods to their variants; anything else — including
/// variants added to the Agent later — becomes [`PaymentMethod::Other`] with
/// the wire token.
impl From<szamlazz_agent::PaymentMethod> for PaymentMethod {
    fn from(method: szamlazz_agent::PaymentMethod) -> Self {
        match method {
            szamlazz_agent::PaymentMethod::Transfer => Self::Transfer,
            szamlazz_agent::PaymentMethod::Cash => Self::Cash,
            szamlazz_agent::PaymentMethod::Card => Self::Card,
            szamlazz_agent::PaymentMethod::Check => Self::Check,
            szamlazz_agent::PaymentMethod::CashOnDelivery => Self::CashOnDelivery,
            szamlazz_agent::PaymentMethod::PayPal => Self::PayPal,
            szamlazz_agent::PaymentMethod::SzepCard => Self::SzepCard,
            szamlazz_agent::PaymentMethod::Other(method) => Self::Other(method),
            other => Self::Other(other.as_wire().to_owned()),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use jiff::civil::date;
    use rust_decimal::dec;
    use serde_json::json;

    use super::*;

    pub(crate) fn sample_document() -> DocumentInput {
        let mut buyer = BuyerInput::new("Kovács Bt.", "2030", "Érd", "Tárnoki út 23.");
        buyer.email = Some("buyer@example.com".to_owned());
        buyer.taxpayer_status = Some(TaxpayerStatus::HasTaxNumber);
        buyer.tax_number = Some("12345678-2-42".to_owned());
        let mut item = LineItemInput::new("Elado izé", dec!(2), "db", dec!(10000), "27");
        item.comment = Some("row".to_owned());
        let mut document = DocumentInput::new(
            buyer,
            vec![item],
            date(2026, 7, 4),
            date(2026, 7, 12),
            PaymentMethod::Transfer,
        );
        document.comment = Some("thanks".to_owned());
        document
    }

    #[test]
    fn document_input_round_trips() {
        let document = sample_document();
        let json = serde_json::to_value(&document).expect("serialize");
        assert_eq!(json["payment_method"], "transfer");
        assert_eq!(json["fulfillment_date"], "2026-07-04");
        assert_eq!(json["paid"], false);
        let back: DocumentInput = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, document);
    }

    #[test]
    fn document_input_defaults_optional_fields() {
        let document: DocumentInput = serde_json::from_value(json!({
            "buyer": {"name": "A", "zip": "1", "city": "B", "address": "C"},
            "items": [{"name": "x", "quantity": "1", "unit": "db", "unit_price": "100", "vat_rate": "AAM"}],
            "fulfillment_date": "2026-07-04",
            "due_date": "2026-07-12",
            "payment_method": {"other": "Bitcoin"},
        }))
        .expect("deserialize");
        assert!(!document.paid);
        assert_eq!(document.overrides, DocumentOverrides::default());
        assert_eq!(
            document.payment_method,
            PaymentMethod::Other("Bitcoin".to_owned())
        );
        assert_eq!(document.items[0].vat_rate(), VatRate::Aam);
    }

    #[test]
    fn overrides_round_trip() {
        let overrides = DocumentOverrides {
            language: Some("en".to_owned()),
            currency: Some("EUR".to_owned()),
            exchange_rate: Some(ExchangeRateInput {
                bank: "MNB".to_owned(),
                rate: None,
            }),
            template: Some("SzlaMost".to_owned()),
            send_email: Some(false),
            e_invoice: Some(true),
            number_prefix: Some("WEB".to_owned()),
        };
        let json = serde_json::to_string(&overrides).expect("serialize");
        assert_eq!(
            serde_json::from_str::<DocumentOverrides>(&json).expect("deserialize"),
            overrides
        );
        assert_eq!(
            serde_json::from_str::<DocumentOverrides>("{}").expect("deserialize"),
            DocumentOverrides::default()
        );
    }

    #[test]
    fn payment_method_wire_shapes() {
        assert_eq!(
            serde_json::to_value(PaymentMethod::CashOnDelivery).expect("serialize"),
            json!("cash_on_delivery")
        );
        assert_eq!(
            serde_json::to_value(PaymentMethod::Other("Bitcoin".to_owned())).expect("serialize"),
            json!({"other": "Bitcoin"})
        );
        assert_eq!(
            serde_json::from_value::<PaymentMethod>(json!("szep_card")).expect("deserialize"),
            PaymentMethod::SzepCard
        );
    }

    #[test]
    fn payment_method_converts_to_and_from_agent() {
        let cases = [
            (PaymentMethod::Transfer, "átutalás"),
            (PaymentMethod::Cash, "készpénz"),
            (PaymentMethod::Card, "bankkártya"),
            (PaymentMethod::Check, "csekk"),
            (PaymentMethod::CashOnDelivery, "utánvét"),
            (PaymentMethod::PayPal, "PayPal"),
            (PaymentMethod::SzepCard, "SZÉP kártya"),
            (PaymentMethod::Other("Bitcoin".to_owned()), "Bitcoin"),
        ];
        for (method, wire) in cases {
            let agent = szamlazz_agent::PaymentMethod::from(method.clone());
            assert_eq!(agent.as_wire(), wire);
            assert_eq!(PaymentMethod::from(agent), method);
        }
    }

    #[test]
    fn taxpayer_status_converts_to_agent() {
        let cases = [
            (TaxpayerStatus::NonEuBusiness, "7", "non_eu_business"),
            (TaxpayerStatus::EuBusiness, "6", "eu_business"),
            (TaxpayerStatus::HasTaxNumber, "1", "has_tax_number"),
            (TaxpayerStatus::Unknown, "0", "unknown"),
            (TaxpayerStatus::NoTaxNumber, "-1", "no_tax_number"),
        ];
        for (status, wire, token) in cases {
            assert_eq!(szamlazz_agent::TaxpayerStatus::from(status).as_wire(), wire);
            assert_eq!(
                serde_json::to_value(status).expect("serialize"),
                json!(token)
            );
        }
    }

    #[test]
    fn buyer_converts_to_agent() {
        let mut input = BuyerInput::new(" Kovács Bt.", "2030", "Érd", "Tárnoki út 23.");
        input.country = Some("Magyarország".to_owned());
        input.email = Some("a@b.hu".to_owned());
        input.taxpayer_status = Some(TaxpayerStatus::NoTaxNumber);
        input.postal_address = Some(PostalAddressInput {
            name: Some("Kovács János".to_owned()),
            city: Some("Budapest".to_owned()),
            ..PostalAddressInput::default()
        });
        input.id = Some("P-1".to_owned());
        let buyer = Buyer::from(input);
        assert_eq!(buyer.name, " Kovács Bt.");
        assert_eq!(buyer.country.as_deref(), Some("Magyarország"));
        assert_eq!(buyer.email.as_deref(), Some("a@b.hu"));
        assert_eq!(buyer.send_email, None);
        assert_eq!(
            buyer.taxpayer_status,
            Some(szamlazz_agent::TaxpayerStatus::NoTaxNumber)
        );
        let postal = buyer.postal_address.expect("postal address");
        assert_eq!(postal.name.as_deref(), Some("Kovács János"));
        assert_eq!(postal.city.as_deref(), Some("Budapest"));
        assert_eq!(postal.zip, None);
        assert_eq!(buyer.id.as_deref(), Some("P-1"));
    }

    #[test]
    fn line_item_computes_totals_for_currency() {
        let mut input = LineItemInput::new("x", dec!(3), "db", dec!(33.335), "27");
        input.id = Some("SKU-1".to_owned());
        let huf = input.to_line_item(&Currency::HUF);
        assert_eq!(huf.net_value, dec!(100));
        assert_eq!(huf.vat_value, dec!(27));
        assert_eq!(huf.gross_value, dec!(127));
        assert_eq!(huf.id.as_deref(), Some("SKU-1"));
        assert_eq!(huf.vat_rate, VatRate::percent(27));
        let eur = input.to_line_item(&Currency::EUR);
        assert_eq!(eur.net_value, dec!(100.005));
    }

    #[test]
    fn exchange_rate_converts_to_agent() {
        let fixed = ExchangeRate::from(ExchangeRateInput {
            bank: "OTP".to_owned(),
            rate: Some(dec!(395.5)),
        });
        assert_eq!(fixed.bank, "OTP");
        assert_eq!(fixed.rate, Some(dec!(395.5)));
        let automatic = ExchangeRate::from(ExchangeRateInput {
            bank: "MNB".to_owned(),
            rate: None,
        });
        assert_eq!(automatic, ExchangeRate::automatic_mnb());
    }
}
