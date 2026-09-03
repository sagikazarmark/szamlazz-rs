//! Handler outputs of the `Szamlazz.Order` Virtual Object and the
//! `Szamlazz.Agent` service.
//!
//! Domain outcomes are data, returned with HTTP 200 through the ingress. A
//! `TerminalError` (see [`TerminalCode`](super::TerminalCode)) is reserved
//! for faults and always means "outcome unknown — call again with the same
//! request id".

use jiff::civil::Date;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{IssuedKind, RequestId};

/// The domain outcome of a create or correct request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Outcome {
    /// szamlazz.hu issued the document in this invocation.
    Issued,
    /// The document was already issued for this request id and is live.
    AlreadyIssued,
    /// A document issued by an earlier, unconfirmed attempt was found under
    /// our external id (or adopted via a proforma reference) and committed.
    Reconciled,
    /// The recorded document was reversed; nothing new was issued. Pass
    /// `reissue: true` with a new request id to issue the next generation.
    Reversed,
    /// szamlazz.hu refused the document; see `code` and `message`.
    Rejected,
    /// The request contradicts the ledger or the live account; see
    /// `conflict_reason`.
    Conflict,
}

/// Why a request was answered with [`Outcome::Conflict`] or
/// [`StornoOutcome::Conflict`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConflictReason {
    /// A prepayment (or, for `create_prepayment`, an invoice) exists for the
    /// order: the two chains are exclusive.
    PrepaidChain,
    /// A known request id arrived with a different document payload.
    PayloadMismatch,
    /// A known request id arrived for a different document kind.
    RequestIdReused,
    /// The slot is `pending` (storno only): a create has not been confirmed.
    Pending,
    /// `reissue: true` while the recorded document is live.
    Live,
    /// A live invoice-kind document not owned by the ledger exists under the
    /// order number; see `existing_number`.
    Foreign,
    /// szamlazz.hu refuses the order number as a duplicate (code 71) and no
    /// document of ours can be found; the slot is `blocked`.
    DuplicateOrderNumber,
    /// A document found under our external id belongs to another order,
    /// kind, account mode or supplier.
    ExternalIdCollision,
    /// szamlazz.hu no longer knows the recorded document (code 7). Never
    /// reissued automatically; an operator `forget` clears the slot.
    RecordedDocumentMissing,
    /// `proforma: none` while a live proforma exists under the order number.
    ProformaLive,
    /// The referenced proforma cannot be found.
    ProformaMissing,
    /// The proforma was consumed by an invoice or prepayment.
    ProformaConsumed,
    /// `create_final` while the recorded prepayment is reversed.
    PrepaymentReversed,
    /// `create_final` while a final invoice is already committed.
    FinalExists,
    /// `correct_invoice` on a base the ledger records as reversed.
    BaseReversed,
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
    /// The create carried a proforma reference that szamlazz.hu silently
    /// dropped (deleted or already consumed proforma); the proforma slot is
    /// unchanged.
    ProformaLinkDropped,
}

