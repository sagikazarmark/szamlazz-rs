//! Proforma deletion (`xmlszamladbkdel`): removes one proforma by number,
//! or all matching proformas by order number, from the account.
//!
//! This operation does not check payment status before sending. Deletion of
//! a fully paid proforma was observed on the test account. If paid proformas
//! must be retained, the caller must enforce that policy before deletion.

use crate::credentials::Credentials;
use crate::error::ResponseError;
use crate::types::InvoiceNumber;
use crate::wire::{AgentRequest, RawResponse};
use crate::xml;

/// How the deletion selects its target or targets.
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
    /// Deletes **all** proformas carrying this order number (`rendelesszam`).
    /// A query returning the latest match does not narrow this deletion.
    /// See the [vendor's batch-scope note](https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/xml).
    #[doc(alias = "rendelésszám")]
    OrderNumber(String),
}

/// The proforma-deletion operation (`xmlszamladbkdel`,
/// `action-szamla_agent_dijbekero_torlese`).
///
/// Targets an existing proforma without a local paid-state check. A fully
/// paid proforma was deletable on the test account; callers needing to retain
/// paid proformas must enforce that policy before sending.
/// Number selection targets one proforma; order selection targets **all**
/// matching proformas, including any new matches when deliberately repeated.
///
/// Success carries no count or deleted-number list. Deleting a proforma that does not exist (or was
/// already deleted) fails with
/// [`ErrorCode::ProformaNotFound`](crate::ErrorCode::ProformaNotFound).
#[doc(alias = "xmlszamladbkdel")]
#[doc(alias = "díjbekérő törlése")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeleteProforma {
    /// Which proforma or order's proformas to delete.
    pub selector: ProformaSelector,
}

impl DeleteProforma {
    /// A deletion request for the target(s) selected by `selector`.
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
        xml::verdict(
            response,
            "xmlszamladbkdelvalasz",
            "http://www.szamlazz.hu/xmlszamladbkdelvalasz",
        )
    }
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

    /// A `sikeres=false` without a `hibakod` is an error with an absent code,
    /// never a fabricated one.
    #[test]
    fn failure_without_a_code_is_absent() {
        let body = br#"<xmlszamladbkdelvalasz xmlns="http://www.szamlazz.hu/xmlszamladbkdelvalasz"><sikeres>false</sikeres><hibauzenet>Hiba</hibauzenet></xmlszamladbkdelvalasz>"#;
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        match sample().parse(&response).expect_err("error") {
            ResponseError::Api(api) => {
                assert_eq!(api.code, crate::ErrorCode::Absent);
                assert_eq!(api.message, "Hiba");
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
                ResponseError::Parse(crate::ParseError::UnexpectedBody(body)) => {
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
            Err(ResponseError::Parse(crate::ParseError::UnexpectedBody(_)))
        ));
    }
}
