//! Handler inputs of the `Order` Virtual Object and the `SzamlaAgent` service.

use jiff::civil::Date;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::document::{DocumentInput, PaymentMethod};
use super::{DocumentKind, RequestId};

/// Input of `Order.create_proforma`, `create_invoice`, `create_prepayment`
/// and `create_final`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct CreateRequest {
    /// The retry identity of this logical request.
    pub request_id: RequestId,
    /// The document to issue.
    pub document: DocumentInput,
    /// Kind-specific options; all default.
    #[serde(default)]
    pub options: CreateOptions,
}

impl CreateRequest {
    /// A create request with default [`CreateOptions`].
    #[must_use]
    pub fn new(request_id: RequestId, document: DocumentInput) -> Self {
        Self {
            request_id,
            document,
            options: CreateOptions::default(),
        }
    }
}

/// Options of a create request.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(default)]
pub struct CreateOptions {
    /// Issue a new generation after the recorded document was reversed
    /// externally or by an operator. Requires a new request id; without it a
    /// reversed slot answers `outcome: reversed`. Unnecessary after a
    /// service-side storno or a proforma deletion.
    pub reissue: bool,
    /// Which proforma the invoice or prepayment converts (`create_invoice` and
    /// `create_prepayment` only).
    pub proforma: ProformaLink,
}

/// How a create request refers to a proforma.
///
/// Serialises as `"ledger"`, `"none"` or `{"number": "D-…"}`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProformaLink {
    /// Reference the proforma recorded in the order's ledger: a `committed`
    /// one is pre-queried and referenced (`conflict{proforma_missing}` when
    /// szamlazz.hu no longer has it), a `consumed` one is
    /// `conflict{proforma_consumed}`, a `pending` one `conflict{pending}`.
    /// With no proforma recorded the request behaves like
    /// [`ProformaLink::None`].
    #[default]
    Ledger,
    /// Reference no proforma. Refused with `conflict{proforma_live}` while a
    /// live proforma exists under the order number, because szamlazz.hu links
    /// by shared order number regardless.
    None,
    /// Reference a proforma by number, whether or not the ledger knows it.
    Number(String),
}

/// Input of `Order.correct_invoice`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct CorrectRequest {
    /// The invoice being corrected; must be managed by this order.
    pub invoice_number: String,
    /// The retry identity of this logical request. A new id issues a new
    /// corrective by contract.
    pub request_id: RequestId,
    /// The corrective document.
    pub document: DocumentInput,
}

/// Input of `Order.storno_invoice` and `SzamlaAgent.storno`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct StornoRequest {
    /// The invoice to reverse.
    pub invoice_number: String,
    /// Comment placed on the storno invoice.
    #[serde(default)]
    pub comment: Option<String>,
}

impl StornoRequest {
    /// A storno request without a comment.
    pub fn new(invoice_number: impl Into<String>) -> Self {
        Self {
            invoice_number: invoice_number.into(),
            comment: None,
        }
    }
}

/// Input of `Order.delete_proforma`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(default)]
pub struct DeleteProformaRequest {
    /// Delete even when the proforma has registered payments. szamlazz.hu has
    /// no guard of its own; without `force` a paid proforma is
    /// `rejected{proforma_paid}`.
    pub force: bool,
}

/// Input of `Order.get`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(default)]
pub struct GetRequest {
    /// Verify every committed document against szamlazz.hu before answering
    /// (`freshness: live`) instead of returning the ledger as recorded.
    pub verify: bool,
}

/// Input of the private `Order.record_reversal` handler: an operator asserts
/// what szamlazz.hu shows for a recorded document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct RecordReversalRequest {
    /// The recorded invoice the assertion is about.
    pub invoice_number: String,
    /// What the operator asserts.
    pub result: RecordedReversal,
}

/// An operator's assertion about a recorded document.
///
/// Serialises as `{"reversed": {"storno_number": "SS-…"}}` or `"live"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum RecordedReversal {
    /// The document was reversed outside the service; the slot becomes
    /// `reversed{origin: operator}` and the generation advances.
    Reversed {
        /// The storno invoice number, when known.
        #[serde(default)]
        storno_number: Option<String>,
    },
    /// The document is live; a `reversal_unverified` slot returns to
    /// `committed`.
    Live,
}

/// Input of the private `Order.forget` handler: drop a slot whose document
/// szamlazz.hu no longer knows (`conflict{recorded_document_missing}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ForgetRequest {
    /// The slot to forget.
    pub kind: DocumentKind,
}

/// Input of `SzamlaAgent.query`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct QueryRequest {
    /// Which document to look up.
    pub selector: Selector,
}

/// A document selector for the query operation.
///
/// Serialises as `{"invoice_number": "…"}`, `{"order_number": "…"}` or
/// `{"external_id": "…"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Selector {
    /// By invoice number (`számlaszám`).
    InvoiceNumber(String),
    /// By order number (`rendelésszám`); returns the last document issued
    /// under it.
    OrderNumber(String),
    /// By external id (`szamlaKulsoAzon`); not unique server-side, the last
    /// writer wins.
    ExternalId(String),
}

/// Input of `SzamlaAgent.set_payments`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct SetPaymentsRequest {
    /// The invoice to register credit entries on.
    pub invoice_number: String,
    /// The credit entries (`jóváírások`); szamlazz.hu accepts at most five.
    pub entries: Vec<PaymentEntry>,
    /// Add to the existing entries instead of replacing them.
    #[serde(default)]
    pub additive: bool,
}

