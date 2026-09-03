//! Credit entry / payment registration (`xmlszamlakifiz`): records payments
//! against an existing invoice.

use jiff::civil::Date;
use rust_decimal::Decimal;

use crate::credentials::Credentials;
use crate::error::{ParseError, ResponseError};
use crate::ops::invoice::{InvoiceResponse, decimal_body_or_header};
use crate::types::{InvoiceNumber, PaymentMethod};
use crate::wire::{AgentRequest, RawResponse};
use crate::xml;

/// One payment recorded against the invoice (a `kifizetes` block).
#[doc(alias = "kifizetés")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct CreditEntry {
    /// Payment date (`datum`).
    pub date: Date,
    /// Payment method / legal title of the payment (`jogcim`).
    #[doc(alias = "jogcím")]
    pub method: PaymentMethod,
    /// Amount paid (`osszeg`).
    #[doc(alias = "összeg")]
    pub amount: Decimal,
    /// Free-text description (`leiras`).
    pub description: Option<String>,
}

impl CreditEntry {
    /// A credit entry without a description.
    #[must_use]
    pub fn new(date: Date, method: PaymentMethod, amount: Decimal) -> Self {
        Self {
            date,
            method,
            amount,
            description: None,
        }
    }
}

/// A bounded collection of at most five credit entries.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
#[serde(transparent)]
pub struct CreditEntries(Vec<CreditEntry>);

impl CreditEntries {
    /// An empty collection.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends an entry, rejecting a sixth entry.
    ///
    /// # Errors
    ///
    /// Returns an error when the collection already contains five entries.
    pub fn push(&mut self, entry: CreditEntry) -> Result<(), CreditEntriesError> {
        if self.0.len() == 5 {
            return Err(CreditEntriesError::TooMany);
        }
        self.0.push(entry);
        Ok(())
    }

    /// The credit entries.
    #[must_use]
    pub fn as_slice(&self) -> &[CreditEntry] {
        &self.0
    }
}

impl TryFrom<Vec<CreditEntry>> for CreditEntries {
    type Error = CreditEntriesError;

    fn try_from(entries: Vec<CreditEntry>) -> Result<Self, Self::Error> {
        if entries.len() > 5 {
            Err(CreditEntriesError::TooMany)
        } else {
            Ok(Self(entries))
        }
    }
}

impl<'de> serde::Deserialize<'de> for CreditEntries {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let entries = <Vec<CreditEntry> as serde::Deserialize>::deserialize(deserializer)?;
        Self::try_from(entries).map_err(serde::de::Error::custom)
    }
}

/// Invalid credit-entry collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CreditEntriesError {
    /// The request XSD accepts at most five entries.
    #[error("a credit-entry request can contain at most five entries")]
    TooMany,
}

/// The credit-entry operation (`xmlszamlakifiz`, `action-szamla_agent_kifiz`).
///
/// Registers up to five payments against the invoice named by
/// [`RegisterCreditEntry::invoice_number`]. Unless
/// [`RegisterCreditEntry::additive`] is set, the entries *replace* the
/// invoice's existing credit entries.
#[doc(alias = "xmlszamlakifiz")]
#[doc(alias = "jóváírás")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct RegisterCreditEntry {
    /// The invoice to register credit entries against (`szamlaszam`).
    #[doc(alias = "számlaszám")]
    pub invoice_number: InvoiceNumber,
    /// Tax number of the invoice issuer (`adoszam`); when given, szamlazz.hu
    /// matches the incoming invoice with the corresponding incoming receipt.
    #[doc(alias = "adószám")]
    pub issuer_tax_number: Option<String>,
    /// Keep the invoice's existing credit entries and add these on top
    /// (`additiv`); `false` replaces them.
    #[doc(alias = "additív")]
    #[serde(default)]
    pub additive: bool,
    /// Aggregator identifier (`aggregator`) for contracted integrations.
    pub aggregator: Option<String>,
    /// The payments to record; at most five per request.
    pub entries: CreditEntries,
}

impl RegisterCreditEntry {
    /// A credit-entry request for the given invoice with no entries yet;
    /// existing entries are replaced (`additive` is `false`).
    pub fn new(invoice_number: impl Into<InvoiceNumber>) -> Self {
        Self {
            invoice_number: invoice_number.into(),
            issuer_tax_number: None,
            additive: false,
            aggregator: None,
            entries: CreditEntries::new(),
        }
    }
}

