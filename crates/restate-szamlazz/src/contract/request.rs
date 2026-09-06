//! Handler inputs of the `Szamlazz.Order` Virtual Object and the
//! `Szamlazz.Agent` service.
//!
//! Every request type refuses a field it does not know
//! (`#[serde(deny_unknown_fields)]`, `additionalProperties: false` in the
//! schema): a misspelt `reissue` or `additive` is an error naming the field,
//! never a silent default. Response types stay permissive — a client must
//! tolerate fields added later.

use jiff::civil::Date;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use szamlazz_agent::ops::credit_entry::CreditEntry;

use super::CorrectionId;
use super::document::{DocumentInput, PaymentMethod};

/// Input of `Szamlazz.Order.create_proforma`, `create_invoice`,
/// `create_prepayment` and `create_final`.
///
/// The retry identity of a request is Restate's ingress `Idempotency-Key`;
/// the request carries none of its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreateRequest {
    /// The document to issue.
    pub document: DocumentInput,
    /// Kind-specific options; all default.
    #[serde(default)]
    pub options: CreateOptions,
}

impl CreateRequest {
    /// A create request with default [`CreateOptions`].
    #[must_use]
    pub fn new(document: DocumentInput) -> Self {
        Self {
            document,
            options: CreateOptions::default(),
        }
    }
}

/// Options of a create request.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(default, deny_unknown_fields)]
pub struct CreateOptions {
    /// Issue a new document after the existing one was reversed — by this
    /// service, the UI or anyone. Without it a reversed document answers
    /// `outcome: reversed`; with it a live document answers
    /// `conflict{live}`, so the flag can never cause a duplicate.
    pub reissue: bool,
    /// Which proforma the invoice converts (`create_invoice` only; the other
    /// kinds refuse anything but `auto` as `invalid_input`). A prepayment
    /// invoice cannot carry the reference — szamlazz.hu converts the order's
    /// live proforma by shared order number on its own.
    pub proforma: ProformaLink,
}

/// How a create request refers to a proforma.
///
/// Serialises as `"auto"`, `"none"` or `{"number": "D-…"}`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProformaLink {
    /// Reference the order's live proforma when szamlazz.hu has one under our
    /// external id; otherwise reference none.
    #[default]
    Auto,
    /// Reference no proforma. Refused with `conflict{proforma_live}` while a
    /// live proforma of ours exists, because szamlazz.hu links by shared order
    /// number regardless.
    None,
    /// Reference a proforma by number. Checked like every document found by
    /// number: `conflict{proforma_missing}` when szamlazz.hu does not know
    /// it, `conflict{not_managed}` when it does not carry this order's
    /// number, the `account_mismatch` fault when it belongs to another
    /// szamlazz.hu account, `invalid_input` when it is not a proforma.
    Number(String),
}

/// Input of `Szamlazz.Order.correct_invoice`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CorrectRequest {
    /// The invoice being corrected; must carry this order's number.
    pub invoice_number: String,
    /// The identity of this corrective. A new id issues a new corrective by
    /// contract; the same id finds the one it issued.
    pub correction_id: CorrectionId,
    /// The corrective document.
    pub document: DocumentInput,
}

/// Input of `Szamlazz.Order.storno_invoice` and `Szamlazz.Agent.storno`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
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

/// Input of `Szamlazz.Order.delete_proforma`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(default, deny_unknown_fields)]
pub struct DeleteProformaRequest {
    /// Delete even when the proforma has registered payments. szamlazz.hu has
    /// no guard of its own; without `force` a paid proforma is
    /// `rejected{proforma_paid}`.
    pub force: bool,
}

/// Input of `Szamlazz.Agent.query`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
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

/// Input of `Szamlazz.Agent.set_payments`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct SetPaymentsRequest {
    /// The invoice to register credit entries on.
    pub invoice_number: String,
    /// The credit entries (`jóváírások`); szamlazz.hu accepts at most five.
    pub entries: Vec<PaymentEntry>,
    /// Add to the existing entries instead of replacing them.
    ///
    /// **At-least-once.** Replacing is idempotent — a repeat sends the same
    /// final state — but additive entries are appended by every send that
    /// reaches szamlazz.hu, and the handler cannot tell a lost reply from a
    /// lost request: an `outcome_unknown` fault, or the handler's one retry
    /// after a crash, may have landed the entries already. A caller that sees
    /// `outcome_unknown` on an additive call queries the invoice
    /// (`Szamlazz.Agent.query`) before re-sending.
    #[serde(default)]
    pub additive: bool,
}

