//! Contract of the handlers that reverse, delete and read what an order
//! holds: `Szamlazz.Order.storno_invoice`, `delete_proforma` and `get`.
//!
//! [`StornoRequest`] and [`StornoResponse`] are shared with
//! `Szamlazz.Agent.storno`, the by-number storno of an unmanaged invoice:
//! the same request and the same response, with outcomes for an invoice
//! carrying an order number: [`StornoOutcome::ManagedByOrder`] or
//! [`StornoOutcome::UnsupportedOrderNumber`].

use std::fmt;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{ConflictReason, DocumentKind, InvoiceNumber};

/// Input of `Szamlazz.Order.storno_invoice` and `Szamlazz.Agent.storno`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct StornoRequest {
    /// The invoice to reverse.
    pub invoice_number: InvoiceNumber,
    /// Comment placed on the storno invoice.
    #[serde(default)]
    pub comment: Option<String>,
}

impl StornoRequest {
    /// A storno request without a comment.
    #[must_use]
    pub fn new(invoice_number: InvoiceNumber) -> Self {
        Self {
            invoice_number,
            comment: None,
        }
    }
}

/// The domain outcome of a storno request.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
    /// it is managed by the `Order` with key `order_key`: call
    /// `Szamlazz.Order.storno_invoice` there instead.
    ManagedByOrder,
    /// `Szamlazz.Agent.storno` only: the document carries an order number
    /// outside the worker's supported key alphabet. Nothing was sent, and
    /// there is no usable `order_key` to route to.
    UnsupportedOrderNumber,
    /// A token this version does not know, preserved verbatim.
    Other(String),
}

impl StornoOutcome {
    /// Known storno outcomes.
    pub const KNOWN: [Self; 5] = [
        Self::Reversed,
        Self::Rejected,
        Self::Conflict,
        Self::ManagedByOrder,
        Self::UnsupportedOrderNumber,
    ];

    /// The outcome as its wire string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Reversed => "reversed",
            Self::Rejected => "rejected",
            Self::Conflict => "conflict",
            Self::ManagedByOrder => "managed_by_order",
            Self::UnsupportedOrderNumber => "unsupported_order_number",
            Self::Other(token) => token,
        }
    }
}

impl fmt::Display for StornoOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<String> for StornoOutcome {
    fn from(token: String) -> Self {
        match token.as_str() {
            "reversed" => Self::Reversed,
            "rejected" => Self::Rejected,
            "conflict" => Self::Conflict,
            "managed_by_order" => Self::ManagedByOrder,
            "unsupported_order_number" => Self::UnsupportedOrderNumber,
            _ => Self::Other(token),
        }
    }
}

impl From<StornoOutcome> for String {
    fn from(outcome: StornoOutcome) -> Self {
        match outcome {
            StornoOutcome::Other(token) => token,
            known => known.as_str().to_owned(),
        }
    }
}

impl Serialize for StornoOutcome {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for StornoOutcome {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from(String::deserialize(deserializer)?))
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for StornoOutcome {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "StornoOutcome".into()
    }

    fn schema_id() -> std::borrow::Cow<'static, str> {
        concat!(module_path!(), "::StornoOutcome").into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "The domain outcome of a storno request. Known values: reversed, rejected, conflict, managed_by_order (call the managing Order), unsupported_order_number (the document's order number is not a supported Order key; nothing was sent). Other strings are preserved for newer outcomes.",
        })
    }
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

    /// The worker's own `code` on `rejected` for a document szamlazz.hu
    /// cannot reverse (a proforma, a delivery note, a storno): no szamlazz.hu
    /// code, since szamlazz.hu either was not asked or answered a no-op.
    pub const NOT_STORNOABLE: &str = "not_stornoable";

    /// `rejected{code: not_stornoable}` for `invoice_number`, with `message`
    /// saying how the worker knows: the verify saw a `tipus` szamlazz.hu
    /// cannot reverse (nothing was sent), or szamlazz.hu echoed the document
    /// unchanged to the storno it was sent (a success-shaped no-op). The two
    /// sites word the message; the code is one.
    pub fn not_stornoable(invoice_number: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(StornoOutcome::Rejected, invoice_number)
            .with_code(Self::NOT_STORNOABLE)
            .with_message(message)
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
    /// Delete even when the proforma has registered credit entries. szamlazz.hu has
    /// no guard of its own; without `force` a paid proforma is answered
    /// `{deleted: false, reason: "proforma_paid"}`.
    pub force: bool,
}

