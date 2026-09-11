//! Operator recovery and the versioned, fail-closed unresolved-write state.

use serde::{Deserialize, Deserializer, Serialize};

use crate::identity::{IssuedKind, Namespace, OrderKey};

/// A vendor-reported document number submitted as recovery evidence.
///
/// Preserves the exact spelling, including whitespace and `:`, with no mutation-input
/// length bound. The number must be nonblank and representable in XML 1.0 so it can
/// be queried. It never becomes an external-id segment or permission for a new write.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EvidenceNumber(String);

impl EvidenceNumber {
    /// The vendor's exact number; nothing is trimmed or normalized.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for EvidenceNumber {
    type Error = InvalidEvidenceNumber;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.trim().is_empty() {
            return Err(InvalidEvidenceNumber::Blank);
        }
        szamlazz_agent::wire::validate_xml_text(&value)?;
        Ok(Self(value))
    }
}

impl std::str::FromStr for EvidenceNumber {
    type Err = InvalidEvidenceNumber;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.to_owned().try_into()
    }
}

impl From<EvidenceNumber> for String {
    fn from(number: EvidenceNumber) -> Self {
        number.0
    }
}

impl AsRef<str> for EvidenceNumber {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for EvidenceNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A recovery number that cannot identify a queryable document.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidEvidenceNumber {
    /// Empty or entirely Unicode whitespace.
    #[error("recovery evidence number must not be blank")]
    Blank,
    /// A character cannot be represented in XML 1.0.
    #[error(transparent)]
    Xml(#[from] szamlazz_agent::RequestError),
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for EvidenceNumber {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "EvidenceNumber".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "Exact vendor document number: nonblank XML 1.0 text, no mutation-input length bound or normalization.",
            "minLength": 1,
            "pattern": r"[^\u0009-\u000D\u0020\u0085\u00A0\u1680\u2000-\u200A\u2028\u2029\u202F\u205F\u3000]",
            "not": {"pattern": r"[\u0000-\u0008\u000B\u000C\u000E-\u001F\uFFFE\uFFFF]"}
        })
    }
}

/// Recovery intent without buyer data, line items, XML or credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Deletion of the pinned proforma.
    Delete {
        /// Pinned proforma number.
        number: String,
    },
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

/// The sole Order state: a possibly effective write awaiting conclusive evidence.
/// Its schema crosses deployments and is deliberately closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct UnresolvedWrite {
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
    /// External identity of the write.
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

/// The operation-specific result independently established by an operator.
/// The audit record must tie it to the exact marker, including corrective base
/// or reissue intent. Merely observing an absent document is insufficient.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// Evidence an authorized operator submits; no generic clearance exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// Exclusive operator recovery; the complete marker must match exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct RecoveryRequest {
    /// Exact marker observed under the same scope and Order key.
    pub marker: UnresolvedWrite,
    /// Evidence permitting settlement, never permission to send.
    pub evidence: RecoveryEvidence,
}

/// Shared observation available even while the owner is paused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
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
                match serde_json::from_value(marker) {
                    Ok(marker) => Ok(Self::Unresolved { marker }),
                    Err(_) => Ok(Self::Other { state, fields }),
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
    /// Operator identity supplied by the host authorizer.
    pub operator: String,
    /// Evidence that settled uncertainty. An attestation stays an attestation.
    pub evidence: serde_json::Value,
}
