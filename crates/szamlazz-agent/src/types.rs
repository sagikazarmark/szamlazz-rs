//! Domain value types shared across operations.

use std::borrow::Cow;
use std::convert::Infallible;
use std::fmt;
use std::str::FromStr;

use rust_decimal::Decimal;

use crate::error::ParseError;

/// An invoice number (`számlaszám`), e.g. `E-2026-123`.
#[doc(alias = "számlaszám")]
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct InvoiceNumber(String);

impl InvoiceNumber {
    /// Wraps an invoice number.
    pub fn new(number: impl Into<String>) -> Self {
        Self(number.into())
    }

    /// The number as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for InvoiceNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for InvoiceNumber {
    fn from(number: &str) -> Self {
        Self::new(number)
    }
}

impl From<String> for InvoiceNumber {
    fn from(number: String) -> Self {
        Self::new(number)
    }
}

/// A receipt number (`nyugtaszám`), e.g. `NYGTA-2026-1`.
#[doc(alias = "nyugtaszám")]
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct ReceiptNumber(String);

impl ReceiptNumber {
    /// Wraps a receipt number.
    pub fn new(number: impl Into<String>) -> Self {
        Self(number.into())
    }

    /// The number as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ReceiptNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for ReceiptNumber {
    fn from(number: &str) -> Self {
        Self::new(number)
    }
}

impl From<String> for ReceiptNumber {
    fn from(number: String) -> Self {
        Self::new(number)
    }
}

/// A PDF document returned by szamlazz.hu, already base64-decoded.
///
/// Response types carry `Pdf` values holding raw bytes. Base64 is exposed only
/// at explicit wire-conversion boundaries such as [`Pdf::from_base64`] and
/// serde serialization.
#[derive(Clone, PartialEq, Eq)]
pub struct Pdf(Vec<u8>);

impl Pdf {
    /// Decodes a base64 payload as received in response XML.
    ///
    /// # Errors
    ///
    /// Returns an error if `encoded` is not valid standard base64.
    pub fn from_base64(encoded: &str) -> Result<Self, ParseError> {
        use base64::Engine as _;
        // szamlazz.hu wraps base64 payloads in whitespace/newlines.
        let compact: String = encoded.split_whitespace().collect();
        let bytes = base64::engine::general_purpose::STANDARD.decode(compact)?;

        Ok(Self(bytes))
    }

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

/// Serializes as a base64 string — the wire representation.
impl serde::Serialize for Pdf {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use base64::Engine as _;
        serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(&self.0))
    }
}

/// Deserializes from a base64 string — the wire representation.
impl<'de> serde::Deserialize<'de> for Pdf {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let encoded = String::deserialize(deserializer)?;
        Self::from_base64(&encoded).map_err(serde::de::Error::custom)
    }
}