/// The invoice's payment state after the credit entries were registered.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct CreditEntryResult {
    /// The invoice the payments were registered on (`szamlaszam`).
    pub invoice_number: InvoiceNumber,
    /// Net total of the invoice (`szamlanetto` / `szlahu_nettovegosszeg`).
    pub net_total: Option<Decimal>,
    /// Gross total of the invoice (`szamlabrutto` / `szlahu_bruttovegosszeg`).
    pub gross_total: Option<Decimal>,
    /// Outstanding amount (`kintlevoseg` / `szlahu_kintlevoseg`).
    #[doc(alias = "kintlévőség")]
    pub outstanding: Option<Decimal>,
    /// Payment method of the invoice (`szlahu_fizetesmod`).
    pub payment_method: Option<PaymentMethod>,
    /// Customer account URL of the invoice (`vevoifiokurl` /
    /// `szlahu_vevoifiokurl`).
    #[doc(alias = "vevőifiókurl")]
    pub customer_account_url: Option<String>,
}

impl AgentRequest for RegisterCreditEntry {
    const ACTION: &'static str = "action-szamla_agent_kifiz";
    type Response = CreditEntryResult;

    fn write_xml(&self, credentials: &Credentials) -> Vec<u8> {
        xml::document(
            "xmlszamlakifiz",
            "http://www.szamlazz.hu/xmlszamlakifiz",
            |root| {
                root.node("beallitasok", |s| {
                    s.credentials(credentials);
                    s.text("szamlaszam", self.invoice_number.as_str());
                    s.text_opt("adoszam", self.issuer_tax_number.as_deref());
                    s.bool("additiv", self.additive);
                    s.text_opt("aggregator", self.aggregator.as_deref());
                    s.text("valaszVerzio", "2");
                });
                for entry in self.entries.as_slice() {
                    root.node("kifizetes", |k| {
                        k.date("datum", entry.date);
                        k.text("jogcim", entry.method.as_wire());
                        k.decimal("osszeg", entry.amount);
                        k.text_opt("leiras", entry.description.as_deref());
                    });
                }
            },
        )
    }

    fn parse(&self, response: &RawResponse) -> Result<Self::Response, ResponseError> {
        response.check()?;
        let valasz = InvoiceResponse::from_body(response.body())?.into_success()?;

        Ok(CreditEntryResult {
            invoice_number: valasz
                .szamlaszam
                .filter(|s| !s.is_empty())
                .map(InvoiceNumber::new)
                .or_else(|| header_invoice_number(response))
                .ok_or(ParseError::Missing("szamlaszam"))?,
            net_total: decimal_body_or_header(
                valasz.szamlanetto,
                response,
                "szlahu_nettovegosszeg",
            )?,
            gross_total: decimal_body_or_header(
                valasz.szamlabrutto,
                response,
                "szlahu_bruttovegosszeg",
            )?,
            outstanding: decimal_body_or_header(
                valasz.kintlevoseg,
                response,
                "szlahu_kintlevoseg",
            )?,
            payment_method: header_payment_method(response),
            customer_account_url: valasz.vevoifiokurl.filter(|s| !s.is_empty()).or_else(|| {
                response
                    .szlahu("szlahu_vevoifiokurl")
                    .filter(|s| !s.is_empty())
            }),
        })
    }
}

/// The invoice number from the `szlahu_szamlaszam` header, if present.
fn header_invoice_number(response: &RawResponse) -> Option<InvoiceNumber> {
    response
        .szlahu("szlahu_szamlaszam")
        .filter(|s| !s.is_empty())
        .map(InvoiceNumber::new)
}

