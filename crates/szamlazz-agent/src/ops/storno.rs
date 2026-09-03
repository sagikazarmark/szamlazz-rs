//! Invoice reversal / storno (`xmlszamlast`): issues a reversing invoice for
//! a previously created document.

use jiff::civil::Date;

use crate::credentials::Credentials;
use crate::error::ResponseError;
use crate::ops::invoice::{CreatedInvoice, InvoiceTemplate, SellerEmail, parse_issued};
use crate::types::InvoiceNumber;
use crate::wire::{AgentRequest, RawResponse};
use crate::xml;

/// The invoice-reversal operation (`xmlszamlast`, `action-szamla_agent_st`).
///
/// Reverses the invoice named by [`StornoInvoice::invoice_number`]. The storno
/// invoice is itself a newly issued document, so the response is a
/// [`CreatedInvoice`]; its PDF, when [`StornoInvoice::download_pdf`] is set,
/// arrives decoded in [`CreatedInvoice::pdf`].
///
/// # Server behaviour
///
/// Observed against a szamlazz.hu test account; the response shape alone does
/// not tell these cases apart, so check
/// [`CreatedInvoice::reverses`] after every call.
///
/// - **Repeat storno is idempotent.** Reversing an already reversed invoice
///   returns success echoing the *existing* storno invoice — same number,
///   same negative totals, same [`document_id`](CreatedInvoice::document_id).
///   No second storno invoice is issued and no error code is raised, so
///   "created now" and "already existed" are indistinguishable from the
///   response. Re-sending a storno after a transport failure is therefore
///   safe.
/// - **Storno of a storno invoice** is rejected with
///   [`ErrorCode::StornoOfReversalInvoice`](crate::ErrorCode::StornoOfReversalInvoice)
///   (14).
/// - **Storno of an invoice that has a corrective invoice** is rejected with
///   [`ErrorCode::HasCorrectiveInvoice`](crate::ErrorCode::HasCorrectiveInvoice)
///   (221).
/// - **Storno of a proforma or a delivery note** is a success-shaped no-op:
///   the response echoes the *requested* document unchanged (its own number,
///   positive totals) and nothing is reversed. Only
///   [`CreatedInvoice::reverses`] detects this.
/// - **Payments are not carried over.** After the reversal, the original
///   invoice's recorded payments disappear from its queried XML and the storno
///   invoice's outstanding amount is its full negative gross, ignoring prior
///   credit entries.
#[doc(alias = "xmlszamlast")]
#[doc(alias = "sztornó")]
#[doc(alias = "számla sztornózás")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct StornoInvoice {
    /// The invoice to reverse (`fejlec`/`szamlaszam`).
    #[doc(alias = "számlaszám")]
    pub invoice_number: InvoiceNumber,
    /// Issue the storno invoice as an e-invoice (`eszamla`); requires the
    /// subscription feature.
    #[doc(alias = "e-számla")]
    #[serde(default)]
    pub e_invoice: bool,
    /// Return the PDF in the response (`szamlaLetoltes`).
    #[serde(default)]
    pub download_pdf: bool,
    /// Number of copies in the downloaded PDF (`szamlaLetoltesPld`).
    ///
    /// Deprecated by szamlazz.hu: the element remains schema-valid but the
    /// server ignores it, so it no longer affects the returned PDF.
    pub copies: Option<u8>,
    /// Aggregator identifier (`aggregator`) for contracted integrations.
    pub aggregator: Option<String>,
    /// Guardian processing flag (`guardian`) for contracted integrations.
    pub guardian: Option<bool>,
    /// External identifier stored on the storno invoice (`szamlaKulsoAzon`).
    ///
    /// The official docs describe this field both as a lookup key for the
    /// invoice being reversed and as an identifier assigned to the storno
    /// invoice. Observed behaviour, sent together with
    /// [`invoice_number`](Self::invoice_number): the value is stored on the
    /// *created storno invoice*, which then resolves through
    /// [`InvoiceSelector::ExternalId`](crate::ops::query_pdf::InvoiceSelector::ExternalId);
    /// the original keeps its own external identifier. It is not used to look
    /// up the original when `invoice_number` is present. It attaches only on
    /// the call that actually creates the storno invoice: a repeat storno
    /// echoes the existing storno invoice and silently drops the identifier.
    pub external_id: Option<String>,
    /// Issue date of the storno invoice (`keltDatum`). `None` lets szamlazz.hu
    /// use today.
    ///
    /// Leave it `None`: on e-invoice accounts any `keltDatum` other than today
    /// is rejected with
    /// [`ErrorCode::IssueDateMustBeToday`](crate::ErrorCode::IssueDateMustBeToday)
    /// (352).
    #[doc(alias = "keltDatum")]
    pub issue_date: Option<Date>,
    /// Fulfillment date of the storno invoice (`teljesitesDatum`).
    #[doc(alias = "teljesítés dátum")]
    pub fulfillment_date: Option<Date>,
    /// Free-text comment (`megjegyzes`), e.g. the reason for the reversal.
    pub comment: Option<String>,
    /// PDF template for the storno invoice (`szamlaSablon`).
    pub template: Option<InvoiceTemplate>,
    /// Settings for the notification email szamlazz.hu sends to the buyer
    /// (`elado` block).
    pub seller_email: Option<SellerEmail>,
    /// Buyer email address (`vevo`/`email`) the storno invoice is sent to.
    pub buyer_email: Option<String>,
    /// Buyer's Hungarian tax number (`vevo`/`adoszam`).
    #[doc(alias = "adószám")]
    pub buyer_tax_number: Option<String>,
    /// Buyer's EU tax number (`vevo`/`adoszamEU`).
    pub buyer_eu_tax_number: Option<String>,
}

