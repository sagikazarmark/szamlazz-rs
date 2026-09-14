//! Operator recovery and the versioned, fail-closed unresolved-write state.

use serde::{Deserialize, Deserializer, Serialize};

use crate::identity::{IssuedKind, Namespace, OrderKey};

/// A provider document number submitted as recovery evidence. The same type
/// is accepted by explicit exact-number queries, without conversion or narrowing.
pub type EvidenceNumber = super::ProviderDocumentNumber;

/// A recovery number that cannot identify a queryable document.
pub type InvalidEvidenceNumber = super::InvalidProviderDocumentNumber;

super::object::object_input! {
/// Recovery intent without buyer data, line items, XML or credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum WriteOperation {
    /// Issuance, including its expected old holder and corrective base.
    Create {
        /// The kind being issued.
        kind: IssuedKind,
        /// The old reversed holder, for explicit reissue.
        expected_number: Option<String>,
        /// The original of a corrective.
        corrected_number: Option<String>,
    },
    /// Reversal of the exact original.
    Storno {
        /// Original invoice number.
        number: String,
    },
    /// Deletion of the pinned proforma, selected by namespace or exact number.
    /// Recovery needs the target number, not the original selection mode.
    Delete {
        /// Pinned proforma number.
        number: String,
    },
}
}

/// Schema version accepted by this deployment. Unknown versions fail closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(into = "u8")]
pub struct MarkerVersion;

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for MarkerVersion {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "MarkerVersion".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"type":"integer", "const":1})
    }
}

impl From<MarkerVersion> for u8 {
    fn from(_: MarkerVersion) -> Self {
        1
    }
}

impl<'de> Deserialize<'de> for MarkerVersion {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        match u8::deserialize(de)? {
            1 => Ok(Self),
            _ => Err(serde::de::Error::custom(
                "unsupported unresolved-write version",
            )),
        }
    }
}

super::object::object_input! {
/// The sole Order state: a possibly effective write awaiting conclusive evidence.
/// Its schema crosses deployments and is deliberately closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct UnresolvedWrite {
    /// Operation-specific replay execution contract, absent on protected
    /// markers. Unknown tokens cannot authorize recovery or resend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_contract: Option<ReplayExecutionContract>,
    /// Pinned proforma reference submitted with ordinary or prepayment issuance. Omission
    /// means no explicit link; recovery establishes issuance, not linkage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proforma_number: Option<String>,
    /// Exact prepayment selected for final issuance; never substituted on replay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepayment_number: Option<String>,
    /// Explicit state schema version.
    pub version: MarkerVersion,
    /// Unique marker token, the original invocation id.
    pub token: String,
    /// Invocation retaining or formerly holding the lock.
    pub owner_invocation: String,
    /// Arming time, for operator observation only; never an expiry.
    pub created_at: String,
    /// Restate scope, including unscoped as a distinct value.
    pub scope: Option<String>,
    /// Exact Order key.
    #[serde(deserialize_with = "exact_order_key")]
    #[cfg_attr(feature = "schemars", schemars(with = "String"))]
    pub order: OrderKey,
    /// Pinned deployment namespace.
    #[cfg_attr(feature = "schemars", schemars(with = "String"))]
    pub namespace: Namespace,
    /// External identity of the write. For deletion this is the Order's proforma
    /// slot for correlation, not evidence that the pinned number occupies it.
    pub external_id: String,
    /// Pinned resolver-owned account id.
    pub account_id: String,
    /// Pinned Számla Agent endpoint.
    pub endpoint: String,
    /// Stable non-secret credential reference.
    pub credential_ref: String,
    /// Minimal operation-specific recovery intent.
    pub operation: WriteOperation,
}
}

/// The approved replay-risk contracts; never inferred from the hosting target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ReplayExecutionContract {
    /// Ordinary issuance with fresh checks before unfinished-run resubmission.
    #[serde(rename = "request_response_ordinary_v1")]
    RequestResponseOrdinaryV1,
    /// Proforma issuance with fresh invoice-family guards on unfinished replay.
    #[serde(rename = "request_response_proforma_v1")]
    RequestResponseProformaV1,
    /// Prepayment issuance with pinned proforma and fresh exclusivity checks.
    #[serde(rename = "request_response_prepayment_v1")]
    RequestResponsePrepaymentV1,
    /// Final issuance with a pinned prepayment prerequisite.
    #[serde(rename = "request_response_final_v1")]
    RequestResponseFinalV1,
    /// Exact-target deletion; only an acknowledged deletion clears automatically.
    #[serde(rename = "request_response_delete_v1")]
    RequestResponseDeleteV1 {
        /// Selection mode retained before sending.
        mode: super::DeleteMode,
        /// Whether the caller explicitly bypassed the credit-entry guard.
        force: bool,
        /// Provider id of the verified target, checked on every open execution.
        document_id: i64,
    },
    /// Order reversal with a pinned verified original and fresh replay guards.
    #[serde(rename = "request_response_storno_v1")]
    RequestResponseStornoV1 {
        /// Provider id of the original.
        document_id: i64,
        /// Original fulfillment date repeated by every send.
        fulfillment_date: szamlazz_agent::Date,
        /// Derived invoice form sent on reversal.
        e_invoice: bool,
        /// Original projected appearance code.
        appearance: i64,
    },
}