/// One credit entry (`jóváírás`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct PaymentEntry {
    /// Payment date.
    pub date: Date,
    /// Payment method.
    pub method: PaymentMethod,
    /// Amount in the invoice currency.
    pub amount: Decimal,
    /// Free-text description.
    #[serde(default)]
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use rust_decimal::dec;
    use serde_json::json;

    use super::*;
    use crate::contract::document::tests::sample_document;

    fn request_id() -> RequestId {
        "r-1".parse().expect("valid request id")
    }

    fn round_trip<T>(value: &T) -> serde_json::Value
    where
        T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_value(value).expect("serialize");
        let back: T = serde_json::from_value(json.clone()).expect("deserialize");
        assert_eq!(&back, value);
        json
    }

    #[test]
    fn create_request_round_trips() {
        let mut request = CreateRequest::new(request_id(), sample_document());
        request.options.reissue = true;
        request.options.proforma = ProformaLink::Number("D-1".to_owned());
        let json = round_trip(&request);
        assert_eq!(json["request_id"], "r-1");
        assert_eq!(json["options"]["proforma"], json!({"number": "D-1"}));
    }

    #[test]
    fn create_request_defaults_options() {
        let request: CreateRequest = serde_json::from_value(json!({
            "request_id": "r-1",
            "document": serde_json::to_value(sample_document()).expect("serialize"),
        }))
        .expect("deserialize");
        assert_eq!(request.options, CreateOptions::default());
        assert_eq!(request.options.proforma, ProformaLink::Ledger);
        assert!(!request.options.reissue);
    }

    #[test]
    fn proforma_link_wire_shapes() {
        let cases = [
            (ProformaLink::Ledger, json!("ledger")),
            (ProformaLink::None, json!("none")),
            (
                ProformaLink::Number("D-1".to_owned()),
                json!({"number": "D-1"}),
            ),
        ];
        for (link, expected) in cases {
            assert_eq!(serde_json::to_value(&link).expect("serialize"), expected);
            assert_eq!(
                serde_json::from_value::<ProformaLink>(expected).expect("deserialize"),
                link
            );
        }
    }

    #[test]
    fn correct_and_storno_requests_round_trip() {
        let correct = CorrectRequest {
            invoice_number: "SZ-1".to_owned(),
            request_id: request_id(),
            document: sample_document(),
        };
        round_trip(&correct);
        let mut storno = StornoRequest::new("SZ-1");
        storno.comment = Some("wrong buyer".to_owned());
        round_trip(&storno);
        let bare: StornoRequest =
            serde_json::from_value(json!({"invoice_number": "SZ-1"})).expect("deserialize");
        assert_eq!(bare, StornoRequest::new("SZ-1"));
    }

    #[test]
    fn flag_requests_default_to_false() {
        assert_eq!(
            serde_json::from_value::<DeleteProformaRequest>(json!({})).expect("deserialize"),
            DeleteProformaRequest { force: false }
        );
        assert_eq!(
            serde_json::from_value::<GetRequest>(json!({"verify": true})).expect("deserialize"),
            GetRequest { verify: true }
        );
        round_trip(&DeleteProformaRequest { force: true });
        round_trip(&GetRequest::default());
    }

    #[test]
    fn record_reversal_request_round_trips() {
        let reversed = RecordReversalRequest {
            invoice_number: "SZ-1".to_owned(),
            result: RecordedReversal::Reversed {
                storno_number: Some("SS-1".to_owned()),
            },
        };
        let json = round_trip(&reversed);
        assert_eq!(
            json["result"],
            json!({"reversed": {"storno_number": "SS-1"}})
        );
        let live = RecordReversalRequest {
            invoice_number: "SZ-1".to_owned(),
            result: RecordedReversal::Live,
        };
        let json = round_trip(&live);
        assert_eq!(json["result"], json!("live"));
        let bare: RecordedReversal =
            serde_json::from_value(json!({"reversed": {}})).expect("deserialize");
        assert_eq!(
            bare,
            RecordedReversal::Reversed {
                storno_number: None
            }
        );
    }

    #[test]
    fn forget_request_round_trips() {
        let json = round_trip(&ForgetRequest {
            kind: DocumentKind::Proforma,
        });
        assert_eq!(json, json!({"kind": "proforma"}));
    }

    #[test]
    fn query_request_selectors() {
        let cases = [
            (
                Selector::InvoiceNumber("SZ-1".to_owned()),
                json!({"invoice_number": "SZ-1"}),
            ),
            (
                Selector::OrderNumber("ORD-1".to_owned()),
                json!({"order_number": "ORD-1"}),
            ),
            (
                Selector::ExternalId("acct:ORD-1:invoice:0".to_owned()),
                json!({"external_id": "acct:ORD-1:invoice:0"}),
            ),
        ];
        for (selector, expected) in cases {
            let request = QueryRequest { selector };
            let json = round_trip(&request);
            assert_eq!(json["selector"], expected);
        }
    }

    #[test]
    fn set_payments_request_round_trips() {
        let request = SetPaymentsRequest {
            invoice_number: "SZ-1".to_owned(),
            entries: vec![PaymentEntry {
                date: date(2026, 7, 10),
                method: PaymentMethod::Card,
                amount: dec!(25400),
                description: Some("card".to_owned()),
            }],
            additive: false,
        };
        let json = round_trip(&request);
        assert_eq!(json["entries"][0]["method"], "card");
        assert_eq!(json["entries"][0]["amount"], "25400");
        let minimal: SetPaymentsRequest = serde_json::from_value(json!({
            "invoice_number": "SZ-1",
            "entries": [{"date": "2026-07-10", "method": "cash", "amount": 100}],
        }))
        .expect("deserialize");
        assert!(!minimal.additive);
        assert_eq!(minimal.entries[0].amount, dec!(100));
    }
}
