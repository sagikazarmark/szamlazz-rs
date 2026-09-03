//! The entries of a [`Ledger`](super::Ledger): per-kind slots, corrective
//! entries, the request map and the transition inputs.

use std::fmt;

use jiff::Timestamp;
use jiff::civil::Date;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::contract::{DocumentKind, RequestId};
use crate::identity::Fingerprint;

/// The ledger entry for one document kind of an order.
///
/// Exactly one slot exists per kind. `generation` is the identity counter
/// embedded in the external id: while the slot is `pending` or `committed` it
/// names the current document; after a verified reversal, deletion,
/// consumption or `forget` it has already been bumped and names the
/// generation the next allocation will use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Slot {
    /// The generation. Serialised as `gen`.
    #[serde(rename = "gen")]
    pub generation: u32,
    /// The request id that owns the current generation.
    pub request_id: RequestId,
    /// The slot status.
    pub status: SlotStatus,
    /// The document number, once known.
    #[serde(default)]
    pub number: Option<String>,
    /// Gross total, once known.
    #[serde(default)]
    pub gross: Option<Decimal>,
    /// Net total, once known.
    #[serde(default)]
    pub net: Option<Decimal>,
    /// Whether the document carries `teszt = true`, once known.
    #[serde(default)]
    pub test: Option<bool>,
    /// How the document became ours.
    #[serde(default)]
    pub origin: Origin,
    /// Fingerprint of the request payload that owns the generation.
    pub fp: Fingerprint,
    /// The issue date the caller pinned, re-sent unchanged on every attempt.
    #[serde(default)]
    pub issue_date_requested: Option<Date>,
    /// Issuing attempts made for the current generation.
    #[serde(default)]
    pub attempts: u32,
    /// Journaled time of the last attempt.
    #[serde(default)]
    pub last_attempt_at: Option<Timestamp>,
}

/// The ledger entry for one corrective invoice of an order.
///
/// Correctives are not slots: an order may carry any number of them, each
/// identified by its per-order sequence number `cseq`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CorrectiveEntry {
    /// The per-order sequence number embedded in the external id.
    pub cseq: u32,
    /// The invoice being corrected.
    pub corrected_number: String,
    /// The request id that issued the corrective.
    pub request_id: RequestId,
    /// The entry status; only `pending`, `committed`, `rejected`, `reversed`
    /// and `reversal_unverified` occur.
    pub status: SlotStatus,
    /// The corrective invoice number, once known.
    #[serde(default)]
    pub number: Option<String>,
    /// Gross total, once known.
    #[serde(default)]
    pub gross: Option<Decimal>,
    /// Net total, once known.
    #[serde(default)]
    pub net: Option<Decimal>,
    /// Whether the document carries `teszt = true`, once known.
    #[serde(default)]
    pub test: Option<bool>,
    /// Fingerprint of the request payload.
    pub fp: Fingerprint,
    /// Issuing attempts made.
    #[serde(default)]
    pub attempts: u32,
    /// Journaled time of the last attempt.
    #[serde(default)]
    pub last_attempt_at: Option<Timestamp>,
}

/// The status of a slot or corrective entry.
///
/// Serialises externally tagged: `"pending"`, `{"rejected": {"code": …}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotStatus {
    /// An issuing request is in flight or unconfirmed: we may have issued
    /// something we have not yet confirmed.
    Pending,
    /// szamlazz.hu refuses the order number as a duplicate (71/152) and no
    /// document of ours could be found.
    Blocked {
        /// The number the hint reported, when any.
        #[serde(default)]
        existing_number: Option<String>,
    },
    /// The document is issued and, as far as the ledger knows, live.
    Committed,
    /// szamlazz.hu refused the document; nothing was created.
    Rejected {
        /// szamlazz.hu error code.
        code: String,
        /// szamlazz.hu error message.
        message: String,
    },
    /// The document was reversed; the generation has been bumped.
    Reversed {
        /// The storno invoice number, when known.
        #[serde(default)]
        by: Option<String>,
        /// Who reversed it.
        origin: ReversalOrigin,
    },
    /// A service-side storno exhausted its attempts without confirmation; the
    /// next `storno_invoice` retries.
    ReversalUnverified,
    /// (Proforma) removed from the query surface because an invoice or
    /// prepayment converted it. Terminal for the order.
    Consumed {
        /// The consuming document's number.
        by: String,
    },
    /// (Proforma) deleted; the generation has been bumped and a new proforma
    /// may be issued flag-free.
    Deleted,
    /// No document and no in-flight request; the generation is preserved so
    /// the next allocation reuses it. Produced when a pending intent is
    /// cleared because nothing of ours was created (a foreign document was
    /// detected) and after an operator `forget` (there with a bumped
    /// generation).
    Vacant,
}