impl DeleteProformaRequest {
    /// A delete request; `force` deletes a proforma with registered credit entries
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
    /// Why it is not deleted (`proforma_paid`, `external_id_collision`, a
    /// szamlazz.hu error code), or `absent` when there was nothing to delete
    /// (deleted earlier or consumed; `get` tells which).
    #[serde(default)]
    pub reason: Option<DeleteReason>,
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
    pub const fn absent() -> Self {
        Self {
            deleted: true,
            reason: Some(DeleteReason::Absent),
        }
    }

    /// The proforma is not deleted for `reason`.
    #[must_use]
    pub const fn not_deleted(reason: DeleteReason) -> Self {
        Self {
            deleted: false,
            reason: Some(reason),
        }
    }
}

/// The `reason` of a [`DeleteProformaResponse`]: the worker's own tokens for
/// what it decided before a send, or szamlazz.hu's code for what it refused.
/// Serialises as the one string it always was (`absent`, `proforma_paid`,
/// `external_id_collision`, or the code as szamlazz.hu wrote it); `#[non_exhaustive]`
/// and open on the way in, like every response type: a token this version
/// does not know reads as [`DeleteReason::Szamlazz`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DeleteReason {
    /// Nothing under the proforma's external id (deleted earlier or consumed;
    /// `get` tells which). Reported with `deleted: true`.
    Absent,
    /// The proforma has registered credit entries and the request did not
    /// `force`.
    ProformaPaid,
    /// The proforma's external id resolves to another order's or kind's
    /// document, which the handler never touches.
    ExternalIdCollision,
    /// szamlazz.hu refused the deletion with this code.
    Szamlazz(String),
}

impl DeleteReason {
    /// The wire string of [`DeleteReason::Absent`].
    pub const ABSENT: &str = "absent";
    /// The wire string of [`DeleteReason::ProformaPaid`].
    pub const PROFORMA_PAID: &str = "proforma_paid";
    /// The wire string of [`DeleteReason::ExternalIdCollision`].
    pub const EXTERNAL_ID_COLLISION: &str = "external_id_collision";

    /// The reason as its wire string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Absent => Self::ABSENT,
            Self::ProformaPaid => Self::PROFORMA_PAID,
            Self::ExternalIdCollision => Self::EXTERNAL_ID_COLLISION,
            Self::Szamlazz(code) => code,
        }
    }
}

impl fmt::Display for DeleteReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// From the wire string: the three tokens as themselves, anything else as
/// szamlazz.hu's code.
impl From<String> for DeleteReason {
    fn from(reason: String) -> Self {
        match reason.as_str() {
            Self::ABSENT => Self::Absent,
            Self::PROFORMA_PAID => Self::ProformaPaid,
            Self::EXTERNAL_ID_COLLISION => Self::ExternalIdCollision,
            _ => Self::Szamlazz(reason),
        }
    }
}

impl From<DeleteReason> for String {
    fn from(reason: DeleteReason) -> Self {
        match reason {
            DeleteReason::Szamlazz(code) => code,
            token => token.as_str().to_owned(),
        }
    }
}

/// Serializes as the wire string.
impl Serialize for DeleteReason {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// Deserializes from the wire string ([`From<String>`](Self::from)).
impl<'de> Deserialize<'de> for DeleteReason {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from(String::deserialize(deserializer)?))
    }
}

