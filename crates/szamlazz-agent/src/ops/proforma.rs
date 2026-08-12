//! Proforma deletion (`xmlszamladbkdel`): removes an unpaid proforma
//! (díjbekérő) from the account.

use crate::credentials::Credentials;
use crate::error::{ApiError, ParseError, ResponseError};
use crate::types::InvoiceNumber;
use crate::wire::{AgentRequest, RawResponse};
use crate::xml;

/// How the deletion identifies the proforma.
///
/// The wire carries either `szamlaszam` or `rendelesszam`; this enum makes
/// sending both (or neither) unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProformaSelector {
    /// By proforma document number (`szamlaszam`).
    #[doc(alias = "számlaszám")]
    InvoiceNumber(InvoiceNumber),
    /// By order number (`rendelesszam`).
    #[doc(alias = "rendelésszám")]
    OrderNumber(String),
}

/// The proforma-deletion operation (`xmlszamladbkdel`,
/// `action-szamla_agent_dijbekero_torlese`).
///
/// Success carries no payload. Deleting a proforma that does not exist (or was
/// already deleted) fails with
/// [`ErrorCode::ProformaNotFound`](crate::ErrorCode::ProformaNotFound).
#[doc(alias = "xmlszamladbkdel")]
#[doc(alias = "díjbekérő törlése")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct DeleteProforma {
    /// Which proforma to delete.
    pub selector: ProformaSelector,
}

impl DeleteProforma {
    /// A deletion request for the proforma named by `selector`.
    #[must_use]
    pub fn new(selector: ProformaSelector) -> Self {
        Self { selector }
    }
}

impl AgentRequest for DeleteProforma {
    const ACTION: &'static str = "action-szamla_agent_dijbekero_torlese";
    type Response = ();

    fn write_xml(&self, credentials: &Credentials) -> Vec<u8> {
        xml::document(
            "xmlszamladbkdel",
            "http://www.szamlazz.hu/xmlszamladbkdel",
            |root| {
                root.node("beallitasok", |s| {
                    s.credentials(credentials);
                });
                root.node("fejlec", |f| match &self.selector {
                    ProformaSelector::InvoiceNumber(number) => {
                        f.text("szamlaszam", number.as_str());
                    }
                    ProformaSelector::OrderNumber(number) => f.text("rendelesszam", number),
                });
            },
        )
    }

    fn parse(&self, response: &RawResponse) -> Result<Self::Response, ResponseError> {
        response.check()?;
        let text = xml::response_text(
            response.body(),
            "xmlszamladbkdelvalasz",
            "http://www.szamlazz.hu/xmlszamladbkdelvalasz",
        )?;
        let valasz: DeleteResponse = quick_xml::de::from_str(text).map_err(ParseError::from)?;

        if valasz.sikeres {
            Ok(())
        } else {
            Err(ApiError {
                code: valasz
                    .hibakod
                    .map_or_else(|| crate::ErrorCode::Unknown("0".to_owned()), Into::into),
                message: valasz.hibauzenet.unwrap_or_default(),
            }
            .into())
        }
    }
}

/// The `xmlszamladbkdelvalasz` response document.
#[derive(Debug, serde::Deserialize)]
struct DeleteResponse {
    #[serde(deserialize_with = "xml::de::flexible_bool")]
    sikeres: bool,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    hibakod: Option<String>,
    #[serde(default)]
    hibauzenet: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DeleteProforma {
        DeleteProforma {
            selector: ProformaSelector::InvoiceNumber(InvoiceNumber::new("E-TST-2026-1")),
        }
    }

    #[test]
    fn writes_canonical_deletion_xml() {
        let xml = sample().write_xml(&Credentials::agent_key("key"));
        let expected = include_str!("../../tests/golden/xmlszamladbkdel.xml").trim_end();
        assert_eq!(String::from_utf8(xml).expect("utf-8"), expected);
    }

    #[test]
    fn order_number_replaces_invoice_number() {
        let deletion = DeleteProforma {
            selector: ProformaSelector::OrderNumber("ORDER-123".into()),
        };
        let xml =
            String::from_utf8(deletion.write_xml(&Credentials::agent_key("key"))).expect("utf-8");
        assert!(xml.contains("<fejlec><rendelesszam>ORDER-123</rendelesszam></fejlec>"));
        assert!(!xml.contains("<szamlaszam>"));
    }

    #[test]
    fn parses_success_response() {
        let body = include_bytes!("../../tests/synthetic/xmlszamladbkdelvalasz.xml");
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        sample().parse(&response).expect("success");
    }

    #[test]
    fn parses_error_response() {
        let body = include_bytes!("../../tests/synthetic/xmlszamladbkdelvalasz_error.xml");
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let error = sample().parse(&response).expect_err("error");
        match error {
            ResponseError::Api(api) => {
                assert_eq!(api.code, crate::ErrorCode::ProformaNotFound);
                assert!(api.message.contains("Synthetic proforma not found"));
            }
            other => panic!("expected api error, got {other:?}"),
        }
    }

    #[test]
    fn preserves_critical_text_or_html_error() {
        for body in [
            b"critical server error".as_slice(),
            b"<html><body>critical server error</body></html>".as_slice(),
        ] {
            let response = RawResponse::new::<&str, &str>([], body.to_vec());
            let error = sample().parse(&response).expect_err("error");
            match error {
                ResponseError::Parse(ParseError::UnexpectedBody(body)) => {
                    assert!(body.contains("critical server error"));
                }
                other => panic!("expected preserved response body, got {other:?}"),
            }
        }
    }

    #[test]
    fn rejects_deletion_response_in_wrong_namespace() {
        let body = br#"<xmlszamladbkdelvalasz xmlns="https://wrong.example"><sikeres>true</sikeres></xmlszamladbkdelvalasz>"#;
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        assert!(matches!(
            sample().parse(&response),
            Err(ResponseError::Parse(ParseError::UnexpectedBody(_)))
        ));
    }
}