impl SlotStatus {
    /// The snake-case token used in snapshots.
    #[must_use]
    pub const fn token(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Blocked { .. } => "blocked",
            Self::Committed => "committed",
            Self::Rejected { .. } => "rejected",
            Self::Reversed { .. } => "reversed",
            Self::ReversalUnverified => "reversal_unverified",
            Self::Consumed { .. } => "consumed",
            Self::Deleted => "deleted",
            Self::Vacant => "vacant",
        }
    }

    /// Whether a new intent may be allocated over this status.
    ///
    /// `Rejected`, `Deleted`, `Vacant` and every `Reversed` origin are open;
    /// whether a `Reversed{external | operator}` slot needs `reissue: true`
    /// is the service's policy, not a ledger precondition.
    #[must_use]
    pub const fn is_allocatable(&self) -> bool {
        matches!(
            self,
            Self::Rejected { .. } | Self::Reversed { .. } | Self::Deleted | Self::Vacant
        )
    }
}

impl fmt::Display for SlotStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.token())
    }
}

/// How a committed document became ours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// Issued by the service (directly or reconciled under our external id).
    #[default]
    Service,
    /// Found under the order number and adopted (a document referencing our
    /// proforma that no external id of ours resolves to).
    Adopted,
}

impl Origin {
    /// The snake-case token.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Service => "service",
            Self::Adopted => "adopted",
        }
    }
}

/// Who reversed a recorded document, as the ledger knows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReversalOrigin {
    /// Via `Szamlazz.Order.storno_invoice`; the slot is open flag-free.
    Service,
    /// Detected by verification (UI, support, another integration); the next
    /// create needs `reissue: true`.
    External,
    /// Asserted through the private `record_reversal` handler; the next create
    /// needs `reissue: true`.
    Operator,
}

impl ReversalOrigin {
    /// The snake-case token.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Service => "service",
            Self::External => "external",
            Self::Operator => "operator",
        }
    }
}

/// What a request id refers to.
///
/// Serialises externally tagged: `{"slot": {"kind": "invoice", "gen": 0}}`,
/// `{"corrective": {"cseq": 1}}`, `"abandoned"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestRef {
    /// A generation of a slot.
    Slot {
        /// The slot kind.
        kind: DocumentKind,
        /// The generation the request allocated. Serialised as `gen`.
        #[serde(rename = "gen")]
        generation: u32,
    },
    /// A corrective entry.
    Corrective {
        /// The corrective sequence number.
        cseq: u32,
    },
    /// The request's pending slot was taken over by another request id.
    Abandoned,
}

/// A slot or corrective entry addressed by a transition.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Target {
    /// The slot of a kind.
    Slot(DocumentKind),
    /// The corrective issued by a request id.
    Corrective(RequestId),
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Slot(kind) => write!(f, "{kind} slot"),
            Self::Corrective(id) => write!(f, "corrective {id}"),
        }
    }
}

impl From<DocumentKind> for Target {
    fn from(kind: DocumentKind) -> Self {
        Self::Slot(kind)
    }
}

/// The document a `commit` records.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CommittedDocument {
    /// The document number.
    pub number: String,
    /// Gross total, when known.
    pub gross: Option<Decimal>,
    /// Net total, when known.
    pub net: Option<Decimal>,
    /// `teszt`, when known.
    pub test: Option<bool>,
    /// How the document became ours.
    pub origin: Origin,
    /// Whether the document was found by a query (reconciled) rather than
    /// issued by the committing invocation.
    pub reconciled: bool,
}

impl CommittedDocument {
    /// A document issued by this invocation.
    pub fn issued(number: impl Into<String>) -> Self {
        Self {
            number: number.into(),
            gross: None,
            net: None,
            test: None,
            origin: Origin::Service,
            reconciled: false,
        }
    }

    /// A document found by a query and committed after validation.
    pub fn reconciled(number: impl Into<String>, origin: Origin) -> Self {
        Self {
            number: number.into(),
            gross: None,
            net: None,
            test: None,
            origin,
            reconciled: true,
        }
    }

    /// Sets the totals.
    #[must_use]
    pub const fn with_totals(mut self, gross: Option<Decimal>, net: Option<Decimal>) -> Self {
        self.gross = gross;
        self.net = net;
        self
    }

