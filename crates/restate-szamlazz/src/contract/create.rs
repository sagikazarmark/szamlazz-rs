//! Contract of the issuing handlers: `Szamlazz.Order.create_proforma`,
//! `create_invoice`, `create_prepayment`, `create_final` and
//! `correct_invoice`.
//!
//! Every one of them takes a [`DocumentInput`] — inside a [`CreateRequest`]
//! or a [`CorrectRequest`] — and answers a [`CreateResponse`], whose
//! [`Outcome`] is the domain result and whose [`ConflictReason`] says why a
//! request contradicts what szamlazz.hu holds. [`ConflictReason`] is shared
//! with the storno handlers (see [`storno`](super::storno)).

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::document::DocumentInput;
use super::{CorrectionId, IssuedKind};

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

impl CorrectRequest {
    /// A corrective of `invoice_number` under `correction_id`.
    #[must_use]
    pub fn new(
        invoice_number: impl Into<String>,
        correction_id: CorrectionId,
        document: DocumentInput,
    ) -> Self {
        Self {
            invoice_number: invoice_number.into(),
            correction_id,
            document,
        }
    }
}

/// The domain outcome of a create or correct request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Outcome {
    /// szamlazz.hu issued the document in this invocation.
    Issued,
    /// A live document of this kind already exists under our external id;
    /// nothing new was issued.
    AlreadyIssued,
    /// szamlazz.hu refused the order number as a duplicate (71/152) and the
    /// external-id re-query found our live document: an earlier execution's
    /// send had landed.
    Reconciled,
    /// The document of this kind was reversed; nothing new was issued. Pass
    /// `reissue: true` (with a new `Idempotency-Key`) to issue a new one.
    Reversed,
    /// szamlazz.hu refused the document; see `code` and `message`.
    Rejected,
    /// The request contradicts what szamlazz.hu holds for the order; see
    /// `conflict_reason`.
    Conflict,
}

/// Why a create, correct or storno request was answered with `outcome:
/// conflict`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConflictReason {
    /// A live prepayment invoice or final invoice (or, for `create_prepayment`,
    /// a live invoice or final invoice) of ours exists for the order (see
    /// `existing_number`): the two chains are exclusive, and the final invoice
    /// keeps the prepayment chain closed after its prepayment is reversed.
    PrepaidChain,
    /// `create_proforma` while the order's invoice, prepayment invoice or
    /// final invoice of ours is live (see `existing_number`): a proforma after
    /// the invoice makes no sense. Not `foreign` — the document is this
    /// order's, issued by this service.
    OrderInvoiced,
    /// `reissue: true` while the document is live.
    Live,
    /// A live invoice-kind document under the order number that is under none
    /// of this order's external ids — another channel or namespace on the
    /// same szamlazz.hu account; see `existing_number`.
    Foreign,
    /// szamlazz.hu refuses the order number as a duplicate (71/152) and no
    /// live document of ours can be found under our external id.
    DuplicateOrderNumber,
    /// A document found under our external id belongs to another order,
    /// kind, account mode or supplier.
    ExternalIdCollision,
    /// `proforma: none` while a live proforma of ours exists.
    ProformaLive,
    /// The referenced proforma cannot be found.
    ProformaMissing,
    /// `create_final` without a prepayment invoice of ours.
    PrepaymentMissing,
    /// `create_final` while the prepayment invoice is reversed.
    PrepaymentReversed,
    /// `correct_invoice` on a reversed invoice.
    BaseReversed,
    /// The document named by number — the invoice to reverse or correct, or
    /// the proforma of `options.proforma: {number}` — does not carry this
    /// order's number (`existing_number` on a create, `invoice_number` on a
    /// storno). Use the managing order, or `Szamlazz.Agent.storno` for an
    /// unmanaged invoice.
    NotManaged,
}

/// Informational flags attached to a successful response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Warning {
    /// The document was issued but szamlazz.hu could not deliver its
    /// notification email (code 56).
    NotificationDeliveryFailed,
}