impl ReplayExecutionContract {
    /// The same pinned deletion facts are used at admission and fresh verification.
    pub(crate) fn for_delete(request: &super::DeleteProformaRequest, document_id: i64) -> Self {
        Self::RequestResponseDeleteV1 {
            mode: request.mode,
            force: request.force,
            document_id,
        }
    }
}

impl<'de> Deserialize<'de> for ReplayExecutionContract {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = ReplayExecutionContract;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a known execution contract")
            }
            fn visit_str<E: serde::de::Error>(self, token: &str) -> Result<Self::Value, E> {
                match token {
                    "request_response_ordinary_v1" => {
                        Ok(ReplayExecutionContract::RequestResponseOrdinaryV1)
                    }
                    "request_response_proforma_v1" => {
                        Ok(ReplayExecutionContract::RequestResponseProformaV1)
                    }
                    "request_response_prepayment_v1" => {
                        Ok(ReplayExecutionContract::RequestResponsePrepaymentV1)
                    }
                    "request_response_final_v1" => {
                        Ok(ReplayExecutionContract::RequestResponseFinalV1)
                    }
                    _ => Err(E::custom("unknown execution contract")),
                }
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                let result = match map.next_key::<String>()?.as_deref() {
                    Some("request_response_delete_v1") => {
                        let payload: DeleteExecution = map.next_value()?;
                        ReplayExecutionContract::RequestResponseDeleteV1 {
                            mode: payload.mode,
                            force: payload.force,
                            document_id: payload.document_id,
                        }
                    }
                    Some("request_response_storno_v1") => {
                        let payload: StornoExecution = map.next_value()?;
                        ReplayExecutionContract::RequestResponseStornoV1 {
                            document_id: payload.document_id,
                            fulfillment_date: payload.fulfillment_date,
                            e_invoice: payload.e_invoice,
                            appearance: payload.appearance,
                        }
                    }
                    _ => return Err(serde::de::Error::custom("unknown execution contract")),
                };
                if map.next_key::<String>()?.is_some() {
                    return Err(serde::de::Error::custom(
                        "execution contract requires one variant",
                    ));
                }
                Ok(result)
            }
        }
        de.deserialize_any(Visitor)
    }
}

super::object::object_input! {
#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct DeleteExecution { mode:super::DeleteMode, force:bool, document_id:i64 }
}

super::object::object_input! {
#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct StornoExecution {
    document_id:i64,
    #[serde(deserialize_with = "super::date::required")]
    fulfillment_date:szamlazz_agent::Date,
    e_invoice:bool,
    appearance:i64
}
}

fn exact_order_key<'de, D: Deserializer<'de>>(de: D) -> Result<OrderKey, D::Error> {
    let value = String::deserialize(de)?;
    if value.trim() != value {
        return Err(serde::de::Error::custom(
            "marker order must not have leading or trailing whitespace",
        ));
    }
    value.parse().map_err(serde::de::Error::custom)
}

/// Explicit affirmative operator assertion, never vendor proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(into = "bool")]
pub struct NonExecutionAttestation;

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for NonExecutionAttestation {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "NonExecutionAttestation".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"type":"boolean", "const":true})
    }
}

impl From<NonExecutionAttestation> for bool {
    fn from(_: NonExecutionAttestation) -> Self {
        true
    }
}

impl<'de> Deserialize<'de> for NonExecutionAttestation {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        if bool::deserialize(de)? {
            Ok(Self)
        } else {
            Err(serde::de::Error::custom(
                "must attest that the exact request did not execute and cannot execute later",
            ))
        }
    }
}

/// Affirmative assertion that the exact write completed and cannot execute later.
/// This is operator evidence, never a fact independently proved by the worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(into = "bool")]
pub struct CompletionAttestation;

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for CompletionAttestation {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "CompletionAttestation".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"type":"boolean", "const":true})
    }
}

impl From<CompletionAttestation> for bool {
    fn from(_: CompletionAttestation) -> Self {
        true
    }
}

impl<'de> Deserialize<'de> for CompletionAttestation {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        if bool::deserialize(de)? {
            Ok(Self)
        } else {
            Err(serde::de::Error::custom(
                "must attest that the exact write completed and cannot execute later",
            ))
        }
    }
}

super::object::object_input! {
/// The operation-specific result independently established by an operator.
/// The audit record must tie it to the exact marker, including corrective base
/// or reissue intent. Merely observing an absent document is insufficient.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum AttestedCompletion {
    /// The requested document was issued, even if subsequently consumed or reversed.
    Issued {
        /// Issued document number, never the expected old reissue target.
        number: EvidenceNumber,
    },
    /// The exact original was reversed by this storno document.
    Reversed {
        /// Storno document number, distinct from the original in the marker.
        number: EvidenceNumber,
    },
    /// The exact pinned proforma was deleted.
    Deleted {
        /// Deleted proforma number; must equal the marker's target.
        number: crate::identity::InvoiceNumber,
    },
}
}