impl StornoInvoice {
    /// A reversal of the given invoice; every optional field defaults to
    /// absent and no PDF is requested.
    pub fn new(invoice_number: impl Into<InvoiceNumber>) -> Self {
        Self {
            invoice_number: invoice_number.into(),
            e_invoice: false,
            download_pdf: false,
            copies: None,
            aggregator: None,
            guardian: None,
            external_id: None,
            issue_date: None,
            fulfillment_date: None,
            comment: None,
            template: None,
            seller_email: None,
            buyer_email: None,
            buyer_tax_number: None,
            buyer_eu_tax_number: None,
        }
    }
}

impl AgentRequest for StornoInvoice {
    const ACTION: &'static str = "action-szamla_agent_st";
    type Response = CreatedInvoice;

    fn write_xml(&self, credentials: &Credentials) -> Vec<u8> {
        xml::document(
            "xmlszamlast",
            "http://www.szamlazz.hu/xmlszamlast",
            |root| {
                root.node("beallitasok", |s| {
                    s.credentials(credentials);
                    s.bool("eszamla", self.e_invoice);
                    s.bool("szamlaLetoltes", self.download_pdf);
                    if let Some(copies) = self.copies {
                        s.text("szamlaLetoltesPld", &copies.to_string());
                    }
                    s.text_opt("aggregator", self.aggregator.as_deref());
                    if let Some(guardian) = self.guardian {
                        s.bool("guardian", guardian);
                    }
                    s.text("valaszVerzio", "2");
                    s.text_opt("szamlaKulsoAzon", self.external_id.as_deref());
                });
                root.node("fejlec", |f| {
                    f.text("szamlaszam", self.invoice_number.as_str());
                    f.date_opt("keltDatum", self.issue_date);
                    f.date_opt("teljesitesDatum", self.fulfillment_date);
                    f.text_opt("megjegyzes", self.comment.as_deref());
                    f.text("tipus", "SS");
                    if let Some(template) = &self.template {
                        f.text("szamlaSablon", template.as_wire());
                    }
                });
                root.node("elado", |e| {
                    if let Some(email) = &self.seller_email {
                        e.text_opt("emailReplyto", email.reply_to.as_deref());
                        e.text_opt("emailTargy", email.subject.as_deref());
                        e.text_opt("emailSzoveg", email.body.as_deref());
                    }
                });
                root.node("vevo", |v| {
                    v.text_opt("email", self.buyer_email.as_deref());
                    v.text_opt("adoszam", self.buyer_tax_number.as_deref());
                    v.text_opt("adoszamEU", self.buyer_eu_tax_number.as_deref());
                });
            },
        )
    }