impl AsRef<[u8]> for Pdf {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Wraps raw PDF bytes.
impl From<Vec<u8>> for Pdf {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

/// Unwraps the raw PDF bytes.
impl From<Pdf> for Vec<u8> {
    fn from(pdf: Pdf) -> Self {
        pdf.0
    }
}

/// A VAT rate (`áfakulcs`): a numeric percentage or a NAV-defined special code.
///
/// The code set is NAV-driven and changes over time, so the enum is open:
/// unknown codes round-trip through [`VatRate::Other`]. Doc comments give the
/// NAV meaning of each code; consult a tax advisor for which one applies.
#[doc(alias = "áfakulcs")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum VatRate {
    /// A numeric percentage (27, 18, 5, 0, or fractional foreign rates such as
    /// 5.5).
    Percent(Decimal),
    /// `AAM` — alanyi adómentes (subjective/personal VAT exemption).
    Aam,
    /// `TAM` — tárgyi adómentes (objective exemption / exempt activity).
    Tam,
    /// `TAHK` — tárgyi adómentes, a tevékenység közérdekű vagy sajátos
    /// jellegére tekintettel (exempt due to public-interest or special nature).
    Tahk,
    /// `EUT` — EU-n belüli ügylet (intra-EU transaction).
    Eut,
    /// `EUKT` — EU-n kívüli ügylet (transaction outside the EU).
    Eukt,
    /// `F.AFA` — fordított áfa (domestic reverse charge).
    FAfa,
    /// `K.AFA` — különbözet szerinti áfa (margin scheme).
    KAfa,
    /// `HO` — területi hatályon kívüli (outside the territorial scope of the
    /// Hungarian VAT act).
    Ho,
    /// `EUE` — EU-n belüli, másik tagállamban teljesített ügylet.
    Eue,
    /// `EUFADE` — EU-n belüli fordított adózású ügylet (intra-EU reverse
    /// charge, not under §37).
    Eufade,
    /// `EUFAD37` — az Áfa tv. 37.§-a alapján EU-n belüli fordított adózású
    /// ügylet (intra-EU reverse charge under §37; requires an EU tax number).
    Eufad37,
    /// `ATK` — áfa tárgyi hatályán kívüli (outside the scope of VAT).
    Atk,
    /// `NAM` — nemzetközi ügyletekhez kapcsolódó adómentesség (exemption for
    /// other international transactions).
    Nam,
    /// `EAM` — adómentes termékexport harmadik országba (exempt export to a
    /// third country).
    Eam,
    /// `KBAUK` — közösségen belüli adómentes új közlekedési eszköz értékesítés
    /// (intra-EU exempt sale of new means of transport).
    Kbauk,
    /// `KBAET` — közösségen belüli adómentes termékértékesítés (intra-EU
    /// exempt supply of goods; requires an EU tax number).
    Kbaet,
    /// `ÁKK` — áfakörön kívüli (outside the VAT system; seen on receipts).
    Akk,
    /// `EU` — EU-n belüli értékesítés (intra-EU sale; receipt code list).
    Eu,
    /// `EUK` — EU-n kívüli értékesítés (sale outside the EU; receipt code
    /// list).
    Euk,
    /// `MAA` — mentes az adó alól (exempt from tax; receipt code list).
    Maa,
    /// Any other code accepted by szamlazz.hu/NAV.
    Other(String),
}

impl VatRate {
    /// A percentage rate from an integer, e.g. `VatRate::percent(27)`.
    pub fn percent(value: impl Into<Decimal>) -> Self {
        Self::Percent(value.into())
    }

    /// The exact wire token.
    #[must_use]
    pub fn as_wire(&self) -> Cow<'_, str> {
        match self {
            Self::Percent(rate) => Cow::Owned(rate.to_string()),
            Self::Aam => Cow::Borrowed("AAM"),
            Self::Tam => Cow::Borrowed("TAM"),
            Self::Tahk => Cow::Borrowed("TAHK"),
            Self::Eut => Cow::Borrowed("EUT"),
            Self::Eukt => Cow::Borrowed("EUKT"),
            Self::FAfa => Cow::Borrowed("F.AFA"),
            Self::KAfa => Cow::Borrowed("K.AFA"),
            Self::Ho => Cow::Borrowed("HO"),
            Self::Eue => Cow::Borrowed("EUE"),
            Self::Eufade => Cow::Borrowed("EUFADE"),
            Self::Eufad37 => Cow::Borrowed("EUFAD37"),
            Self::Atk => Cow::Borrowed("ATK"),
            Self::Nam => Cow::Borrowed("NAM"),
            Self::Eam => Cow::Borrowed("EAM"),
            Self::Kbauk => Cow::Borrowed("KBAUK"),
            Self::Kbaet => Cow::Borrowed("KBAET"),
            Self::Akk => Cow::Borrowed("ÁKK"),
            Self::Eu => Cow::Borrowed("EU"),
            Self::Euk => Cow::Borrowed("EUK"),
            Self::Maa => Cow::Borrowed("MAA"),
            Self::Other(code) => Cow::Borrowed(code),
        }
    }
}

/// Parses a wire token: known codes map to their variant, numbers to
/// [`VatRate::Percent`], anything else to [`VatRate::Other`].
impl From<&str> for VatRate {
    fn from(token: &str) -> Self {
        match token {
            "AAM" => Self::Aam,
            "TAM" => Self::Tam,
            "TAHK" => Self::Tahk,
            "EUT" => Self::Eut,
            "EUKT" => Self::Eukt,
            "F.AFA" => Self::FAfa,
            "K.AFA" => Self::KAfa,
            "HO" => Self::Ho,
            "EUE" => Self::Eue,
            "EUFADE" => Self::Eufade,
            "EUFAD37" => Self::Eufad37,
            "ATK" => Self::Atk,
            "NAM" => Self::Nam,
            "EAM" => Self::Eam,
            "KBAUK" => Self::Kbauk,
            "KBAET" => Self::Kbaet,
            "ÁKK" => Self::Akk,
            "EU" => Self::Eu,
            "EUK" => Self::Euk,
            "MAA" => Self::Maa,
            other => match other.parse::<Decimal>() {
                Ok(rate) => Self::Percent(rate),
                Err(_) => Self::Other(other.to_owned()),
            },
        }
    }
}