/// Output of every create and correct handler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct CreateResponse {
    /// The domain outcome.
    pub outcome: Outcome,
    /// Present when `outcome` is `conflict`.
    #[serde(default)]
    pub conflict_reason: Option<ConflictReason>,
    /// The document kind.
    pub kind: IssuedKind,
    /// The external id (`szamlaKulsoAzon`) the document carries.
    pub external_id: String,
    /// The document's number, when one exists.
    #[serde(default)]
    pub invoice_number: Option<String>,
    /// The storno invoice number, when `outcome` is `reversed` and it is
    /// known.
    #[serde(default)]
    pub storno_number: Option<String>,
    /// Net total (`nettó végösszeg`).
    #[serde(default)]
    pub net_total: Option<Decimal>,
    /// Gross total (`bruttó végösszeg`).
    #[serde(default)]
    pub gross_total: Option<Decimal>,
    /// Outstanding amount (`kintlévőség`).
    #[serde(default)]
    pub outstanding: Option<Decimal>,
    /// Buyer-facing account URL (`vevői fiók URL`).
    #[serde(default)]
    pub customer_account_url: Option<String>,
    /// The number of the document a conflict is about (the live document on
    /// `live`, the foreign document on `foreign`, …).
    #[serde(default)]
    pub existing_number: Option<String>,
    /// szamlazz.hu error code on `rejected` (and on
    /// `conflict{duplicate_order_number}`).
    #[serde(default)]
    pub code: Option<String>,
    /// szamlazz.hu error message on `rejected`.
    #[serde(default)]
    pub message: Option<String>,
    /// Informational flags.
    #[serde(default)]
    pub warnings: Vec<Warning>,
}

impl CreateResponse {
    /// A response with the identity fields set and every optional field
    /// absent.
    #[must_use]
    pub fn new(outcome: Outcome, kind: IssuedKind, external_id: impl Into<String>) -> Self {
        Self {
            outcome,
            conflict_reason: None,
            kind,
            external_id: external_id.into(),
            invoice_number: None,
            storno_number: None,
            net_total: None,
            gross_total: None,
            outstanding: None,
            customer_account_url: None,
            existing_number: None,
            code: None,
            message: None,
            warnings: Vec::new(),
        }
    }

    /// A [`Outcome::Conflict`] response with its reason.
    #[must_use]
    pub fn conflict(
        reason: ConflictReason,
        kind: IssuedKind,
        external_id: impl Into<String>,
    ) -> Self {
        let mut response = Self::new(Outcome::Conflict, kind, external_id);
        response.conflict_reason = Some(reason);
        response
    }

    /// Sets the conflict reason.
    #[must_use]
    pub fn with_conflict_reason(mut self, reason: ConflictReason) -> Self {
        self.conflict_reason = Some(reason);
        self
    }

    /// Sets the document's number.
    #[must_use]
    pub fn with_invoice_number(mut self, number: impl Into<String>) -> Self {
        self.invoice_number = Some(number.into());
        self
    }

    /// Sets the storno invoice number.
    #[must_use]
    pub fn with_storno_number(mut self, number: impl Into<String>) -> Self {
        self.storno_number = Some(number.into());
        self
    }

    /// Sets the net total.
    #[must_use]
    pub fn with_net_total(mut self, net_total: Decimal) -> Self {
        self.net_total = Some(net_total);
        self
    }

    /// Sets the gross total.
    #[must_use]
    pub fn with_gross_total(mut self, gross_total: Decimal) -> Self {
        self.gross_total = Some(gross_total);
        self
    }

    /// Sets the outstanding amount.
    #[must_use]
    pub fn with_outstanding(mut self, outstanding: Decimal) -> Self {
        self.outstanding = Some(outstanding);
        self
    }

    /// Sets the buyer-facing account URL.
    #[must_use]
    pub fn with_customer_account_url(mut self, url: impl Into<String>) -> Self {
        self.customer_account_url = Some(url.into());
        self
    }

    /// Sets the number of the document a conflict is about.
    #[must_use]
    pub fn with_existing_number(mut self, number: impl Into<String>) -> Self {
        self.existing_number = Some(number.into());
        self
    }