/// One credit entry (`jóváírás`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
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

impl From<&PaymentEntry> for CreditEntry {
    fn from(entry: &PaymentEntry) -> Self {
        let mut credit = Self::new(entry.date, entry.method.clone().into(), entry.amount);
        credit.description.clone_from(&entry.description);
        credit
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use rust_decimal::dec;
    use serde_json::json;

    use super::*;
    use crate::contract::document::tests::{refuses_unknown_field, sample_document};

    fn correction_id() -> CorrectionId {
        "c-1".parse().expect("valid correction id")
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
        let mut request = CreateRequest::new(sample_document());
        request.options.reissue = true;
        request.options.proforma = ProformaLink::Number("D-1".to_owned());
        let json = round_trip(&request);
        assert_eq!(json.get("request_id"), None);
        assert_eq!(json["options"]["reissue"], true);
        assert_eq!(json["options"]["proforma"], json!({"number": "D-1"}));
    }

    #[test]
    fn create_request_defaults_options() {
        let request: CreateRequest = serde_json::from_value(json!({
            "document": serde_json::to_value(sample_document()).expect("serialize"),
        }))
        .expect("deserialize");
        assert_eq!(request.options, CreateOptions::default());
        assert_eq!(request.options.proforma, ProformaLink::Auto);
        assert!(!request.options.reissue);
    }

    #[test]
    fn proforma_link_wire_shapes() {
        let cases = [
            (ProformaLink::Auto, json!("auto")),
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
            correction_id: correction_id(),
            document: sample_document(),
        };
        let json = round_trip(&correct);
        assert_eq!(json["correction_id"], "c-1");
        let mut storno = StornoRequest::new("SZ-1");
        storno.comment = Some("wrong buyer".to_owned());
        round_trip(&storno);
        let bare: StornoRequest =
            serde_json::from_value(json!({"invoice_number": "SZ-1"})).expect("deserialize");
        assert_eq!(bare, StornoRequest::new("SZ-1"));
    }

    #[test]
    fn delete_request_defaults_to_false() {
        assert_eq!(
            serde_json::from_value::<DeleteProformaRequest>(json!({})).expect("deserialize"),
            DeleteProformaRequest { force: false }
        );
        round_trip(&DeleteProformaRequest { force: true });
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
                Selector::ExternalId("acct:ORD-1:invoice".to_owned()),
                json!({"external_id": "acct:ORD-1:invoice"}),
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

    /// A misspelt field is refused, never a silent default: `resissue` read as
    /// `reissue: false` would answer `reversed` on a document the caller asked
    /// to reissue, `aditive` as `additive: false` would *replace* the
    /// invoice's credit entries, `froce` as `force: false` would refuse a paid
    /// proforma the caller meant to force.
    #[test]
    fn request_types_refuse_unknown_fields() {
        let document = serde_json::to_value(sample_document()).expect("serialize");
        let entry = json!({"date": "2026-07-10", "method": "cash", "amount": 100});

        refuses_unknown_field::<CreateRequest>(
            json!({"document": document, "optoins": {}}),
            "optoins",
        );
        refuses_unknown_field::<CreateRequest>(
            json!({"document": document, "options": {"resissue": true}}),
            "resissue",
        );
        refuses_unknown_field::<CreateRequest>(
            json!({"document": document, "options": {"reissue": true, "proforma": "auto", "x": 1}}),
            "x",
        );

        refuses_unknown_field::<CorrectRequest>(
            json!({
                "invoice_number": "SZ-1",
                "correction_id": "c-1",
                "document": document,
                "reissue": true,
            }),
            "reissue",
        );
        let mut corrected = document.clone();
        corrected["buyer"]["tax_numer"] = json!("12345678-2-42");
        refuses_unknown_field::<CorrectRequest>(
            json!({"invoice_number": "SZ-1", "correction_id": "c-1", "document": corrected}),
            "tax_numer",
        );

        refuses_unknown_field::<StornoRequest>(
            json!({"invoice_number": "SZ-1", "coment": "wrong buyer"}),
            "coment",
        );

        refuses_unknown_field::<DeleteProformaRequest>(json!({"froce": true}), "froce");

        refuses_unknown_field::<QueryRequest>(
            json!({"selector": {"invoice_number": "SZ-1"}, "invoice_number": "SZ-1"}),
            "invoice_number",
        );

        refuses_unknown_field::<SetPaymentsRequest>(
            json!({"invoice_number": "SZ-1", "entries": [entry], "aditive": true}),
            "aditive",
        );
        refuses_unknown_field::<SetPaymentsRequest>(
            json!({
                "invoice_number": "SZ-1",
                "entries": [{"date": "2026-07-10", "method": "cash", "amount": 100, "note": "x"}],
            }),
            "note",
        );
        refuses_unknown_field::<PaymentEntry>(
            json!({"date": "2026-07-10", "method": "cash", "amount": 100, "descripton": "x"}),
            "descripton",
        );
    }

    /// The externally tagged enums are closed already: a second key beside the
    /// variant is refused by serde itself.
    #[test]
    fn selector_refuses_a_second_key() {
        let error = serde_json::from_value::<QueryRequest>(json!({
            "selector": {"invoice_number": "SZ-1", "order_number": "ORD-1"},
        }))
        .expect_err("two selectors at once are refused");
        assert!(error.to_string().contains("single key"), "{error}");
        assert!(
            serde_json::from_str::<ProformaLink>(r#"{"number": "D-1", "x": 1}"#).is_err(),
            "a second key beside the variant is refused on the wire too"
        );
    }

    /// The bodies the READMEs and the e2e scenarios send still deserialize:
    /// nothing documented carries a field the contract does not know.
    #[test]
    fn documented_bodies_deserialize() {
        // crates/restate-szamlazz-endpoint/README.md — the curl example.
        let readme_curl = r#"{
    "document": {
      "buyer": { "name": "Kovács Bt.", "zip": "2030", "city": "Érd", "address": "Tárnoki út 23." },
      "items": [{ "name": "Consulting", "quantity": "1", "unit": "db", "unit_price": "1000", "vat_rate": "27" }],
      "fulfillment_date": "2026-09-03",
      "due_date": "2026-09-11",
      "payment_method": "transfer"
    }
  }"#;
        let request: CreateRequest =
            serde_json::from_str(readme_curl).expect("the README curl body");
        assert_eq!(request.options, CreateOptions::default());
        assert_eq!(request.document.buyer.name, "Kovács Bt.");

        // tests/service.rs — the literal bodies of the e2e scenarios.
        let document = serde_json::to_value(sample_document()).expect("serialize");
        serde_json::from_value::<CreateRequest>(json!({"document": document}))
            .expect("a bare create body");
        serde_json::from_value::<CreateRequest>(
            json!({"document": document, "options": {"reissue": true}}),
        )
        .expect("create_body");
        serde_json::from_value::<CreateRequest>(
            json!({"document": document, "options": {"proforma": "none"}}),
        )
        .expect("the prepayment scenario's body");
        serde_json::from_value::<StornoRequest>(json!({"invoice_number": "SZ-1"}))
            .expect("a storno body");
        serde_json::from_value::<QueryRequest>(json!({"selector": {"invoice_number": "SZ-12"}}))
            .expect("a query body");
        serde_json::from_value::<DeleteProformaRequest>(json!({})).expect("an empty delete body");
    }

    #[test]
    fn payment_entry_converts_to_agent() {
        let entry = PaymentEntry {
            date: date(2026, 7, 10),
            method: PaymentMethod::Card,
            amount: dec!(25400),
            description: Some("card".to_owned()),
        };
        let credit = CreditEntry::from(&entry);
        assert_eq!(credit.date, date(2026, 7, 10));
        assert_eq!(credit.method, szamlazz_agent::PaymentMethod::Card);
        assert_eq!(credit.amount, dec!(25400));
        assert_eq!(credit.description.as_deref(), Some("card"));

        let bare = PaymentEntry {
            method: PaymentMethod::Other("Bitcoin".to_owned()),
            description: None,
            ..entry
        };
        let credit = CreditEntry::from(&bare);
        assert_eq!(credit.method.as_wire(), "Bitcoin");
        assert_eq!(credit.description, None);
    }
}
