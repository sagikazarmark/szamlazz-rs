//! Contract of the handlers that reverse, delete and read what an order
//! holds: `Szamlazz.Order.storno_invoice`, `delete_proforma` and `get`.
//!
//! [`StornoRequest`] and [`StornoResponse`] are shared with
//! `Szamlazz.Agent.storno`, the by-number storno of an unmanaged invoice:
//! the same request and the same response, with one more outcome
//! ([`StornoOutcome::ManagedByOrder`]) for the invoice that turns out to be
//! an order's.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{ConflictReason, DocumentKind};

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
    #[must_use]
    pub fn new(invoice_number: impl Into<String>) -> Self {
        Self {
            invoice_number: invoice_number.into(),
            comment: None,
        }
    }
}

/// The domain outcome of a storno request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum StornoOutcome {
    /// The invoice is reversed (now or already); `storno_number` is set when
    /// known.
    Reversed,
    /// szamlazz.hu refused the storno; see `code` and `message`.
    Rejected,
    /// The request contradicts what szamlazz.hu holds; see
    /// `conflict_reason`.
    Conflict,
    /// `Szamlazz.Agent.storno` only: the document carries an order number, so
    /// it is managed by the `Order` with key `order_key` — call
    /// `Szamlazz.Order.storno_invoice` there instead.
    ManagedByOrder,
}

/// Output of `Szamlazz.Order.storno_invoice` and `Szamlazz.Agent.storno`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct StornoResponse {
    /// The domain outcome.
    pub outcome: StornoOutcome,
    /// Present when `outcome` is `conflict`.
    #[serde(default)]
    pub conflict_reason: Option<ConflictReason>,
    /// The invoice the request was about.
    pub invoice_number: String,
    /// The storno invoice number, when known.
    #[serde(default)]
    pub storno_number: Option<String>,
    /// The `Order` key managing the document, when `outcome` is
    /// `managed_by_order`.
    #[serde(default)]
    pub order_key: Option<String>,
    /// szamlazz.hu error code on `rejected`.
    #[serde(default)]
    pub code: Option<String>,
    /// szamlazz.hu error message on `rejected`.
    #[serde(default)]
    pub message: Option<String>,
}

impl StornoResponse {
    /// A response with the identity fields set and every optional field
    /// absent.
    pub fn new(outcome: StornoOutcome, invoice_number: impl Into<String>) -> Self {
        Self {
            outcome,
            conflict_reason: None,
            invoice_number: invoice_number.into(),
            storno_number: None,
            order_key: None,
            code: None,
            message: None,
        }
    }

    /// Sets the conflict reason.
    #[must_use]
    pub fn with_conflict_reason(mut self, reason: ConflictReason) -> Self {
        self.conflict_reason = Some(reason);
        self
    }

    /// Sets the storno invoice number.
    #[must_use]
    pub fn with_storno_number(mut self, number: impl Into<String>) -> Self {
        self.storno_number = Some(number.into());
        self
    }

    /// Sets the managing `Order` key.
    #[must_use]
    pub fn with_order_key(mut self, key: impl Into<String>) -> Self {
        self.order_key = Some(key.into());
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

impl DeleteProformaRequest {
    /// A delete request; `force` deletes a proforma with registered payments
    /// too. `Default::default()` is `new(false)`.
    #[must_use]
    pub const fn new(force: bool) -> Self {
        Self { force }
    }
}

/// Output of `Szamlazz.Order.delete_proforma`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct DeleteProformaResponse {
    /// Whether the proforma is deleted (now or already).
    pub deleted: bool,
    /// Why it is not deleted (`proforma_paid`, a szamlazz.hu error code, …),
    /// or `absent` when there was nothing to delete (deleted earlier or
    /// consumed — `get` tells which).
    #[serde(default)]
    pub reason: Option<String>,
}

impl DeleteProformaResponse {
    /// The proforma is deleted.
    #[must_use]
    pub const fn deleted() -> Self {
        Self {
            deleted: true,
            reason: None,
        }
    }

    /// There was nothing to delete: szamlazz.hu holds no proforma under our
    /// external id.
    #[must_use]
    pub fn absent() -> Self {
        Self {
            deleted: true,
            reason: Some("absent".to_owned()),
        }
    }