/// Output of every create and correct handler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct CreateResponse {
    /// The domain outcome.
    pub outcome: Outcome,
    /// Present when `outcome` is [`Outcome::Conflict`].
    #[serde(default)]
    pub conflict_reason: Option<ConflictReason>,
    /// The request id the response is about.
    pub request_id: RequestId,
    /// The document kind.
    pub kind: IssuedKind,
    /// The generation of the slot (or the corrective sequence) the request
    /// targeted. Serialised as `gen`.
    #[serde(rename = "gen")]
    pub generation: u32,
    /// The external id (`szamlaKulsoAzon`) the document carries.
    pub external_id: String,
    /// The issued document's number, when one exists for this request.
    #[serde(default)]
    pub invoice_number: Option<String>,
    /// The storno invoice number, when `outcome` is [`Outcome::Reversed`]
    /// and it is known.
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
    /// `payload_mismatch`, the foreign document on `foreign`, …).
    #[serde(default)]
    pub existing_number: Option<String>,
    /// szamlazz.hu error code on [`Outcome::Rejected`].
    #[serde(default)]
    pub code: Option<String>,
    /// szamlazz.hu error message on [`Outcome::Rejected`].
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
    pub fn new(
        outcome: Outcome,
        request_id: RequestId,
        kind: IssuedKind,
        generation: u32,
        external_id: impl Into<String>,
    ) -> Self {
        Self {
            outcome,
            conflict_reason: None,
            request_id,
            kind,
            generation,
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
        request_id: RequestId,
        kind: IssuedKind,
        generation: u32,
        external_id: impl Into<String>,
    ) -> Self {
        let mut response = Self::new(Outcome::Conflict, request_id, kind, generation, external_id);
        response.conflict_reason = Some(reason);
        response
    }

    /// Sets the conflict reason.
    #[must_use]
    pub fn with_conflict_reason(mut self, reason: ConflictReason) -> Self {
        self.conflict_reason = Some(reason);
        self
    }

    /// Sets the issued document's number.
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
    /// The request contradicts the ledger; see `conflict_reason`.
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
    /// Present when `outcome` is [`StornoOutcome::Conflict`].
    #[serde(default)]
    pub conflict_reason: Option<ConflictReason>,
    /// The invoice the request was about.
    pub invoice_number: String,
    /// The storno invoice number, when known.
    #[serde(default)]
    pub storno_number: Option<String>,
    /// The `Order` key managing the document on
    /// [`StornoOutcome::ManagedByOrder`].
    #[serde(default)]
    pub order_key: Option<String>,
    /// szamlazz.hu error code on [`StornoOutcome::Rejected`].
    #[serde(default)]
    pub code: Option<String>,
    /// szamlazz.hu error message on [`StornoOutcome::Rejected`].
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

/// Output of `Szamlazz.Order.delete_proforma`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct DeleteProformaResponse {
    /// Whether the proforma is deleted (now or already).
    pub deleted: bool,
    /// Why it is not deleted (`pending`, `proforma_paid`, a szamlazz.hu error
    /// code, …).
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

    /// The proforma is not deleted for `reason`.
    pub fn not_deleted(reason: impl Into<String>) -> Self {
        Self {
            deleted: false,
            reason: Some(reason.into()),
        }
    }
}

/// Output of `Szamlazz.Agent.set_payments`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct SetPaymentsResponse {
    /// The invoice the entries were registered on.
    pub invoice_number: String,
    /// Outstanding amount after the update (`kintlévőség`).
    #[serde(default)]
    pub outstanding: Option<Decimal>,
    /// Gross total of the invoice.
    #[serde(default)]
    pub gross_total: Option<Decimal>,
}

impl SetPaymentsResponse {
    /// A response for `invoice_number` without totals.
    pub fn new(invoice_number: impl Into<String>) -> Self {
        Self {
            invoice_number: invoice_number.into(),
            outstanding: None,
            gross_total: None,
        }
    }
}

/// One registered credit entry as szamlazz.hu reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct PaymentRecord {
    /// Payment date.
    #[serde(default)]
    pub date: Option<Date>,
    /// Title / payment method text (`jogcím`).
    #[serde(default)]
    pub title: Option<String>,
    /// Amount in the invoice currency.
    pub amount: Decimal,
    /// Free-text comment.
    #[serde(default)]
    pub comment: Option<String>,
    /// Bank account the payment arrived on.
    #[serde(default)]
    pub bank_account: Option<String>,
}

impl PaymentRecord {
    /// A record of `amount` with every optional field absent.
    #[must_use]
    pub const fn new(amount: Decimal) -> Self {
        Self {
            date: None,
            title: None,
            amount,
            comment: None,
            bank_account: None,
        }
    }
}