    fn parse(&self, response: &RawResponse) -> Result<Self::Response, ResponseError> {
        parse_issued(response)
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use rust_decimal::dec;

    use super::*;

    fn sample() -> StornoInvoice {
        StornoInvoice {
            download_pdf: true,
            copies: Some(1),
            issue_date: Some(date(2026, 7, 4)),
            comment: Some("Hibás vevő".to_owned()),
            seller_email: Some(SellerEmail {
                reply_to: Some("agent@example.com".to_owned()),
                subject: Some("Sztornó számla".to_owned()),
                body: Some("Lorem ipsum".to_owned()),
            }),
            buyer_email: Some("buyer@example.com".to_owned()),
            ..StornoInvoice::new("E-TST-2026-1")
        }
    }

    #[test]
    fn writes_canonical_storno_xml() {
        let xml = sample().write_xml(&Credentials::agent_key("key"));
        let expected = include_str!("../../tests/golden/xmlszamlast.xml").trim_end();
        assert_eq!(String::from_utf8(xml).expect("utf-8"), expected);
    }

    #[test]
    fn defaults_are_minimal() {
        let xml = String::from_utf8(
            StornoInvoice::new("E-TST-2026-1").write_xml(&Credentials::agent_key("key")),
        )
        .expect("utf-8");
        assert!(xml.contains("<szamlaLetoltes>false</szamlaLetoltes>"));
        assert!(!xml.contains("<szamlaLetoltesPld>"));
        assert!(!xml.contains("<keltDatum>"));
        assert!(xml.contains("<tipus>SS</tipus>"));
        assert!(xml.contains("<elado></elado>"));
        assert!(xml.contains("<vevo></vevo>"));
    }

    #[test]
    fn writes_current_specialized_settings_and_template_in_xsd_order() {
        let mut storno = StornoInvoice::new("E-TST-2026-1");
        storno.aggregator = Some("AGG".into());
        storno.guardian = Some(true);
        storno.template = Some(InvoiceTemplate::NoEnvelope);
        let xml =
            String::from_utf8(storno.write_xml(&Credentials::agent_key("key"))).expect("UTF-8");
        assert!(xml.contains(
            "<aggregator>AGG</aggregator><guardian>true</guardian><valaszVerzio>2</valaszVerzio>"
        ));
        assert!(xml.contains("<tipus>SS</tipus><szamlaSablon>SzlaNoEnv</szamlaSablon>"));
    }

    #[test]
    fn parses_success_response() {
        let body = include_bytes!("../../tests/synthetic/xmlszamlavalasz.xml");
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let created = sample().parse(&response).expect("success");
        assert_eq!(created.invoice_number.as_str(), "E-TST-2026-3");
        assert_eq!(created.document_id, None);
        assert_eq!(created.net_total, Some(dec!(30000)));
        assert_eq!(created.gross_total, Some(dec!(38100)));
        assert!(created.pdf.is_none());
    }

    /// The observed storno response: a new number, negative totals, and the
    /// storno invoice's own document id in `szlahu_id`.
    #[test]
    fn parses_observed_storno_response() {
        let body = br#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><szamlaszam>CTEST-2026-42</szamlaszam><szamlanetto>-1000</szamlanetto><szamlabrutto>-1270</szamlabrutto><kintlevoseg>-1270</kintlevoseg></xmlszamlavalasz>"#;
        let response = RawResponse::new(
            [
                ("szlahu_szamlaszam", "CTEST-2026-42"),
                ("szlahu_id", "924307747"),
                ("szlahu_bruttovegosszeg", "-1270"),
            ],
            body.to_vec(),
        );
        let request = StornoInvoice::new("CTEST-2026-40");
        let created = request.parse(&response).expect("success");
        assert_eq!(created.invoice_number.as_str(), "CTEST-2026-42");
        assert_eq!(created.document_id, Some(924_307_747));
        assert_eq!(created.gross_total, Some(dec!(-1270)));
        assert_eq!(created.outstanding, Some(dec!(-1270)));
        assert!(created.reverses(&request.invoice_number));
    }

    /// Storno of a proforma or delivery note succeeds on the wire but echoes
    /// the requested document unchanged; `reverses` is the only tell.
    #[test]
    fn success_shaped_no_op_is_not_a_reversal() {
        let body = br#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><szamlaszam>D-CTEST-14</szamlaszam><szamlanetto>1000</szamlanetto><szamlabrutto>1270</szamlabrutto><kintlevoseg>1270</kintlevoseg></xmlszamlavalasz>"#;
        let response = RawResponse::new([("szlahu_id", "924309236")], body.to_vec());
        let request = StornoInvoice::new("D-CTEST-14");
        let created = request.parse(&response).expect("wire success");
        assert_eq!(created.invoice_number, request.invoice_number);
        assert!(!created.reverses(&request.invoice_number));
    }

    #[test]
    fn observed_rejections_are_typed() {
        for (code, expected) in [
            ("14", crate::ErrorCode::StornoOfReversalInvoice),
            ("221", crate::ErrorCode::HasCorrectiveInvoice),
            ("352", crate::ErrorCode::IssueDateMustBeToday),
        ] {
            let body = format!(
                r#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod>{code}</hibakod><hibauzenet>rejected</hibauzenet></xmlszamlavalasz>"#
            );
            let response = RawResponse::new(
                [("szlahu_error_code", code), ("szlahu_error", "rejected")],
                body.into_bytes(),
            );
            match sample().parse(&response).expect_err("error") {
                ResponseError::Api(api) => assert_eq!(api.code, expected, "code {code}"),
                other => panic!("expected api error, got {other:?}"),
            }
        }
    }

    #[test]
    fn successful_storno_requires_invoice_number() {
        let body = br#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres></xmlszamlavalasz>"#;
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        assert!(matches!(
            sample().parse(&response),
            Err(ResponseError::Parse(crate::ParseError::Missing(
                "szamlaszam"
            )))
        ));
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
}