    /// The proforma is not deleted for `reason`.
    pub fn not_deleted(reason: impl Into<String>) -> Self {
        Self {
            deleted: false,
            reason: Some(reason.into()),
        }
    }
}

/// Output of `Szamlazz.Order.get`: what szamlazz.hu holds under the order's
/// four external ids right now. Carries numbers and totals — never buyer
/// data.
///
/// A slot is `None` when szamlazz.hu holds nothing under its external id
/// *or* when the newest holder of the id fails validation (an external-id
/// collision: another order, kind, account mode or supplier). A read must
/// not fail, so `get` reports such a slot as absent; the issuing handlers
/// refuse the same situation as `conflict{external_id_collision}`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(default)]
#[non_exhaustive]
pub struct OrderStatus {
    /// The proforma, when szamlazz.hu holds one — or, when an invoice or
    /// prepayment of the order references a proforma szamlazz.hu no longer
    /// returns, the consumed proforma.
    pub proforma: Option<DocumentStatus>,
    /// The invoice.
    pub invoice: Option<DocumentStatus>,
    /// The prepayment invoice.
    pub prepayment: Option<DocumentStatus>,
    /// The final invoice.
    #[serde(rename = "final")]
    pub r#final: Option<DocumentStatus>,
}

impl OrderStatus {
    /// The status of the `kind` document.
    #[must_use]
    pub const fn get(&self, kind: DocumentKind) -> Option<&DocumentStatus> {
        match kind {
            DocumentKind::Proforma => self.proforma.as_ref(),
            DocumentKind::Invoice => self.invoice.as_ref(),
            DocumentKind::Prepayment => self.prepayment.as_ref(),
            DocumentKind::Final => self.r#final.as_ref(),
        }
    }

    /// Sets the status of the `kind` document.
    pub fn set(&mut self, kind: DocumentKind, status: Option<DocumentStatus>) {
        match kind {
            DocumentKind::Proforma => self.proforma = status,
            DocumentKind::Invoice => self.invoice = status,
            DocumentKind::Prepayment => self.prepayment = status,
            DocumentKind::Final => self.r#final = status,
        }
    }
}

/// The live view of one document of an order, as szamlazz.hu reports it.
///
/// The state is flattened: `{"number": "SZ-1", "state": "live", …}`,
/// `{"state": "reversed", "storno_number": "SS-1", …}` or
/// `{"state": "consumed", "by": "SZ-1", …}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct DocumentStatus {
    /// The document number.
    pub number: String,
    /// Whether it is live, reversed or (proformas) consumed.
    #[serde(flatten)]
    pub state: DocumentState,
    /// Gross total.
    #[serde(default)]
    pub gross: Option<Decimal>,
    /// Net total.
    #[serde(default)]
    pub net: Option<Decimal>,
    /// Registered credit entry amounts, in the order szamlazz.hu lists them.
    #[serde(default)]
    pub payments: Vec<Decimal>,
    /// The proforma this document converted (`hivdijbekszam`).
    #[serde(default)]
    pub referenced_proforma: Option<String>,
    /// Whether it is an e-invoice; `None` for proformas and unknown codes.
    #[serde(default)]
    pub e_invoice: Option<bool>,
}

impl DocumentStatus {
    /// A status of `number` in `state` with no totals, payments or
    /// references.
    pub fn new(number: impl Into<String>, state: DocumentState) -> Self {
        Self {
            number: number.into(),
            state,
            gross: None,
            net: None,
            payments: Vec::new(),
            referenced_proforma: None,
            e_invoice: None,
        }
    }
}

/// The state of a document as szamlazz.hu reports it.
///
/// Tagged by `state`: `live`, `reversed` (with `storno_number` when known) or
/// `consumed` (with the consuming document in `by`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "state", rename_all = "snake_case")]
#[non_exhaustive]
pub enum DocumentState {
    /// The document exists and is not reversed.
    Live,
    /// The document carries `<sztornozott>true</sztornozott>`.
    Reversed {
        /// The storno invoice number, when known.
        #[serde(default)]
        storno_number: Option<String>,
    },
    /// A proforma szamlazz.hu no longer returns because the document `by`
    /// converted it.
    Consumed {
        /// The invoice or prepayment that references the proforma.
        by: String,
    },
}

#[cfg(test)]
mod tests {
    use rust_decimal::dec;
    use serde_json::json;

    use super::*;
    use crate::contract::document::tests::{refuses_unknown_field, round_trip};