/// Output of `Szamlazz.Agent.query`: a projection of the queried document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct QueryResponse {
    /// Document number (`számlaszám`).
    pub invoice_number: String,
    /// Document type code (`tipus`): `SZ` invoice, `D` proforma, `ES`
    /// prepayment, `VS` final, `SS` storno, `HS` corrective, ….
    pub document_type: String,
    /// Whether the document has been reversed; `None` when szamlazz.hu did
    /// not report it.
    #[serde(default)]
    pub reversed: Option<bool>,
    /// The invoice this document references (`hivszamlaszam`): the reversed
    /// invoice of a storno, the corrected one of a corrective.
    #[serde(default)]
    pub referenced_invoice_number: Option<String>,
    /// The proforma this document converted (`hivdijbekszam`).
    #[serde(default)]
    pub referenced_proforma_number: Option<String>,
    /// Order number (`rendelésszám`).
    #[serde(default)]
    pub order_number: Option<String>,
    /// Issue date (`kelt`).
    #[serde(default)]
    pub issue_date: Option<Date>,
    /// Fulfillment date (`teljesítés`).
    #[serde(default)]
    pub fulfillment_date: Option<Date>,
    /// Payment due date (`fizetési határidő`).
    #[serde(default)]
    pub due_date: Option<Date>,
    /// Currency code (`pénznem`).
    #[serde(default)]
    pub currency: Option<String>,
    /// Net total.
    #[serde(default)]
    pub net_total: Option<Decimal>,
    /// VAT total.
    #[serde(default)]
    pub vat_total: Option<Decimal>,
    /// Gross total.
    #[serde(default)]
    pub gross_total: Option<Decimal>,
    /// Registered credit entries.
    #[serde(default)]
    pub payments: Vec<PaymentRecord>,
    /// Outstanding amount: gross total minus the sum of payments.
    #[serde(default)]
    pub outstanding: Option<Decimal>,
    /// The issuing account's supplier id (`szállító/id`).
    #[serde(default)]
    pub supplier_id: Option<u64>,
    /// Issued from a test account (`teszt`).
    #[serde(default)]
    pub test: bool,
}

impl QueryResponse {
    /// A response with the number and type set and everything else absent.
    pub fn new(invoice_number: impl Into<String>, document_type: impl Into<String>) -> Self {
        Self {
            invoice_number: invoice_number.into(),
            document_type: document_type.into(),
            reversed: None,
            referenced_invoice_number: None,
            referenced_proforma_number: None,
            order_number: None,
            issue_date: None,
            fulfillment_date: None,
            due_date: None,
            currency: None,
            net_total: None,
            vat_total: None,
            gross_total: None,
            payments: Vec::new(),
            outstanding: None,
            supplier_id: None,
            test: false,
        }
    }
}

/// Output of `Szamlazz.Order.get`, `record_reversal` and `forget`: the serialisable
/// projection of the order's ledger. Carries numbers, ids and totals — never
/// buyer data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct OrderSnapshot {
    /// The issuing account's supplier id, once learned from a query.
    #[serde(default)]
    pub supplier_id: Option<u64>,
    /// The four slots.
    #[serde(default)]
    pub slots: SlotsSnapshot,
    /// Correctives issued for the order, in `cseq` order.
    #[serde(default)]
    pub correctives: Vec<CorrectiveSnapshot>,
    /// The last foreign document seen under the order number.
    #[serde(default)]
    pub foreign_hint: Option<ForeignHint>,
    /// Bounded event history, oldest first.
    #[serde(default)]
    pub history: Vec<HistorySnapshot>,
    /// Whether the snapshot was verified against szamlazz.hu.
    pub freshness: Freshness,
    /// What the verification found for each recorded document; empty unless
    /// `freshness` is [`Freshness::Live`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verification: Vec<DocumentVerification>,
}

impl OrderSnapshot {
    /// An empty snapshot with the given freshness.
    #[must_use]
    pub fn new(freshness: Freshness) -> Self {
        Self {
            supplier_id: None,
            slots: SlotsSnapshot::default(),
            correctives: Vec::new(),
            foreign_hint: None,
            history: Vec::new(),
            freshness,
            verification: Vec::new(),
        }
    }
}

/// What a live verification found for one recorded document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct DocumentVerification {
    /// The document kind.
    pub kind: IssuedKind,
    /// The generation (or corrective sequence) verified. Serialised as `gen`.
    #[serde(rename = "gen")]
    pub generation: u32,
    /// The recorded document number.
    pub number: String,
    /// What szamlazz.hu reported.
    pub result: VerificationResult,
}

