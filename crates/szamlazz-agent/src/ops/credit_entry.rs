//! Credit-entry registration (`xmlszamlakifiz`): records credit entries
//! against an existing invoice and answers its balance.

use jiff::civil::Date;
use rust_decimal::Decimal;

use super::envelope::{self, decimal_body_or_header};
use crate::credentials::Credentials;
use crate::error::{ParseError, RequestError, ResponseError};
use crate::types::{InvoiceNumber, PaymentMethod};
use crate::wire::{AgentRequest, RawResponse};
use crate::xml;

/// One credit entry to register against the invoice (a `kifizetes` block).
///
/// What the XML query reads back as a
/// [`RecordedCreditEntry`](crate::ops::query_xml::RecordedCreditEntry).
#[doc(alias = "kifizetés")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CreditEntry {
    /// The date of the credit entry (`datum`).
    pub date: Date,
    /// The title of the credit entry (`jogcim`): the payment method it was
    /// settled by. The same element a queried document reports as
    /// [`RecordedCreditEntry::title`](crate::ops::query_xml::RecordedCreditEntry::title).
    #[doc(alias = "jogcím")]
    pub title: PaymentMethod,
    /// Amount credited (`osszeg`).
    #[doc(alias = "összeg")]
    pub amount: Decimal,
    /// Free-text description (`leiras`).
    pub description: Option<String>,
}

impl CreditEntry {
    /// A credit entry without a description.
    #[must_use]
    pub fn new(date: Date, title: PaymentMethod, amount: Decimal) -> Self {
        Self {
            date,
            title,
            amount,
            description: None,
        }
    }
}

/// A bounded collection of at most five credit entries.
///
/// Dereferences to the slice of entries and iterates over them, so the
/// collection idioms (`len`, `is_empty`, `iter`, `for`) read as on a `Vec`;
/// growth goes through [`CreditEntries::push`], which keeps the bound.
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

impl std::ops::Deref for CreditEntries {
    type Target = [CreditEntry];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<[CreditEntry]> for CreditEntries {
    fn as_ref(&self) -> &[CreditEntry] {
        &self.0
    }
}

impl IntoIterator for CreditEntries {
    type Item = CreditEntry;
    type IntoIter = std::vec::IntoIter<CreditEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a CreditEntries {
    type Item = &'a CreditEntry;
    type IntoIter = std::slice::Iter<'a, CreditEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
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
/// Registers up to five credit entries against the invoice named by
/// [`RegisterCreditEntry::invoice_number`]. Unless
/// [`RegisterCreditEntry::additive`] is set, the entries *replace* the
/// invoice's existing credit entries, so a replacing request with no
/// entries would clear them, and is refused by [`validate`](AgentRequest::validate)
/// ([`RequestError::EmptyCreditEntryReplace`]).
#[doc(alias = "xmlszamlakifiz")]
#[doc(alias = "jóváírás")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    /// The credit entries to register; at most five per request, and at
    /// least one unless [`additive`](Self::additive).
    pub entries: CreditEntries,
}

impl RegisterCreditEntry {
    /// A credit-entry request for the given invoice with no entries yet;
    /// existing entries are replaced (`additive` is `false`). Set
    /// [`entries`](Self::entries) before sending: a replacing request with
    /// none is refused, since it would clear the invoice's payments.
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

/// The invoice's balance after the credit entries were registered: the reply
/// of [`RegisterCreditEntry`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct InvoiceBalance {
    /// The invoice the credit entries were registered on (`szamlaszam`).
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
    type Response = InvoiceBalance;