    #[test]
    fn storno_request_round_trips() {
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

    /// A misspelt field is refused, never a silent default: `froce` as
    /// `force: false` would refuse a paid proforma the caller meant to force.
    #[test]
    fn request_types_refuse_unknown_fields() {
        refuses_unknown_field::<StornoRequest>(
            json!({"invoice_number": "SZ-1", "coment": "wrong buyer"}),
            "coment",
        );
        refuses_unknown_field::<DeleteProformaRequest>(json!({"froce": true}), "froce");
    }

    /// The bodies the e2e scenarios send still deserialize: nothing documented
    /// carries a field the contract does not know.
    #[test]
    fn documented_bodies_deserialize() {
        serde_json::from_value::<StornoRequest>(json!({"invoice_number": "SZ-1"}))
            .expect("a storno body");
        serde_json::from_value::<DeleteProformaRequest>(json!({})).expect("an empty delete body");
    }

    #[test]
    fn storno_response_round_trips() {
        let reversed =
            StornoResponse::new(StornoOutcome::Reversed, "SZ-1").with_storno_number("SS-1");
        let json = round_trip(&reversed);
        assert_eq!(json["outcome"], "reversed");
        assert_eq!(json["storno_number"], "SS-1");

        let managed =
            StornoResponse::new(StornoOutcome::ManagedByOrder, "SZ-2").with_order_key("ORD-2");
        let json = round_trip(&managed);
        assert_eq!(json["outcome"], "managed_by_order");
        assert_eq!(json["order_key"], "ORD-2");

        let conflict = StornoResponse::new(StornoOutcome::Conflict, "SZ-3")
            .with_conflict_reason(ConflictReason::NotManaged);
        let json = round_trip(&conflict);
        assert_eq!(json["conflict_reason"], "not_managed");
        let rejected = StornoResponse::new(StornoOutcome::Rejected, "SZ-4")
            .with_code("221")
            .with_message("has corrective");
        round_trip(&rejected);
    }

    #[test]
    fn delete_response_round_trips() {
        let json = round_trip(&DeleteProformaResponse::deleted());
        assert_eq!(json, json!({"deleted": true, "reason": null}));
        let json = round_trip(&DeleteProformaResponse::absent());
        assert_eq!(json, json!({"deleted": true, "reason": "absent"}));
        round_trip(&DeleteProformaResponse::not_deleted("proforma_paid"));
    }

    #[test]
    fn order_status_round_trips_with_flattened_states() {
        let mut status = OrderStatus::default();
        let mut invoice = DocumentStatus::new("SZ-2", DocumentState::Live);
        invoice.gross = Some(dec!(25400));
        invoice.net = Some(dec!(20000));
        invoice.payments = vec![dec!(10000)];
        invoice.referenced_proforma = Some("D-1".to_owned());
        invoice.e_invoice = Some(true);
        status.set(DocumentKind::Invoice, Some(invoice));
        status.proforma = Some(DocumentStatus::new(
            "D-1",
            DocumentState::Consumed {
                by: "SZ-2".to_owned(),
            },
        ));
        status.r#final = Some(DocumentStatus::new(
            "VS-1",
            DocumentState::Reversed {
                storno_number: Some("SS-9".to_owned()),
            },
        ));

        let json = round_trip(&status);
        assert_eq!(json["invoice"]["number"], "SZ-2");
        assert_eq!(json["invoice"]["state"], "live");
        assert_eq!(json["invoice"]["payments"], json!(["10000"]));
        assert_eq!(json["proforma"]["state"], "consumed");
        assert_eq!(json["proforma"]["by"], "SZ-2");
        assert_eq!(json["final"]["state"], "reversed");
        assert_eq!(json["final"]["storno_number"], "SS-9");
        assert_eq!(json["prepayment"], serde_json::Value::Null);
        assert_eq!(status.get(DocumentKind::Final), status.r#final.as_ref());

        let empty: OrderStatus = serde_json::from_value(json!({})).expect("deserialize");
        assert_eq!(empty, OrderStatus::default());
        let bare: DocumentStatus =
            serde_json::from_value(json!({"number": "SZ-1", "state": "reversed"}))
                .expect("deserialize");
        assert_eq!(
            bare.state,
            DocumentState::Reversed {
                storno_number: None
            }
        );
    }
}