/// Parses a wire token; unknown codes become [`VatRate::Other`] without
/// reallocating.
impl From<String> for VatRate {
    fn from(token: String) -> Self {
        match Self::from(token.as_str()) {
            Self::Other(_) => Self::Other(token),
            rate => rate,
        }
    }
}

/// Parses a wire token; never fails because unknown codes become
/// [`VatRate::Other`].
impl FromStr for VatRate {
    type Err = Infallible;

    fn from_str(token: &str) -> Result<Self, Self::Err> {
        Ok(Self::from(token))
    }
}

impl fmt::Display for VatRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_wire())
    }
}

/// Serializes as the wire token, e.g. `"27"` or `"AAM"`.
impl serde::Serialize for VatRate {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.as_wire())
    }
}

/// Deserializes from the wire token; unknown codes become
/// [`VatRate::Other`].
impl<'de> serde::Deserialize<'de> for VatRate {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from(String::deserialize(deserializer)?))
    }
}

/// A currency code (`pénznem`).
///
/// szamlazz.hu accepts 37 ISO-style codes; `HUF` may also be written `Ft`.
/// The set is open — any code converts via [`Currency::new`] or `From`.
#[doc(alias = "pénznem")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Currency(Cow<'static, str>);

impl Currency {
    /// Hungarian forint.
    pub const HUF: Self = Self(Cow::Borrowed("HUF"));
    /// Euro.
    pub const EUR: Self = Self(Cow::Borrowed("EUR"));
    /// US dollar.
    pub const USD: Self = Self(Cow::Borrowed("USD"));
    /// Swiss franc.
    pub const CHF: Self = Self(Cow::Borrowed("CHF"));
    /// Pound sterling.
    pub const GBP: Self = Self(Cow::Borrowed("GBP"));

    /// A currency from its wire code.
    pub fn new(code: impl Into<String>) -> Self {
        Self(Cow::Owned(code.into()))
    }

    /// The wire code.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this is the Hungarian forint (`HUF` or its `Ft` alias).
    ///
    /// Non-HUF invoices must carry an exchange rate and quoting bank.
    #[must_use]
    pub fn is_huf(&self) -> bool {
        self.0 == "HUF" || self.0 == "Ft"
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for Currency {
    fn from(code: &str) -> Self {
        Self::new(code)
    }
}

impl From<String> for Currency {
    fn from(code: String) -> Self {
        Self::new(code)
    }
}

/// Serializes as the wire code, e.g. `"HUF"`.
impl serde::Serialize for Currency {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// Deserializes from the wire code.
impl<'de> serde::Deserialize<'de> for Currency {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::new(String::deserialize(deserializer)?))
    }
}

/// The language a document is issued in (`számla nyelve`).
#[doc(alias = "számla nyelve")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Language {
    /// Hungarian (`hu`).
    Hungarian,
    /// English (`en`).
    English,
    /// German (`de`).
    German,
    /// Italian (`it`).
    Italian,
    /// Romanian (`ro`).
    Romanian,
    /// Slovak (`sk`).
    Slovak,
    /// Croatian (`hr`).
    Croatian,
    /// French (`fr`).
    French,
    /// Spanish (`es`).
    Spanish,
    /// Czech (`cz`).
    Czech,
    /// Polish (`pl`).
    Polish,
    /// Bulgarian (`bg`).
    Bulgarian,
    /// Dutch (`nl`).
    Dutch,
    /// Russian (`ru`).
    Russian,
    /// Slovenian (`si`).
    Slovenian,
}

impl Language {
    /// The exact wire token.
    #[must_use]
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Hungarian => "hu",
            Self::English => "en",
            Self::German => "de",
            Self::Italian => "it",
            Self::Romanian => "ro",
            Self::Slovak => "sk",
            Self::Croatian => "hr",
            Self::French => "fr",
            Self::Spanish => "es",
            Self::Czech => "cz",
            Self::Polish => "pl",
            Self::Bulgarian => "bg",
            Self::Dutch => "nl",
            Self::Russian => "ru",
            Self::Slovenian => "si",
        }
    }
}

