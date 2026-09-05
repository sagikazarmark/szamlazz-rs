//! Invoice PDF query (`xmlszamlapdf`): re-download the PDF of a previously
//! issued invoice.

use rust_decimal::Decimal;

use crate::credentials::Credentials;
use crate::error::{ParseError, ResponseError};
use crate::types::{InvoiceNumber, Pdf};
use crate::wire::{AgentRequest, RawResponse};
use crate::xml;

/// How a query identifies the invoice; shared by the PDF and XML queries.
///
/// The wire carries one of `szamlaszam`, `rendelesSzam`, or
/// `szamlaKulsoAzon`; this enum makes
/// sending both (or neither) unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum InvoiceSelector {
    /// By invoice number (`szamlaszam`).
    #[doc(alias = "számlaszám")]
    InvoiceNumber(InvoiceNumber),
    /// By order number (`rendelesSzam`); the *last* invoice issued with this
    /// order number is returned.
    #[doc(alias = "rendelésszám")]
    OrderNumber(String),
    /// By the external identifier supplied when the invoice was created
    /// (`szamlaKulsoAzon`).
    ExternalId(String),
}

/// The invoice PDF query (`xmlszamlapdf`, `action-szamla_agent_pdf`).
///
/// Unlike most operations, this request document has no `beallitasok` block:
/// the credentials sit directly under the root element. The response is
/// always requested in structured form (response version 2), so the PDF
/// arrives decoded in [`InvoicePdf::pdf`].
#[doc(alias = "xmlszamlapdf")]
#[doc(alias = "számla pdf lekérdezés")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct QueryInvoicePdf {
    /// Which invoice to fetch.
    pub selector: InvoiceSelector,
}

impl QueryInvoicePdf {
    /// A PDF query for the invoice named by `selector`.
    #[must_use]
    pub fn new(selector: InvoiceSelector) -> Self {
        Self { selector }
    }
}

/// A fetched invoice PDF with the totals reported alongside it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct InvoicePdf {
    /// The invoice number (`szamlaszam`).
    pub invoice_number: InvoiceNumber,
    /// Net total (`szamlanetto`).
    pub net_total: Option<Decimal>,
    /// Gross total (`szamlabrutto`).
    pub gross_total: Option<Decimal>,
    /// The invoice PDF.
    pub pdf: Pdf,
}

impl AgentRequest for QueryInvoicePdf {
    const ACTION: &'static str = "action-szamla_agent_pdf";
    type Response = InvoicePdf;

    fn write_xml(&self, credentials: &Credentials) -> Vec<u8> {
        xml::document(
            "xmlszamlapdf",
            "http://www.szamlazz.hu/xmlszamlapdf",
            |root| {
                root.credentials(credentials);
                match &self.selector {
                    InvoiceSelector::InvoiceNumber(number) => {
                        root.text("szamlaszam", number.as_str());
                    }
                    InvoiceSelector::OrderNumber(number) => root.text("rendelesSzam", number),
                    InvoiceSelector::ExternalId(_) => {}
                }
                root.text("valaszVerzio", "2");
                if let InvoiceSelector::ExternalId(id) = &self.selector {
                    root.text("szamlaKulsoAzon", id);
                }
            },
        )
    }

    fn parse(&self, response: &RawResponse) -> Result<Self::Response, ResponseError> {
        let created = crate::ops::invoice::parse_issued(response)?;

        Ok(InvoicePdf {
            invoice_number: created.invoice_number,
            net_total: created.net_total,
            gross_total: created.gross_total,
            pdf: created.pdf.ok_or(ParseError::Missing("pdf"))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::dec;

    use super::*;

    fn sample() -> QueryInvoicePdf {
        QueryInvoicePdf {
            selector: InvoiceSelector::InvoiceNumber(InvoiceNumber::new("E-TST-2026-1")),
        }
    }

    #[test]
    fn writes_canonical_query_xml() {
        let xml = sample().write_xml(&Credentials::agent_key("key"));
        let expected = include_str!("../../tests/golden/xmlszamlapdf.xml").trim_end();
        assert_eq!(String::from_utf8(xml).expect("utf-8"), expected);
    }

    #[test]
    fn order_number_replaces_invoice_number() {
        let query = QueryInvoicePdf {
            selector: InvoiceSelector::OrderNumber("ORDER-123".into()),
        };
        let xml =
            String::from_utf8(query.write_xml(&Credentials::agent_key("key"))).expect("utf-8");
        assert!(xml.contains("<rendelesSzam>ORDER-123</rendelesSzam>"));
        assert!(!xml.contains("<szamlaszam>"));
    }

    #[test]
    fn external_id_is_the_only_serialized_selector() {
        let query = QueryInvoicePdf::new(InvoiceSelector::ExternalId("EXT-42".into()));
        let xml =
            String::from_utf8(query.write_xml(&Credentials::agent_key("key"))).expect("utf-8");
        assert!(xml.contains("<szamlaKulsoAzon>EXT-42</szamlaKulsoAzon>"));
        assert!(!xml.contains("<szamlaszam>"));
        assert!(!xml.contains("<rendelesSzam>"));
    }

    #[test]
    fn parses_success_response() {
        let body = include_bytes!("../../tests/synthetic/querying_pdf_xmlszamlavalasz.xml");
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let fetched = sample().parse(&response).expect("success");
        assert_eq!(fetched.invoice_number.as_str(), "E-TST-2026-3");
        assert_eq!(fetched.net_total, Some(dec!(30000)));
        assert_eq!(fetched.gross_total, Some(dec!(38100)));
        assert_eq!(fetched.pdf.as_bytes(), b"%PDF-");
    }

    /// The fetched PDF is journal-safe: it round-trips through JSON as base64.
    #[test]
    fn invoice_pdf_round_trips_through_json() {
        let body = include_bytes!("../../tests/synthetic/querying_pdf_xmlszamlavalasz.xml");
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let fetched = sample().parse(&response).expect("success");

        let json = serde_json::to_value(&fetched).expect("serialize");
        assert_eq!(json["invoice_number"], "E-TST-2026-3");
        assert_eq!(json["pdf"], "JVBERi0=");

        let restored: InvoicePdf = serde_json::from_value(json).expect("deserialize");
        assert_eq!(restored, fetched);
    }

    #[test]
    fn missing_pdf_is_an_error() {
        let body = include_bytes!("../../tests/synthetic/xmlszamlavalasz.xml");
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let error = sample().parse(&response).expect_err("error");
        match error {
            ResponseError::Parse(ParseError::Missing("pdf")) => {}
            other => panic!("expected missing pdf, got {other:?}"),
        }
    }

    #[test]
    fn parses_error_response() {
        let body = include_bytes!("../../tests/synthetic/querying_pdf_xmlszamlavalasz_error.xml");
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let error = sample().parse(&response).expect_err("error");
        match error {
            ResponseError::Api(api) => {
                assert_eq!(api.code, crate::ErrorCode::InvalidCredentials);
                assert!(api.message.contains("Synthetic login error"));
            }
            other => panic!("expected api error, got {other:?}"),
        }
    }
}