    /// Sets the szamlazz.hu error code.
    #[must_use]
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Sets the szamlazz.hu error message.
    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Appends a warning.
    #[must_use]
    pub fn with_warning(mut self, warning: Warning) -> Self {
        self.warnings.push(warning);
        self
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::dec;
    use serde_json::json;

    use super::*;
    use crate::contract::document::tests::{refuses_unknown_field, round_trip, sample_document};

    fn correction_id() -> CorrectionId {
        "c-1".parse().expect("valid correction id")
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
    fn correct_request_round_trips() {
        let correct = CorrectRequest {
            invoice_number: "SZ-1".to_owned(),
            correction_id: correction_id(),
            document: sample_document(),
        };
        let json = round_trip(&correct);
        assert_eq!(json["correction_id"], "c-1");
    }

    /// A misspelt field is refused, never a silent default: `resissue` read as
    /// `reissue: false` would answer `reversed` on a document the caller asked
    /// to reissue.
    #[test]
    fn request_types_refuse_unknown_fields() {
        let document = serde_json::to_value(sample_document()).expect("serialize");

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
    }

    /// The externally tagged enum is closed already: a second key beside the
    /// variant is refused by serde itself.
    #[test]
    fn proforma_link_refuses_a_second_key() {
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
    }

    #[test]
    fn create_response_round_trips() {
        let response =
            CreateResponse::new(Outcome::Issued, IssuedKind::Invoice, "acct:ORD-1:invoice")
                .with_invoice_number("SZ-1")
                .with_net_total(dec!(20000))
                .with_gross_total(dec!(25400))
                .with_outstanding(dec!(25400))
                .with_customer_account_url("https://example.test/acct")
                .with_warning(Warning::NotificationDeliveryFailed);
        let json = round_trip(&response);
        assert_eq!(json["outcome"], "issued");
        assert_eq!(json["conflict_reason"], serde_json::Value::Null);
        assert_eq!(json["kind"], "invoice");
        assert_eq!(json.get("gen"), None);
        assert_eq!(json.get("request_id"), None);
        assert_eq!(json["warnings"], json!(["notification_delivery_failed"]));
    }

    #[test]
    fn create_response_conflict_and_rejection() {
        let conflict = CreateResponse::conflict(
            ConflictReason::Live,
            IssuedKind::Invoice,
            "acct:ORD-1:invoice",
        )
        .with_existing_number("SZ-1");
        let json = round_trip(&conflict);
        assert_eq!(json["outcome"], "conflict");
        assert_eq!(json["conflict_reason"], "live");
        assert_eq!(json["existing_number"], "SZ-1");

        let rejected = CreateResponse::new(
            Outcome::Rejected,
            IssuedKind::Corrective,
            "acct:ORD-1:corrective:c-2",
        )
        .with_code("259")
        .with_message("net value mismatch");
        let json = round_trip(&rejected);
        assert_eq!(json["code"], "259");
    }

    #[test]
    fn create_response_defaults_optional_fields() {
        let response: CreateResponse = serde_json::from_value(json!({
            "outcome": "reversed",
            "kind": "final",
            "external_id": "acct:ORD-1:final",
        }))
        .expect("deserialize");
        assert_eq!(response.outcome, Outcome::Reversed);
        assert!(response.warnings.is_empty());
        assert_eq!(response.invoice_number, None);
    }

    #[test]
    fn every_conflict_reason_is_snake_case() {
        let reasons = [
            (ConflictReason::PrepaidChain, "prepaid_chain"),
            (ConflictReason::OrderInvoiced, "order_invoiced"),
            (ConflictReason::Live, "live"),
            (ConflictReason::Foreign, "foreign"),
            (
                ConflictReason::DuplicateOrderNumber,
                "duplicate_order_number",
            ),
            (ConflictReason::ExternalIdCollision, "external_id_collision"),
            (ConflictReason::ProformaLive, "proforma_live"),
            (ConflictReason::ProformaMissing, "proforma_missing"),
            (ConflictReason::PrepaymentMissing, "prepayment_missing"),
            (ConflictReason::PrepaymentReversed, "prepayment_reversed"),
            (ConflictReason::BaseReversed, "base_reversed"),
            (ConflictReason::NotManaged, "not_managed"),
        ];
        for (reason, token) in reasons {
            assert_eq!(
                serde_json::to_value(reason).expect("serialize"),
                json!(token)
            );
        }
    }
}