/// Parses a wire token (`hu`, `en`, …).
impl FromStr for Language {
    type Err = UnknownLanguage;

    fn from_str(token: &str) -> Result<Self, Self::Err> {
        Ok(match token {
            "hu" => Self::Hungarian,
            "en" => Self::English,
            "de" => Self::German,
            "it" => Self::Italian,
            "ro" => Self::Romanian,
            "sk" => Self::Slovak,
            "hr" => Self::Croatian,
            "fr" => Self::French,
            "es" => Self::Spanish,
            "cz" => Self::Czech,
            "pl" => Self::Polish,
            "bg" => Self::Bulgarian,
            "nl" => Self::Dutch,
            "ru" => Self::Russian,
            "si" => Self::Slovenian,
            _ => return Err(UnknownLanguage(token.to_owned())),
        })
    }
}

/// A token that is not a known [`Language`] wire code.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown language: {0}")]
pub struct UnknownLanguage(String);

/// Serializes as the wire token, e.g. `"hu"`.
impl serde::Serialize for Language {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_wire())
    }
}

/// Deserializes from the wire token; unknown languages are an error.
impl<'de> serde::Deserialize<'de> for Language {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// A payment method (`fizetési mód`).
///
/// The wire accepts free text; the constants cover the values szamlazz.hu
/// recognizes and maps for NAV reporting.
#[doc(alias = "fizetési mód")]
#[doc(alias = "fizmod")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PaymentMethod {
    /// `átutalás` — bank transfer.
    Transfer,
    /// `készpénz` — cash.
    Cash,
    /// `bankkártya` — card payment.
    Card,
    /// `csekk` — check.
    Check,
    /// `utánvét` — cash on delivery.
    CashOnDelivery,
    /// `PayPal`.
    PayPal,
    /// `SZÉP kártya` — SZÉP card.
    SzepCard,
    /// Any other free-text payment method.
    Other(String),
}

impl PaymentMethod {
    /// The exact wire token.
    #[must_use]
    pub fn as_wire(&self) -> &str {
        match self {
            Self::Transfer => "átutalás",
            Self::Cash => "készpénz",
            Self::Card => "bankkártya",
            Self::Check => "csekk",
            Self::CashOnDelivery => "utánvét",
            Self::PayPal => "PayPal",
            Self::SzepCard => "SZÉP kártya",
            Self::Other(method) => method,
        }
    }
}

/// Parses a wire token into a known method, or [`PaymentMethod::Other`].
impl From<&str> for PaymentMethod {
    fn from(token: &str) -> Self {
        match token {
            "átutalás" => Self::Transfer,
            "készpénz" => Self::Cash,
            "bankkártya" => Self::Card,
            "csekk" => Self::Check,
            "utánvét" => Self::CashOnDelivery,
            "PayPal" => Self::PayPal,
            "SZÉP kártya" => Self::SzepCard,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// Parses a wire token; unknown methods become [`PaymentMethod::Other`]
/// without reallocating.
impl From<String> for PaymentMethod {
    fn from(token: String) -> Self {
        match Self::from(token.as_str()) {
            Self::Other(_) => Self::Other(token),
            method => method,
        }
    }
}

/// Parses a wire token; never fails because unknown methods become
/// [`PaymentMethod::Other`].
impl FromStr for PaymentMethod {
    type Err = Infallible;

    fn from_str(token: &str) -> Result<Self, Self::Err> {
        Ok(Self::from(token))
    }
}

impl fmt::Display for PaymentMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire())
    }
}

/// Serializes as the wire token, e.g. `"átutalás"`.
impl serde::Serialize for PaymentMethod {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_wire())
    }
}

/// Deserializes from the wire token; unknown methods become
/// [`PaymentMethod::Other`].
impl<'de> serde::Deserialize<'de> for PaymentMethod {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from(String::deserialize(deserializer)?))
    }
}

/// The buyer's taxpayer status (`adóalany`), reported to NAV.
#[doc(alias = "adóalany")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TaxpayerStatus {
    /// `7` — business outside the EU.
    NonEuBusiness,
    /// `6` — business in another EU member state.
    EuBusiness,
    /// `1` — has a Hungarian tax number.
    HasTaxNumber,
    /// `0` — unknown.
    Unknown,
    /// `-1` — no tax number (private individual).
    NoTaxNumber,
}