impl DocumentVerification {
    /// A verification record.
    pub fn new(
        kind: IssuedKind,
        generation: u32,
        number: impl Into<String>,
        result: VerificationResult,
    ) -> Self {
        Self {
            kind,
            generation,
            number: number.into(),
            result,
        }
    }
}

/// The result of verifying one recorded document against szamlazz.hu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum VerificationResult {
    /// The document exists and is not reversed.
    Live,
    /// The document exists and szamlazz.hu reports it reversed.
    Reversed,
    /// szamlazz.hu no longer knows the document (code 7).
    Missing,
    /// The check itself failed; nothing may be concluded.
    Unavailable,
}

/// Whether an [`OrderSnapshot`] reflects the ledger as recorded or as just
/// verified against szamlazz.hu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    /// The ledger as recorded; no szamlazz.hu call was made.
    Snapshot,
    /// Every committed document was verified against szamlazz.hu first.
    Live,
}

/// The four per-kind slots of an order.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(default)]
#[non_exhaustive]
pub struct SlotsSnapshot {
    /// The proforma slot.
    pub proforma: Option<SlotSnapshot>,
    /// The invoice slot.
    pub invoice: Option<SlotSnapshot>,
    /// The prepayment invoice slot.
    pub prepayment: Option<SlotSnapshot>,
    /// The final invoice slot.
    #[serde(rename = "final")]
    pub r#final: Option<SlotSnapshot>,
}

impl SlotsSnapshot {
    /// The slot of `kind`.
    #[must_use]
    pub const fn get(&self, kind: super::DocumentKind) -> Option<&SlotSnapshot> {
        match kind {
            super::DocumentKind::Proforma => self.proforma.as_ref(),
            super::DocumentKind::Invoice => self.invoice.as_ref(),
            super::DocumentKind::Prepayment => self.prepayment.as_ref(),
            super::DocumentKind::Final => self.r#final.as_ref(),
        }
    }
}

/// The projection of one ledger slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct SlotSnapshot {
    /// The current generation. Serialised as `gen`.
    #[serde(rename = "gen")]
    pub generation: u32,
    /// The request id that owns the current generation.
    pub request_id: RequestId,
    /// Slot status token: `pending`, `committed`, `rejected`, `blocked`,
    /// `reversed`, `reversal_unverified`, `consumed`, `deleted` or `vacant`.
    pub status: String,
    /// The document number, once known.
    #[serde(default)]
    pub number: Option<String>,
    /// The gross total, once known.
    #[serde(default)]
    pub gross: Option<Decimal>,
    /// How the document became ours (`service`, `adopted`) or, for a reversed
    /// slot, who reversed it (`service`, `external`, `operator`).
    #[serde(default)]
    pub origin: Option<String>,
    /// Issuing attempts made for the current generation.
    pub attempts: u32,
}

impl SlotSnapshot {
    /// A slot projection with the required fields and no number, gross or
    /// origin.
    pub fn new(
        generation: u32,
        request_id: RequestId,
        status: impl Into<String>,
        attempts: u32,
    ) -> Self {
        Self {
            generation,
            request_id,
            status: status.into(),
            number: None,
            gross: None,
            origin: None,
            attempts,
        }
    }
}

/// The projection of one corrective entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct CorrectiveSnapshot {
    /// The request id that issued the corrective.
    pub request_id: RequestId,
    /// The per-order corrective sequence number embedded in the external id.
    pub cseq: u32,
    /// The corrective invoice number, once known.
    #[serde(default)]
    pub number: Option<String>,
    /// The invoice being corrected.
    pub corrected_number: String,
    /// Status token, as for a slot.
    pub status: String,
}

impl CorrectiveSnapshot {
    /// A corrective projection without a number.
    pub fn new(
        request_id: RequestId,
        cseq: u32,
        corrected_number: impl Into<String>,
        status: impl Into<String>,
    ) -> Self {
        Self {
            request_id,
            cseq,
            number: None,
            corrected_number: corrected_number.into(),
            status: status.into(),
        }
    }
}