    fn validate(&self) -> Result<(), RequestError> {
        if !self.additive && self.entries.is_empty() {
            return Err(RequestError::EmptyCreditEntryReplace);
        }

        Ok(())
    }

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
                    s.text("valaszVerzio", super::RESPONSE_VERSION);
                });
                for entry in &self.entries {
                    root.node("kifizetes", |k| {
                        k.date("datum", entry.date);
                        k.text("jogcim", entry.title.as_wire());
                        k.decimal("osszeg", entry.amount);
                        k.text_opt("leiras", entry.description.as_deref());
                    });
                }
            },
        )
    }

    fn parse(&self, response: &RawResponse) -> Result<Self::Response, ResponseError> {
        let body: envelope::Body = xml::valasz(response, envelope::ROOT, envelope::NAMESPACE)?;

        Ok(InvoiceBalance {
            invoice_number: body
                .invoice_number(response)
                .ok_or(ParseError::Missing("szamlaszam"))?,
            net_total: decimal_body_or_header(
                body.szamlanetto.as_deref(),
                "szamlanetto",
                response,
                "szlahu_nettovegosszeg",
            )?,
            gross_total: decimal_body_or_header(
                body.szamlabrutto.as_deref(),
                "szamlabrutto",
                response,
                "szlahu_bruttovegosszeg",
            )?,
            outstanding: decimal_body_or_header(
                body.kintlevoseg.as_deref(),
                "kintlevoseg",
                response,
                "szlahu_kintlevoseg",
            )?,
            payment_method: header_payment_method(response),
            customer_account_url: body.customer_account_url(response),
        })
    }
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

    /// The balance is journal-safe: it round-trips through JSON with the
    /// payment method as its wire token.
    #[test]
    fn invoice_balance_round_trips_through_json() {
        let body = include_bytes!("../../tests/synthetic/xmlszamlavalasz.xml");
        let response = RawResponse::new(
            [
                ("szlahu_kintlevoseg", "8100"),
                ("szlahu_fizetesmod", "%C3%A1tutal%C3%A1s"),
            ],
            body.to_vec(),
        );
        let result = sample().parse(&response).expect("success");
        assert_eq!(result.payment_method, Some(PaymentMethod::Transfer));

        let json = serde_json::to_value(&result).expect("serialize");
        assert_eq!(json["invoice_number"], "E-TST-2026-3");
        assert_eq!(json["outstanding"], "8100");
        assert_eq!(json["payment_method"], "átutalás");

        let restored: InvoiceBalance = serde_json::from_value(json).expect("deserialize");
        assert_eq!(restored, result);
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

    /// A body that is not szamlazz.hu's answer is quoted as a bounded
    /// excerpt with the length noted: a proxy's page or a stack trace never
    /// travels whole into a consumer's logs or journal.
    #[test]
    fn unexpected_body_is_quoted_as_a_bounded_excerpt() {
        let page = format!("<html>{}</html>", "y".repeat(4000));
        let response = RawResponse::new::<&str, &str>([], page.clone().into_bytes());
        let error = sample().parse(&response).expect_err("error");
        match error {
            ResponseError::Parse(ParseError::UnexpectedBody(body)) => {
                assert!(body.contains("got html: <html>yyyy"), "{body}");
                assert!(body.len() < 600, "bounded: {} bytes", body.len());
                assert!(
                    body.contains(&format!("{} bytes", page.len())),
                    "notes the length: {body}"
                );
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
    /// only: szamlazz.hu sets no `szlahu_error_code` header on this path.
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

    /// The bounded collection reads like a slice: length, emptiness,
    /// iteration by reference and by value.
    #[test]
    fn credit_entries_have_the_collection_idioms() {
        let entries = sample().entries;
        assert_eq!(entries.len(), 2);
        assert!(!entries.is_empty());
        assert_eq!(entries.iter().count(), 2);
        assert_eq!((&entries).into_iter().count(), 2);
        assert_eq!(entries.as_ref().len(), 2);
        let amounts: Vec<Decimal> = entries.into_iter().map(|entry| entry.amount).collect();
        assert_eq!(amounts, [dec!(1000), dec!(2000)]);
        assert!(CreditEntries::new().is_empty());
    }

    /// `RegisterCreditEntry::new(n)` is one forgotten `entries = …` away
    /// from a request that would replace the invoice's payments with nothing:
    /// the schema allows zero `kifizetes` and replace is the default. Refused
    /// before the wire; an empty *additive* request is a harmless no-op and a
    /// populated replace is the normal call.
    #[test]
    fn an_empty_replace_never_reaches_the_wire() {
        let credentials = Credentials::agent_key("key");
        let empty_replace = RegisterCreditEntry::new("E-TST-2026-1");
        assert_eq!(
            empty_replace.to_wire(&credentials).expect_err("refused"),
            RequestError::EmptyCreditEntryReplace
        );

        let empty_additive = RegisterCreditEntry {
            additive: true,
            ..RegisterCreditEntry::new("E-TST-2026-1")
        };
        assert!(empty_additive.to_wire(&credentials).is_ok());
        assert!(
            sample().to_wire(&credentials).is_ok(),
            "a populated replace"
        );
    }
}