#[cfg(feature = "schemars")]
#[cfg_attr(docsrs, doc(cfg(feature = "schemars")))]
impl schemars::JsonSchema for DeleteReason {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "DeleteReason".into()
    }

    fn schema_id() -> std::borrow::Cow<'static, str> {
        concat!(module_path!(), "::DeleteReason").into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "Why the proforma is not deleted: `proforma_paid`, `external_id_collision` or a szamlazz.hu error code; or `absent` (with `deleted: true`) when there was nothing to delete.",
        })
    }
}

/// Output of `Szamlazz.Order.get`: what szamlazz.hu holds under the order's
/// four external ids right now. Carries numbers and totals, never buyer
/// data.
///
/// A slot is `None` when szamlazz.hu holds nothing under its external id
/// *or* when the newest holder of the id fails validation (an external-id
/// collision: another order or kind). A read must
/// not fail, so `get` reports such a slot as absent; the issuing handlers
/// refuse the same situation as `conflict{external_id_collision}`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(default)]
#[non_exhaustive]
pub struct OrderStatus {
    /// The proforma, when szamlazz.hu holds one; or, when an invoice or
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
    pub credit_entries: Vec<Decimal>,
    /// The proforma this document converted (`hivdijbekszam`).
    #[serde(default)]
    pub referenced_proforma: Option<String>,
    /// Whether it is an e-invoice; `None` for proformas and unknown codes.
    #[serde(default)]
    pub e_invoice: Option<bool>,
}

impl DocumentStatus {
    /// A status of `number` in `state` with no totals, credit entries or
    /// references.
    pub fn new(number: impl Into<String>, state: DocumentState) -> Self {
        Self {
            number: number.into(),
            state,
            gross: None,
            net: None,
            credit_entries: Vec::new(),
            referenced_proforma: None,
            e_invoice: None,
        }
    }
}

/// The state of a document as szamlazz.hu reports it.
///
/// Tagged by `state`: `live`, `reversed` (with `storno_number` when known) or
/// `consumed` (with the consuming document in `by`). Unknown states preserve
/// their token and all payload fields, including when flattened in a status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
#[non_exhaustive]
pub enum DocumentState {
    /// The document exists and is not reversed.
    Live,
    /// The document carries `<sztornozott>true</sztornozott>`.
    Reversed {
        /// The storno invoice number, when known. `Szamlazz.Order.get` never
        /// fills it (finding the storno would take the order-number hint,
        /// which shows only the newest document); the create and storno
        /// handlers report it in their own responses when the hint yields it.
        storno_number: Option<String>,
    },
    /// A proforma szamlazz.hu no longer returns because the document `by`
    /// converted it.
    Consumed {
        /// The invoice or prepayment that references the proforma.
        by: String,
    },
    /// A newer state and its payload, preserved without interpreting it.
    #[serde(untagged)]
    Other {
        /// The unknown state token.
        state: String,
        /// The state's payload (excluding `state`).
        #[serde(flatten)]
        fields: serde_json::Map<String, serde_json::Value>,
    },
}

impl DocumentState {
    /// Known state tokens; payloads are carried by the variants.
    pub const KNOWN: [&'static str; 3] = ["live", "reversed", "consumed"];

    /// The state token, including an unknown token verbatim.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Live => "live",
            Self::Reversed { .. } => "reversed",
            Self::Consumed { .. } => "consumed",
            Self::Other { state, .. } => state,
        }
    }
}