/// A live invoice-kind document found under the order number that the ledger
/// does not own. Recorded as a hint only, never adopted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct ForeignHint {
    /// The foreign document's number.
    pub number: String,
    /// Its document type code (`tipus`).
    pub document_type: String,
}

impl ForeignHint {
    /// A hint for `number` of `document_type`.
    pub fn new(number: impl Into<String>, document_type: impl Into<String>) -> Self {
        Self {
            number: number.into(),
            document_type: document_type.into(),
        }
    }
}

/// One ledger history event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct HistorySnapshot {
    /// The document kind the event is about.
    pub kind: IssuedKind,
    /// The generation (or corrective sequence) the event is about.
    /// Serialised as `gen`.
    #[serde(rename = "gen")]
    pub generation: u32,
    /// The request id involved, when any.
    #[serde(default)]
    pub request_id: Option<RequestId>,
    /// The document number involved, when known.
    #[serde(default)]
    pub number: Option<String>,
    /// Event token: `issued`, `reconciled`, `reversed`, `abandoned`,
    /// `deleted`, `consumed`, `forgotten`, ….
    pub event: String,
    /// The number of the document that caused the event (a storno or the
    /// consuming invoice), when any.
    #[serde(default)]
    pub by: Option<String>,
    /// Reversal origin (`service`, `external`, `operator`) on a `reversed`
    /// event.
    #[serde(default)]
    pub origin: Option<String>,
}