/// The payment method from the `szlahu_fizetesmod` header, if present.
fn header_payment_method(response: &RawResponse) -> Option<PaymentMethod> {
    response
        .szlahu("szlahu_fizetesmod")
        .filter(|s| !s.is_empty())
        .map(PaymentMethod::from)
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use rust_decimal::dec;

    use super::*;

    fn sample() -> RegisterCreditEntry {
        RegisterCreditEntry {
            invoice_number: InvoiceNumber::new("E-TST-2026-1"),
            issuer_tax_number: Some("12345678-1-13".to_owned()),
            additive: false,
            aggregator: None,
            entries: CreditEntries::try_from(vec![
                CreditEntry::new(date(2026, 7, 1), PaymentMethod::Cash, dec!(1000)),
                CreditEntry {
                    description: Some("Test description".to_owned()),
                    ..CreditEntry::new(date(2026, 7, 15), PaymentMethod::Transfer, dec!(2000))
                },
            ])
            .expect("valid entries"),
        }
    }

    #[test]
    fn writes_canonical_credit_entry_xml() {
        let xml = sample().write_xml(&Credentials::agent_key("key"));
        let expected = include_str!("../../tests/golden/xmlszamlakifiz.xml").trim_end();
        assert_eq!(String::from_utf8(xml).expect("utf-8"), expected);
    }

    #[test]
    fn writes_aggregator_before_response_version() {
        let mut request = sample();
        request.aggregator = Some("AGG".into());
        let xml =
            String::from_utf8(request.write_xml(&Credentials::agent_key("key"))).expect("UTF-8");
        assert!(xml.contains(
            "<additiv>false</additiv><aggregator>AGG</aggregator><valaszVerzio>2</valaszVerzio>"
        ));
    }

    #[test]
    fn parses_structured_response() {
        let body = include_bytes!("../../tests/synthetic/xmlszamlavalasz.xml");
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let result = sample().parse(&response).expect("success");
        assert_eq!(result.invoice_number.as_str(), "E-TST-2026-3");
        assert_eq!(result.net_total, Some(dec!(30000)));
        assert_eq!(result.gross_total, Some(dec!(38100)));
        assert_eq!(result.outstanding, None);
        assert_eq!(result.payment_method, None);
        assert_eq!(result.customer_account_url, None);
    }

    #[test]
    fn version_two_rejects_non_xml_body_even_with_success_headers() {
        let response = RawResponse::new(
            [
                ("szlahu_szamlaszam", "E-TST-2026-1"),
                ("szlahu_nettovegosszeg", "3000"),
                ("szlahu_bruttovegosszeg", "3810"),
                ("szlahu_kintlevoseg", "810"),
                ("szlahu_fizetesmod", "%C3%A1tutal%C3%A1s"),
            ],
            b"A kifizetes rogzitve.".to_vec(),
        );
        let error = sample().parse(&response).expect_err("invalid XML");
        assert!(matches!(
            error,
            ResponseError::Parse(ParseError::UnexpectedBody(_))
        ));
    }

    #[test]
    fn missing_invoice_number_everywhere_is_an_error() {
        let response = RawResponse::new::<&str, &str>([], b"not xml".to_vec());
        let error = sample().parse(&response).expect_err("error");
        match error {
            ResponseError::Parse(ParseError::UnexpectedBody(body)) => {
                assert_eq!(body, "not xml");
            }
            other => panic!("expected unexpected body, got {other:?}"),
        }
    }

    #[test]
    fn header_error_takes_precedence() {
        let response = RawResponse::new(
            [("szlahu_error_code", "3"), ("szlahu_error", "login")],
            Vec::new(),
        );
        let error = sample().parse(&response).expect_err("error");
        match error {
            ResponseError::Api(api) => assert_eq!(api.code, crate::ErrorCode::InvalidCredentials),
            other => panic!("expected api error, got {other:?}"),
        }
    }

    /// A credit entry on a reversed invoice is rejected with 463 in the body
    /// only — szamlazz.hu sets no `szlahu_error_code` header on this path.
    #[test]
    fn body_only_error_is_typed() {
        let body = r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod><![CDATA[463]]></hibakod><hibauzenet><![CDATA[Sztornózó vagy sztornózott számlához nem tartozhat kifizetettségi információ.]]></hibauzenet></xmlszamlavalasz>"#;
        let response = RawResponse::new::<&str, &str>([], body.as_bytes().to_vec());
        let error = sample().parse(&response).expect_err("error");
        match error {
            ResponseError::Api(api) => {
                assert_eq!(api.code, crate::ErrorCode::PaymentOnReversedInvoice);
                assert!(api.message.starts_with("Sztornózó vagy sztornózott"));
            }
            other => panic!("expected api error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_more_than_five_credit_entries() {
        let entries: Vec<_> = (0..6)
            .map(|_| CreditEntry::new(date(2026, 7, 1), PaymentMethod::Cash, dec!(1)))
            .collect();
        assert_eq!(
            CreditEntries::try_from(entries).expect_err("too many"),
            CreditEntriesError::TooMany
        );
    }
}