impl TaxpayerStatus {
    /// The exact wire token.
    #[must_use]
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::NonEuBusiness => "7",
            Self::EuBusiness => "6",
            Self::HasTaxNumber => "1",
            Self::Unknown => "0",
            Self::NoTaxNumber => "-1",
        }
    }
}

/// Parses a wire token (`7`, `6`, `1`, `0`, `-1`).
impl FromStr for TaxpayerStatus {
    type Err = UnknownTaxpayerStatus;

    fn from_str(token: &str) -> Result<Self, Self::Err> {
        Ok(match token {
            "7" => Self::NonEuBusiness,
            "6" => Self::EuBusiness,
            "1" => Self::HasTaxNumber,
            "0" => Self::Unknown,
            "-1" => Self::NoTaxNumber,
            _ => return Err(UnknownTaxpayerStatus(token.to_owned())),
        })
    }
}

/// A token that is not a known [`TaxpayerStatus`] wire code.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown taxpayer status: {0}")]
pub struct UnknownTaxpayerStatus(String);

/// Serializes as the wire token, e.g. `"7"`.
impl serde::Serialize for TaxpayerStatus {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_wire())
    }
}

/// Deserializes from the wire token; unknown statuses are an error.
impl<'de> serde::Deserialize<'de> for TaxpayerStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vat_rate_round_trips() {
        assert_eq!(VatRate::from("27"), VatRate::percent(27));
        assert_eq!(VatRate::from("5.5").as_wire(), "5.5");
        assert_eq!(VatRate::from("AAM"), VatRate::Aam);
        assert_eq!(VatRate::from("F.AFA").as_wire(), "F.AFA");
        assert_eq!(VatRate::from("ÁKK"), VatRate::Akk);
        assert_eq!(
            VatRate::from("BRAND_NEW"),
            VatRate::Other("BRAND_NEW".into())
        );
        assert_eq!(
            VatRate::from(String::from("BRAND_NEW")),
            VatRate::Other("BRAND_NEW".into())
        );
        assert_eq!("27".parse::<VatRate>(), Ok(VatRate::percent(27)));
    }

    #[test]
    fn payment_method_parses_wire_tokens() {
        assert_eq!(PaymentMethod::from("átutalás"), PaymentMethod::Transfer);
        assert_eq!(
            PaymentMethod::from(String::from("Bitcoin")),
            PaymentMethod::Other("Bitcoin".into())
        );
        assert_eq!("készpénz".parse::<PaymentMethod>(), Ok(PaymentMethod::Cash));
    }

    #[test]
    fn language_parses_wire_tokens() {
        assert_eq!("hu".parse::<Language>(), Ok(Language::Hungarian));
        assert_eq!(
            "xx".parse::<Language>(),
            Err(UnknownLanguage("xx".to_owned()))
        );
        assert_eq!(Language::English.as_wire().parse(), Ok(Language::English));
    }

    #[test]
    fn taxpayer_status_parses_wire_tokens() {
        assert_eq!(
            "-1".parse::<TaxpayerStatus>(),
            Ok(TaxpayerStatus::NoTaxNumber)
        );
        assert_eq!(
            "42".parse::<TaxpayerStatus>(),
            Err(UnknownTaxpayerStatus("42".to_owned()))
        );
    }

    #[test]
    fn pdf_converts_to_and_from_bytes() {
        let pdf = Pdf::from(b"%PDF-1.4".to_vec());
        assert_eq!(pdf.as_bytes(), b"%PDF-1.4");
        assert_eq!(Vec::from(pdf), b"%PDF-1.4".to_vec());
    }

    #[test]
    fn huf_aliases() {
        assert!(Currency::HUF.is_huf());
        assert!(Currency::new("Ft").is_huf());
        assert!(Currency::from("Ft").is_huf());
        assert!(Currency::from(String::from("HUF")).is_huf());
        assert!(!Currency::EUR.is_huf());
    }

    #[test]
    fn agent_key_debug_is_redacted() {
        let debug = format!("{:?}", crate::credentials::AgentKey::new("secret"));
        assert!(!debug.contains("secret"));
    }
}