impl<'de> Deserialize<'de> for DocumentState {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            state: String,
            #[serde(flatten)]
            fields: serde_json::Map<String, serde_json::Value>,
        }
        let Wire { state, mut fields } = Wire::deserialize(deserializer)?;
        match state.as_str() {
            "live" => Ok(Self::Live),
            "reversed" => Ok(Self::Reversed {
                storno_number: serde_json::from_value(
                    fields.remove("storno_number").unwrap_or_default(),
                )
                .map_err(serde::de::Error::custom)?,
            }),
            "consumed" => Ok(Self::Consumed {
                by: serde_json::from_value(
                    fields
                        .remove("by")
                        .ok_or_else(|| serde::de::Error::missing_field("by"))?,
                )
                .map_err(serde::de::Error::custom)?,
            }),
            _ => Ok(Self::Other { state, fields }),
        }
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for DocumentState {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "DocumentState".into()
    }

    fn schema_id() -> std::borrow::Cow<'static, str> {
        concat!(module_path!(), "::DocumentState").into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "object",
            "description": "The document state: live, reversed (optional storno_number), or consumed (required by). Unknown state strings and their payload fields are preserved.",
            "required": ["state"],
            "properties": { "state": { "type": "string" } },
            "allOf": [
                {
                    "if": { "properties": { "state": { "const": "reversed" } } },
                    "then": { "properties": { "storno_number": { "type": ["string", "null"] } } }
                },
                {
                    "if": { "properties": { "state": { "const": "consumed" } } },
                    "then": { "required": ["by"], "properties": { "by": { "type": "string" } } }
                }
            ]
        })
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::dec;
    use serde_json::json;

    use super::*;
    use crate::contract::document::tests::{refuses_unknown_field, round_trip};

    #[test]
    fn unknown_document_states_preserve_payloads_inside_the_full_order_status() {
        let mut wire = serde_json::to_value(OrderStatus {
            invoice: Some(DocumentStatus::new("SZ-1", DocumentState::Live)),
            ..OrderStatus::default()
        })
        .expect("json");
        wire["invoice"]["state"] = json!(" future_state/é ");
        wire["invoice"]["gross"] = json!("25400");
        wire["invoice"]["credit_entries"] = json!(["1000"]);
        wire["invoice"]["future_details"] = json!({"nested": [null, true, 17, "é"]});
        // A newer state can use a known state's payload name differently.
        wire["invoice"]["by"] = json!({"new": "shape"});
        let decoded: OrderStatus = serde_json::from_value(wire.clone()).expect("open status");
        let invoice = decoded.invoice.as_ref().expect("invoice");
        assert_eq!(invoice.gross, Some(dec!(25400)));
        let DocumentState::Other { state, fields } = &invoice.state else {
            panic!("unknown state must not be classified: {:?}", invoice.state);
        };
        assert_eq!(state, " future_state/é ");
        assert_eq!(invoice.state.as_str(), state);
        assert_eq!(fields["future_details"], wire["invoice"]["future_details"]);
        assert!(
            !fields.contains_key("gross"),
            "sibling fields stay with the status"
        );
        assert_eq!(serde_json::to_value(decoded).expect("json"), wire);
    }

    #[test]
    fn known_document_state_payloads_stay_validated() {
        for wire in [
            json!({"state": "consumed"}),
            json!({"state": "consumed", "by": null}),
            json!({"state": "reversed", "storno_number": 17}),
            json!({"state": 17}),
            json!({}),
        ] {
            assert!(serde_json::from_value::<DocumentState>(wire).is_err());
        }
        for state in [
            DocumentState::Live,
            DocumentState::Reversed {
                storno_number: None,
            },
            DocumentState::Consumed {
                by: "SZ-1".to_owned(),
            },
        ] {
            assert!(DocumentState::KNOWN.contains(&state.as_str()));
            round_trip(&state);
        }
        let wire = json!({"state": "future", "storno_number": {"unknown": true}});
        let decoded: DocumentState = serde_json::from_value(wire.clone()).expect("unknown payload");
        assert_eq!(serde_json::to_value(decoded).expect("json"), wire);
    }

    #[test]
    fn storno_request_round_trips() {
        let sz_1 = || "SZ-1".parse::<InvoiceNumber>().expect("valid number");
        let mut storno = StornoRequest::new(sz_1());
        storno.comment = Some("wrong buyer".to_owned());
        round_trip(&storno);
        let bare: StornoRequest =
            serde_json::from_value(json!({"invoice_number": "SZ-1"})).expect("deserialize");
        assert_eq!(bare, StornoRequest::new(sz_1()));
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
    fn known_storno_tokens_include_unsupported_order_number() {
        let expected = [
            (StornoOutcome::Reversed, "reversed"),
            (StornoOutcome::Rejected, "rejected"),
            (StornoOutcome::Conflict, "conflict"),
            (StornoOutcome::ManagedByOrder, "managed_by_order"),
            (
                StornoOutcome::UnsupportedOrderNumber,
                "unsupported_order_number",
            ),
        ];
        assert_eq!(StornoOutcome::KNOWN.len(), expected.len());
        for (outcome, token) in expected {
            assert!(StornoOutcome::KNOWN.contains(&outcome));
            assert_eq!(outcome.as_str(), token);
            let response = StornoResponse::new(outcome, "SZ-1");
            assert_eq!(round_trip(&response)["outcome"], token);
        }
    }

    #[test]
    fn an_unknown_storno_outcome_preserves_the_response() {
        let mut wire = serde_json::to_value(
            StornoResponse::new(StornoOutcome::Reversed, "SZ-1")
                .with_storno_number("SS-1")
                .with_message("a newer worker's explanation"),
        )
        .expect("json");
        wire["outcome"] = json!("future_storno_outcome");
        let decoded: StornoResponse = serde_json::from_value(wire.clone()).expect("open");
        assert_eq!(
            decoded.outcome,
            StornoOutcome::Other("future_storno_outcome".to_owned())
        );
        assert_eq!(serde_json::to_value(decoded).expect("json"), wire);
    }

    /// The delete response's `reason` is the worker's own token or
    /// szamlazz.hu's code, on the wire the one string it always was; a token
    /// this version does not know reads as a szamlazz.hu code (the response
    /// stays open).
    #[test]
    fn delete_response_round_trips() {
        let json = round_trip(&DeleteProformaResponse::deleted());
        assert_eq!(json, json!({"deleted": true, "reason": null}));
        let json = round_trip(&DeleteProformaResponse::absent());
        assert_eq!(json, json!({"deleted": true, "reason": "absent"}));
        for (reason, wire) in [
            (DeleteReason::ProformaPaid, "proforma_paid"),
            (DeleteReason::ExternalIdCollision, "external_id_collision"),
            (DeleteReason::Szamlazz("335".to_owned()), "335"),
        ] {
            let json = round_trip(&DeleteProformaResponse::not_deleted(reason.clone()));
            assert_eq!(
                json,
                json!({"deleted": false, "reason": wire}),
                "{reason:?}"
            );
            assert_eq!(reason.to_string(), wire);
            assert_eq!(String::from(reason.clone()), wire);
            assert_eq!(DeleteReason::from(wire.to_owned()), reason);
        }
        assert_eq!(
            serde_json::from_value::<DeleteReason>(json!("later_token")).expect("open"),
            DeleteReason::Szamlazz("later_token".to_owned())
        );
    }

    /// `not_stornoable` is the worker's own `code` on a storno `rejected`,
    /// built by one constructor; the message is the site's (the verify saw
    /// the kind, or szamlazz.hu echoed the document).
    #[test]
    fn not_stornoable_is_one_code_with_the_sites_message() {
        let response = StornoResponse::not_stornoable("D-1", "only invoices can be stornoed");
        assert_eq!(response.outcome, StornoOutcome::Rejected);
        assert_eq!(response.invoice_number, "D-1");
        assert_eq!(
            response.code.as_deref(),
            Some(StornoResponse::NOT_STORNOABLE)
        );
        assert_eq!(
            response.message.as_deref(),
            Some("only invoices can be stornoed")
        );
        assert_eq!(response.conflict_reason, None);
        let json = round_trip(&response);
        assert_eq!(json["code"], "not_stornoable");
    }

    #[test]
    fn order_status_round_trips_with_flattened_states() {
        let mut status = OrderStatus::default();
        let mut invoice = DocumentStatus::new("SZ-2", DocumentState::Live);
        invoice.gross = Some(dec!(25400));
        invoice.net = Some(dec!(20000));
        invoice.credit_entries = vec![dec!(10000)];
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
        assert_eq!(json["invoice"]["credit_entries"], json!(["10000"]));
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