    /// Sets the `teszt` flag.
    #[must_use]
    pub const fn with_test(mut self, test: bool) -> Self {
        self.test = Some(test);
        self
    }
}

/// The facts a `mark_reversed` records.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Reversal {
    /// The reversed document's number, when the entry does not know it yet
    /// (a pending slot whose document was found already reversed).
    pub number: Option<String>,
    /// The storno invoice number, when known.
    pub by: Option<String>,
    /// Who reversed it.
    pub origin: ReversalOrigin,
    /// The credit entries registered before the reversal wiped them.
    pub payments_before: Vec<Decimal>,
}

impl Reversal {
    /// A reversal by `origin` with nothing else known.
    #[must_use]
    pub const fn new(origin: ReversalOrigin) -> Self {
        Self {
            number: None,
            by: None,
            origin,
            payments_before: Vec::new(),
        }
    }

    /// Sets the storno invoice number.
    #[must_use]
    pub fn with_by(mut self, by: impl Into<String>) -> Self {
        self.by = Some(by.into());
        self
    }

    /// Sets the reversed document's number.
    #[must_use]
    pub fn with_number(mut self, number: impl Into<String>) -> Self {
        self.number = Some(number.into());
        self
    }

    /// Sets the payments snapshot.
    #[must_use]
    pub fn with_payments_before(mut self, payments: Vec<Decimal>) -> Self {
        self.payments_before = payments;
        self
    }
}

/// One ledger history event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct HistoryEvent {
    /// The document kind the event is about.
    pub kind: crate::contract::IssuedKind,
    /// The generation (or corrective sequence). Serialised as `gen`.
    #[serde(rename = "gen")]
    pub generation: u32,
    /// The request id involved, when any.
    #[serde(default)]
    pub request_id: Option<RequestId>,
    /// The document number involved, when known.
    #[serde(default)]
    pub number: Option<String>,
    /// What happened.
    pub event: HistoryKind,
    /// The number of the document that caused the event (a storno or the
    /// consuming invoice), when any.
    #[serde(default)]
    pub by: Option<String>,
    /// Reversal origin on a `reversed` event.
    #[serde(default)]
    pub origin: Option<ReversalOrigin>,
    /// Credit entries registered before a reversal wiped them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub payments_before: Vec<Decimal>,
}

/// The kind of a [`HistoryEvent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryKind {
    /// szamlazz.hu issued the document in a service invocation.
    Issued,
    /// A document issued by an unconfirmed attempt was found under our
    /// external id and committed.
    Reconciled,
    /// A document found under the order number was adopted.
    Adopted,
    /// The document was reversed.
    Reversed,
    /// The proforma was deleted.
    Deleted,
    /// The proforma was consumed by an invoice or prepayment.
    Consumed,
    /// A pending request was taken over by another request id.
    Abandoned,
    /// An operator dropped a slot whose document szamlazz.hu no longer knows.
    Forgotten,
    /// An operator asserted the document live.
    RecordedByOperator,
}

impl HistoryKind {
    /// The snake-case token.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Issued => "issued",
            Self::Reconciled => "reconciled",
            Self::Adopted => "adopted",
            Self::Reversed => "reversed",
            Self::Deleted => "deleted",
            Self::Consumed => "consumed",
            Self::Abandoned => "abandoned",
            Self::Forgotten => "forgotten",
            Self::RecordedByOperator => "recorded_by_operator",
        }
    }
}

/// A transition whose precondition does not hold. The ledger is unchanged.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LedgerError {
    /// The slot holds a live or unresolved entry.
    #[error("the {kind} slot is {status} and cannot be allocated")]
    SlotBusy {
        /// The slot kind.
        kind: DocumentKind,
        /// The current status token.
        status: String,
    },
    /// The request id already refers to another entry.
    #[error("request id {0} is already known")]
    RequestIdKnown(RequestId),
    /// No such slot or corrective entry exists.
    #[error("no {0} exists")]
    MissingTarget(Target),
    /// The entry's status does not permit the transition.
    #[error("the {target} is {from}; cannot {action}")]
    InvalidTransition {
        /// The addressed entry.
        target: Target,
        /// Its current status token.
        from: String,
        /// The attempted transition.
        action: &'static str,
    },
    /// No recorded document carries the number.
    #[error("no recorded document has number {0}")]
    UnknownNumber(String),
    /// A query reported a supplier id other than the recorded one.
    #[error("supplier id {seen} differs from the recorded {recorded}")]
    SupplierMismatch {
        /// The supplier id the ledger holds.
        recorded: u64,
        /// The supplier id just seen.
        seen: u64,
    },
}