super::object::object_input! {
/// Evidence an authorized operator submits; no generic clearance exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum RecoveryEvidence {
    /// Independently query and validate the document on the pinned account.
    Document {
        /// Candidate issued document or storno number.
        number: EvidenceNumber,
    },
    /// Audited operator assertion that the exact request cannot have an effect.
    NotExecuted {
        /// Reference to the operator's durable incident/evidence record.
        audit_reference: String,
        /// Must be explicitly true. Time elapsed is not evidence.
        did_not_execute_and_cannot_execute_later: NonExecutionAttestation,
    },
    /// Audited positive settlement unavailable through the vendor query surface.
    /// Requires independent evidence of this exact write's completed effect and
    /// that no delayed execution remains. Empty queries or elapsed time do not qualify.
    Completed {
        /// Reference to the durable incident record containing that evidence.
        audit_reference: String,
        /// Operation-specific result matching the marker's intent.
        completion: AttestedCompletion,
        /// Must be explicitly true; this remains an operator assertion.
        completed_and_cannot_execute_later: CompletionAttestation,
    },
}
}

super::object::object_input! {
/// Exclusive operator recovery; the complete marker must match exactly.
/// The host authenticates and authorizes the caller before invoking recovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct RecoveryRequest {
    /// Nonblank operator identity supplied by the host for audit attribution.
    /// Preserved verbatim; this field does not grant or prove authorization.
    #[serde(deserialize_with = "nonblank_operator")]
    #[cfg_attr(
        feature = "schemars",
        schemars(
            with = "String",
            length(min = 1),
            regex(
                pattern = r"[^\u0009-\u000D\u0020\u0085\u00A0\u1680\u2000-\u200A\u2028\u2029\u202F\u205F\u3000]"
            )
        )
    )]
    pub operator: String,
    /// Exact marker observed under the same scope and Order key.
    pub marker: UnresolvedWrite,
    /// Evidence permitting settlement, never permission to send.
    pub evidence: RecoveryEvidence,
}
}

fn nonblank_operator<'de, D: Deserializer<'de>>(de: D) -> Result<String, D::Error> {
    let operator = String::deserialize(de)?;
    if operator.trim().is_empty() {
        return Err(serde::de::Error::custom(
            "recovery operator must not be blank",
        ));
    }
    Ok(operator)
}

/// Shared observation available even while the owner is paused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
#[non_exhaustive]
pub enum UnresolvedObservation {
    /// No unresolved marker.
    Absent,
    /// A known marker awaiting recovery.
    Unresolved {
        /// Minimal recovery record.
        marker: Box<UnresolvedWrite>,
    },
    /// State is present but cannot safely be decoded by this deployment.
    Unreadable,
    /// A newer observation or marker schema, preserved without authorizing recovery.
    #[serde(untagged)]
    Other {
        /// Unclassified state token.
        state: String,
        /// Fields returned by the newer deployment.
        #[serde(flatten)]
        fields: serde_json::Map<String, serde_json::Value>,
    },
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for UnresolvedObservation {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "UnresolvedObservation".into()
    }

    fn schema_id() -> std::borrow::Cow<'static, str> {
        concat!(module_path!(), "::UnresolvedObservation").into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        // The closed UnresolvedWrite schema is the recovery *input* contract.
        // Observation must retain even an unreadable/newer marker as opaque JSON.
        schemars::json_schema!({
            "type": "object",
            "description": "Unresolved-write observation: absent, unreadable, or unresolved with a required marker. Unknown state strings and newer or unreadable marker values are preserved without authorizing recovery.",
            "required": ["state"],
            "properties": {"state": {"type": "string"}},
            "if": {"properties": {"state": {"const": "unresolved"}}},
            "then": {"required": ["marker"]}
        })
    }
}

impl<'de> Deserialize<'de> for UnresolvedObservation {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let mut fields = serde_json::Map::<String, serde_json::Value>::deserialize(de)?;
        let state = fields
            .remove("state")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or_else(|| serde::de::Error::custom("observation requires a string state"))?;
        match state.as_str() {
            "absent" => Ok(Self::Absent),
            "unreadable" => Ok(Self::Unreadable),
            "unresolved" => {
                let marker = fields.get("marker").cloned().ok_or_else(|| {
                    serde::de::Error::custom("unresolved observation requires marker")
                })?;
                match serde_json::from_value::<Box<UnresolvedWrite>>(marker.clone()) {
                    Ok(decoded)
                        if serde_json::to_value(&decoded).ok().as_ref() == Some(&marker) =>
                    {
                        Ok(Self::Unresolved { marker: decoded })
                    }
                    _ => Ok(Self::Other { state, fields }),
                }
            }
            _ => Ok(Self::Other { state, fields }),
        }
    }
}

/// Journaled evidence of operator settlement, returned before any new mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct RecoveryResponse {
    /// Marker settled by this recovery invocation.
    pub token: String,
    /// Host-supplied operator identity from the request, preserved for audit attribution.
    pub operator: String,
    /// Evidence that settled uncertainty. An attestation stays an attestation.
    pub evidence: serde_json::Value,
}