impl HistorySnapshot {
    /// An event with the required fields and no request id, number, `by` or
    /// origin.
    pub fn new(kind: IssuedKind, generation: u32, event: impl Into<String>) -> Self {
        Self {
            kind,
            generation,
            request_id: None,
            number: None,
            event: event.into(),
            by: None,
            origin: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use rust_decimal::dec;
    use serde_json::json;

    use super::*;

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
    fn create_response_round_trips() {
        let response = CreateResponse::new(
            Outcome::Issued,
            request_id(),
            IssuedKind::Invoice,
            0,
            "acct:ORD-1:invoice:0",
        )
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
        assert_eq!(json["gen"], 0);
        assert_eq!(json["warnings"], json!(["notification_delivery_failed"]));
    }

    #[test]
    fn create_response_conflict_and_rejection() {
        let conflict = CreateResponse::conflict(
            ConflictReason::PayloadMismatch,
            request_id(),
            IssuedKind::Invoice,
            1,
            "acct:ORD-1:invoice:1",
        )
        .with_existing_number("SZ-1");
        let json = round_trip(&conflict);
        assert_eq!(json["outcome"], "conflict");
        assert_eq!(json["conflict_reason"], "payload_mismatch");
        assert_eq!(json["existing_number"], "SZ-1");

        let rejected = CreateResponse::new(
            Outcome::Rejected,
            request_id(),
            IssuedKind::Corrective,
            2,
            "acct:ORD-1:corrective:2",
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
            "request_id": "r-1",
            "kind": "final",
            "gen": 3,
            "external_id": "acct:ORD-1:final:3",
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
            (ConflictReason::PayloadMismatch, "payload_mismatch"),
            (ConflictReason::RequestIdReused, "request_id_reused"),
            (ConflictReason::Pending, "pending"),
            (ConflictReason::Live, "live"),
            (ConflictReason::Foreign, "foreign"),
            (
                ConflictReason::DuplicateOrderNumber,
                "duplicate_order_number",
            ),
            (ConflictReason::ExternalIdCollision, "external_id_collision"),
            (
                ConflictReason::RecordedDocumentMissing,
                "recorded_document_missing",
            ),
            (ConflictReason::ProformaLive, "proforma_live"),
            (ConflictReason::ProformaMissing, "proforma_missing"),
            (ConflictReason::ProformaConsumed, "proforma_consumed"),
            (ConflictReason::PrepaymentReversed, "prepayment_reversed"),
            (ConflictReason::FinalExists, "final_exists"),
            (ConflictReason::BaseReversed, "base_reversed"),
        ];
        for (reason, token) in reasons {
            assert_eq!(
                serde_json::to_value(reason).expect("serialize"),
                json!(token)
            );
        }
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
            .with_conflict_reason(ConflictReason::Pending);
        round_trip(&conflict);
        let rejected = StornoResponse::new(StornoOutcome::Rejected, "SZ-4")
            .with_code("221")
            .with_message("has corrective");
        round_trip(&rejected);
    }

    #[test]
    fn small_responses_round_trip() {
        let json = round_trip(&DeleteProformaResponse::deleted());
        assert_eq!(json, json!({"deleted": true, "reason": null}));
        round_trip(&DeleteProformaResponse::not_deleted("pending"));

        let mut payments = SetPaymentsResponse::new("SZ-1");
        payments.outstanding = Some(dec!(0));
        payments.gross_total = Some(dec!(25400));
        round_trip(&payments);
    }

    #[test]
    fn query_response_round_trips() {
        let mut response = QueryResponse::new("SZ-1", "SZ");
        response.reversed = Some(false);
        response.referenced_proforma_number = Some("D-1".to_owned());
        response.order_number = Some("ORD-1".to_owned());
        response.issue_date = Some(date(2026, 7, 4));
        response.fulfillment_date = Some(date(2026, 7, 4));
        response.due_date = Some(date(2026, 7, 12));
        response.currency = Some("HUF".to_owned());
        response.net_total = Some(dec!(20000));
        response.vat_total = Some(dec!(5400));
        response.gross_total = Some(dec!(25400));
        let mut payment = PaymentRecord::new(dec!(10000));
        payment.date = Some(date(2026, 7, 10));
        payment.title = Some("átutalás".to_owned());
        response.payments = vec![payment];
        response.outstanding = Some(dec!(15400));
        response.supplier_id = Some(972_720);
        response.test = true;
        let json = round_trip(&response);
        assert_eq!(json["document_type"], "SZ");
        assert_eq!(json["supplier_id"], 972_720);
        assert_eq!(json["payments"][0]["amount"], "10000");

        let minimal: QueryResponse =
            serde_json::from_value(json!({"invoice_number": "D-1", "document_type": "D"}))
                .expect("deserialize");
        assert_eq!(minimal, QueryResponse::new("D-1", "D"));
    }

    #[test]
    fn order_snapshot_round_trips() {
        let mut snapshot = OrderSnapshot::new(Freshness::Live);
        snapshot.supplier_id = Some(972_720);
        let mut invoice = SlotSnapshot::new(1, request_id(), "committed", 1);
        invoice.number = Some("SZ-2".to_owned());
        invoice.gross = Some(dec!(25400));
        invoice.origin = Some("service".to_owned());
        snapshot.slots.invoice = Some(invoice);
        snapshot.slots.r#final = Some(SlotSnapshot::new(0, request_id(), "pending", 2));
        let mut corrective = CorrectiveSnapshot::new(request_id(), 1, "SZ-2", "committed");
        corrective.number = Some("HS-1".to_owned());
        snapshot.correctives = vec![corrective];
        snapshot.foreign_hint = Some(ForeignHint::new("SZ-9", "SZ"));
        let mut event = HistorySnapshot::new(IssuedKind::Invoice, 0, "reversed");
        event.request_id = Some(request_id());
        event.number = Some("SZ-1".to_owned());
        event.by = Some("SS-1".to_owned());
        event.origin = Some("external".to_owned());
        snapshot.history = vec![event];

        let json = round_trip(&snapshot);
        assert_eq!(json["freshness"], "live");
        assert_eq!(json["slots"]["final"]["status"], "pending");
        assert_eq!(json["slots"]["proforma"], serde_json::Value::Null);
        assert_eq!(json["history"][0]["kind"], "invoice");
        assert_eq!(
            snapshot.slots.get(super::super::DocumentKind::Final),
            snapshot.slots.r#final.as_ref()
        );

        let empty: OrderSnapshot =
            serde_json::from_value(json!({"freshness": "snapshot"})).expect("deserialize");
        assert_eq!(empty, OrderSnapshot::new(Freshness::Snapshot));
    }
}
