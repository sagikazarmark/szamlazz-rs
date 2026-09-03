//! The ledger: the `Order` Virtual Object's state (design §4, schema `v = 1`).
//!
//! Per-kind [`Slot`]s, [`CorrectiveEntry`]s, the request-id map, a foreign
//! hint and a bounded history. It holds numbers, ids, totals, a fingerprint
//! and journaled timestamps — never buyer data. It is the source of truth for
//! what the service issued; szamlazz.hu is consulted to verify it, not to
//! rebuild it.
//!
//! Every transition is pure (no I/O, no clock, no panics) and returns
//! [`LedgerError`] when its precondition fails, leaving the ledger unchanged.
//! The generation of a slot bumps only in [`Ledger::mark_reversed`],
//! [`Ledger::mark_deleted`], [`Ledger::mark_consumed`], [`Ledger::forget`] and
//! [`Ledger::record_operator_reversal`] — never on rejections, blocks,
//! transport failures or foreign detections.
//!
//! Every field deserialises with a default, so `{}` and `{"v": 1}` are valid
//! empty ledgers and fields added later read back as absent.

use std::collections::BTreeMap;

use jiff::Timestamp;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use crate::contract::{
    CorrectiveSnapshot, DocumentKind, ForeignHint, Freshness, HistorySnapshot, IssuedKind,
    OrderSnapshot, RequestId, SlotSnapshot, SlotsSnapshot,
};
use crate::identity::Fingerprint;

pub mod entry;

pub use entry::{
    CommittedDocument, CorrectiveEntry, HistoryEvent, HistoryKind, LedgerError, Origin, RequestRef,
    Reversal, ReversalOrigin, Slot, SlotStatus, Target,
};

/// The `Order` Virtual Object state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ledger {
    v: u32,
    supplier_id: Option<u64>,
    slots: Slots,
    correctives: BTreeMap<RequestId, CorrectiveEntry>,
    next_cseq: u32,
    requests: BTreeMap<RequestId, RequestRef>,
    foreign_hint: Option<ForeignHint>,
    history: Vec<HistoryEvent>,
}

/// The four per-kind slots.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct Slots {
    /// The proforma slot.
    pub proforma: Option<Slot>,
    /// The invoice slot.
    pub invoice: Option<Slot>,
    /// The prepayment invoice slot.
    pub prepayment: Option<Slot>,
    /// The final invoice slot.
    #[serde(rename = "final")]
    pub r#final: Option<Slot>,
}

impl Slots {
    /// The slot of `kind`.
    #[must_use]
    pub const fn get(&self, kind: DocumentKind) -> Option<&Slot> {
        match kind {
            DocumentKind::Proforma => self.proforma.as_ref(),
            DocumentKind::Invoice => self.invoice.as_ref(),
            DocumentKind::Prepayment => self.prepayment.as_ref(),
            DocumentKind::Final => self.r#final.as_ref(),
        }
    }

    const fn cell(&mut self, kind: DocumentKind) -> &mut Option<Slot> {
        match kind {
            DocumentKind::Proforma => &mut self.proforma,
            DocumentKind::Invoice => &mut self.invoice,
            DocumentKind::Prepayment => &mut self.prepayment,
            DocumentKind::Final => &mut self.r#final,
        }
    }

    fn get_mut(&mut self, kind: DocumentKind) -> Option<&mut Slot> {
        self.cell(kind).as_mut()
    }
}

impl Default for Ledger {
    fn default() -> Self {
        Self::new()
    }
}

impl Ledger {
    /// The schema version written as `v`.
    pub const SCHEMA_VERSION: u32 = 1;

    /// The maximum number of history events kept; the oldest are dropped.
    pub const HISTORY_CAP: usize = 64;

    /// An empty ledger.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            v: Self::SCHEMA_VERSION,
            supplier_id: None,
            slots: Slots {
                proforma: None,
                invoice: None,
                prepayment: None,
                r#final: None,
            },
            correctives: BTreeMap::new(),
            next_cseq: 1,
            requests: BTreeMap::new(),
            foreign_hint: None,
            history: Vec::new(),
        }
    }

    // ----- reads -----------------------------------------------------------

    /// The schema version.
    #[must_use]
    pub const fn version(&self) -> u32 {
        self.v
    }

    /// The supplier id learned from the first query, if any.
    #[must_use]
    pub const fn supplier_id(&self) -> Option<u64> {
        self.supplier_id
    }

    /// The slot of `kind`, including a [`SlotStatus::Vacant`] one.
    #[must_use]
    pub const fn slot(&self, kind: DocumentKind) -> Option<&Slot> {
        self.slots.get(kind)
    }

    /// All four slots.
    #[must_use]
    pub const fn slots(&self) -> &Slots {
        &self.slots
    }

    /// The corrective issued by `request_id`.
    #[must_use]
    pub fn corrective(&self, request_id: &RequestId) -> Option<&CorrectiveEntry> {
        self.correctives.get(request_id)
    }

    /// Every corrective entry, in `cseq` order.
    #[must_use]
    pub fn correctives(&self) -> Vec<&CorrectiveEntry> {
        let mut entries: Vec<_> = self.correctives.values().collect();
        entries.sort_by_key(|entry| entry.cseq);
        entries
    }

    /// The next corrective sequence number to be allocated.
    #[must_use]
    pub const fn next_cseq(&self) -> u32 {
        self.next_cseq
    }

    /// What `request_id` refers to, if it is known.
    #[must_use]
    pub fn lookup_request(&self, request_id: &RequestId) -> Option<&RequestRef> {
        self.requests.get(request_id)
    }

    /// The last foreign document seen under the order number.
    #[must_use]
    pub const fn foreign_hint(&self) -> Option<&ForeignHint> {
        self.foreign_hint.as_ref()
    }

    /// The bounded history, oldest first.
    #[must_use]
    pub fn history(&self) -> &[HistoryEvent] {
        &self.history
    }

    /// Every document number the ledger knows as ours: slot and corrective
    /// numbers in any status, the storno numbers that reversed them, the
    /// documents that consumed a proforma, and every number in the history.
    /// The foreign hint is not ours.
    #[must_use]
    pub fn our_numbers(&self) -> Vec<String> {
        let mut numbers = Vec::new();
        let mut push = |number: Option<&str>| {
            if let Some(number) = number
                && !numbers.iter().any(|known| known == number)
            {
                numbers.push(number.to_owned());
            }
        };
        for kind in DocumentKind::ALL {
            if let Some(slot) = self.slots.get(kind) {
                push(slot.number.as_deref());
                push(status_by(&slot.status));
            }
        }
        for entry in self.correctives.values() {
            push(entry.number.as_deref());
            push(status_by(&entry.status));
        }
        for event in &self.history {
            push(event.number.as_deref());
            push(event.by.as_deref());
        }
        numbers
    }

    /// The numbers of documents the history records as reversed.
    #[must_use]
    pub fn history_reversed_numbers(&self) -> Vec<String> {
        self.history
            .iter()
            .filter(|event| event.event == HistoryKind::Reversed)
            .filter_map(|event| event.number.clone())
            .collect()
    }

    /// The slot or corrective whose document carries `number`.
    #[must_use]
    pub fn find_by_number(&self, number: &str) -> Option<Target> {
        DocumentKind::ALL
            .into_iter()
            .find(|kind| {
                self.slots
                    .get(*kind)
                    .is_some_and(|slot| slot.number.as_deref() == Some(number))
            })
            .map(Target::Slot)
            .or_else(|| {
                self.correctives
                    .values()
                    .find(|entry| entry.number.as_deref() == Some(number))
                    .map(|entry| Target::Corrective(entry.request_id.clone()))
            })
    }

    // ----- transitions -----------------------------------------------------

    /// Allocates a pending intent on the `kind` slot and returns the
    /// generation the external id must embed.
    ///
    /// An empty slot allocates generation 0; an allocatable slot (see
    /// [`SlotStatus::is_allocatable`]) reuses its current generation, which a
    /// reversal, deletion or consumption has already bumped. The request id is
    /// recorded as owning that generation.
    ///
    /// # Errors
    ///
    /// [`LedgerError::SlotBusy`] when the slot is `pending`, `committed`,
    /// `blocked`, `reversal_unverified` or `consumed`;
    /// [`LedgerError::RequestIdKnown`] when the request id already refers to
    /// anything other than this very slot and generation (re-allocating a
    /// [`SlotStatus::Vacant`] slot under the same id is allowed).
    pub fn allocate_intent(
        &mut self,
        kind: DocumentKind,
        request_id: RequestId,
        fp: Fingerprint,
        issue_date_requested: Option<Date>,
    ) -> Result<u32, LedgerError> {
        let generation = match self.slots.get(kind) {
            None => 0,
            Some(slot) if slot.status.is_allocatable() => slot.generation,
            Some(slot) => {
                return Err(LedgerError::SlotBusy {
                    kind,
                    status: slot.status.token().to_owned(),
                });
            }
        };
        self.ensure_request_free(&request_id, Some(&RequestRef::Slot { kind, generation }))?;

        self.requests
            .insert(request_id.clone(), RequestRef::Slot { kind, generation });
        *self.slots.cell(kind) = Some(Slot {
            generation,
            request_id,
            status: SlotStatus::Pending,
            number: None,
            gross: None,
            net: None,
            test: None,
            origin: Origin::Service,
            fp,
            issue_date_requested,
            attempts: 0,
            last_attempt_at: None,
        });
        Ok(generation)
    }

    /// Allocates a pending corrective entry and returns its `cseq`.
    ///
    /// # Errors
    ///
    /// [`LedgerError::RequestIdKnown`] when the request id is already known.
    pub fn allocate_corrective(
        &mut self,
        request_id: RequestId,
        corrected_number: impl Into<String>,
        fp: Fingerprint,
    ) -> Result<u32, LedgerError> {
        self.ensure_request_free(&request_id, None)?;
        let cseq = self.next_cseq;
        self.next_cseq += 1;
        self.requests
            .insert(request_id.clone(), RequestRef::Corrective { cseq });
        self.correctives.insert(
            request_id.clone(),
            CorrectiveEntry {
                cseq,
                corrected_number: corrected_number.into(),
                request_id,
                status: SlotStatus::Pending,
                number: None,
                gross: None,
                net: None,
                test: None,
                fp,
                attempts: 0,
                last_attempt_at: None,
            },
        );
        Ok(cseq)
    }

    /// Counts an issuing attempt on a pending entry.
    ///
    /// # Errors
    ///
    /// [`LedgerError::MissingTarget`] or [`LedgerError::InvalidTransition`]
    /// when the entry is not `pending`.
    pub fn record_attempt(&mut self, target: &Target, now: Timestamp) -> Result<(), LedgerError> {
        let entry = self.entry_mut(target)?;
        if entry.status() != &SlotStatus::Pending {
            return Err(invalid(target, entry.status(), "record an attempt"));
        }
        entry.record_attempt(now);
        Ok(())
    }

    /// Commits a document to a `pending` or `blocked` entry and records an
    /// `issued`, `reconciled` or `adopted` history event.
    ///
    /// # Errors
    ///
    /// [`LedgerError::MissingTarget`] or [`LedgerError::InvalidTransition`]
    /// when the entry is neither `pending` nor `blocked`.
    pub fn commit(
        &mut self,
        target: &Target,
        document: CommittedDocument,
    ) -> Result<(), LedgerError> {
        let (kind, generation, request_id) = self.identity(target)?;
        let entry = self.entry_mut(target)?;
        if !matches!(
            entry.status(),
            SlotStatus::Pending | SlotStatus::Blocked { .. }
        ) {
            return Err(invalid(target, entry.status(), "commit"));
        }
        let event = match (document.origin, document.reconciled) {
            (Origin::Adopted, _) => HistoryKind::Adopted,
            (Origin::Service, true) => HistoryKind::Reconciled,
            (Origin::Service, false) => HistoryKind::Issued,
        };
        let number = document.number.clone();
        entry.commit(document);
        self.push_history(HistoryEvent {
            kind,
            generation,
            request_id: Some(request_id),
            number: Some(number),
            event,
            by: None,
            origin: None,
            payments_before: Vec::new(),
        });
        Ok(())
    }

    /// Records a szamlazz.hu rejection on a `pending` entry; nothing was
    /// created and the generation is kept.
    ///
    /// # Errors
    ///
    /// [`LedgerError::MissingTarget`] or [`LedgerError::InvalidTransition`]
    /// when the entry is not `pending`.
    pub fn mark_rejected(
        &mut self,
        target: &Target,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Result<(), LedgerError> {
        let entry = self.entry_mut(target)?;
        if entry.status() != &SlotStatus::Pending {
            return Err(invalid(target, entry.status(), "reject"));
        }
        entry.set_status(SlotStatus::Rejected {
            code: code.into(),
            message: message.into(),
        });
        Ok(())
    }

    /// Marks a `pending` slot `blocked` after unresolved duplicate-order-number
    /// rejections; the generation is kept.
    ///
    /// # Errors
    ///
    /// [`LedgerError::MissingTarget`] or [`LedgerError::InvalidTransition`]
    /// when the slot is not `pending`.
    pub fn mark_blocked(
        &mut self,
        kind: DocumentKind,
        existing_number: Option<String>,
    ) -> Result<(), LedgerError> {
        let target = Target::Slot(kind);
        let slot = self.slot_mut(kind)?;
        if slot.status != SlotStatus::Pending {
            return Err(invalid(&target, &slot.status, "block"));
        }
        slot.status = SlotStatus::Blocked { existing_number };
        Ok(())
    }

    /// Clears a `pending` slot because nothing of ours was created (a foreign
    /// document was detected). The slot becomes [`SlotStatus::Vacant`] with
    /// its generation preserved, so the next allocation reuses the same
    /// external id; the request id keeps referring to the slot.
    ///
    /// # Errors
    ///
    /// [`LedgerError::MissingTarget`] or [`LedgerError::InvalidTransition`]
    /// when the slot is not `pending`.
    pub fn clear_pending(&mut self, kind: DocumentKind) -> Result<(), LedgerError> {
        let target = Target::Slot(kind);
        let slot = self.slot_mut(kind)?;
        if slot.status != SlotStatus::Pending {
            return Err(invalid(&target, &slot.status, "clear"));
        }
        slot.status = SlotStatus::Vacant;
        Ok(())
    }

    /// Records a verified reversal of a `committed`, `reversal_unverified`,
    /// `pending` or `blocked` entry (the last two when the document was found
    /// already reversed), pushes a `reversed` history event with the payments
    /// snapshot, and — for a slot — bumps the generation.
    ///
    /// Returns the slot's new generation; for a corrective the unchanged
    /// `cseq` (correctives are never reissued).
    ///
    /// # Errors
    ///
    /// [`LedgerError::MissingTarget`] or [`LedgerError::InvalidTransition`]
    /// when the entry is already `reversed`, `rejected`, `consumed`,
    /// `deleted` or `vacant`.
    pub fn mark_reversed(
        &mut self,
        target: &Target,
        reversal: Reversal,
    ) -> Result<u32, LedgerError> {
        let (kind, generation, request_id) = self.identity(target)?;
        let entry = self.entry_mut(target)?;
        if !matches!(
            entry.status(),
            SlotStatus::Committed
                | SlotStatus::ReversalUnverified
                | SlotStatus::Pending
                | SlotStatus::Blocked { .. }
        ) {
            return Err(invalid(target, entry.status(), "reverse"));
        }
        if entry.number().is_none() {
            entry.set_number(reversal.number.clone());
        }
        let number = entry.number().map(str::to_owned);
        entry.set_status(SlotStatus::Reversed {
            by: reversal.by.clone(),
            origin: reversal.origin,
        });
        let next = entry.bump_generation();
        self.push_history(HistoryEvent {
            kind,
            generation,
            request_id: Some(request_id),
            number,
            event: HistoryKind::Reversed,
            by: reversal.by,
            origin: Some(reversal.origin),
            payments_before: reversal.payments_before,
        });
        Ok(next)
    }

    /// Marks a `committed` entry `reversal_unverified` after a service-side
    /// storno exhausted its attempts. The generation is kept.
    ///
    /// # Errors
    ///
    /// [`LedgerError::MissingTarget`] or [`LedgerError::InvalidTransition`]
    /// when the entry is not `committed`.
    pub fn mark_reversal_unverified(&mut self, target: &Target) -> Result<(), LedgerError> {
        let entry = self.entry_mut(target)?;
        if entry.status() != &SlotStatus::Committed {
            return Err(invalid(target, entry.status(), "mark reversal unverified"));
        }
        entry.set_status(SlotStatus::ReversalUnverified);
        Ok(())
    }

    /// Records the deletion of the `committed` proforma, bumps the generation
    /// and pushes a `deleted` history event. Returns the new generation.
    ///
    /// # Errors
    ///
    /// [`LedgerError::MissingTarget`] or [`LedgerError::InvalidTransition`]
    /// when the proforma slot is not `committed`.
    pub fn mark_deleted(&mut self) -> Result<u32, LedgerError> {
        self.close_proforma(SlotStatus::Deleted, HistoryKind::Deleted, None, "delete")
    }

    /// Records that the `committed` proforma was consumed by `by`, bumps the
    /// generation and pushes a `consumed` history event. Returns the new
    /// generation.
    ///
    /// # Errors
    ///
    /// [`LedgerError::MissingTarget`] or [`LedgerError::InvalidTransition`]
    /// when the proforma slot is not `committed`.
    pub fn mark_consumed(&mut self, by: impl Into<String>) -> Result<u32, LedgerError> {
        let by = by.into();
        self.close_proforma(
            SlotStatus::Consumed { by: by.clone() },
            HistoryKind::Consumed,
            Some(by),
            "consume",
        )
    }

    /// Hands a `pending` slot to `new_request_id`: the old request id becomes
    /// [`RequestRef::Abandoned`], an `abandoned` history event is pushed, the
    /// fingerprint and pinned issue date are replaced and the attempt counter
    /// restarts. The generation and external id stay the same.
    ///
    /// # Errors
    ///
    /// [`LedgerError::MissingTarget`] or [`LedgerError::InvalidTransition`]
    /// when the slot is not `pending`; [`LedgerError::RequestIdKnown`] when
    /// the new request id is already known.
    pub fn take_over(
        &mut self,
        kind: DocumentKind,
        new_request_id: RequestId,
        fp: Fingerprint,
        issue_date_requested: Option<Date>,
    ) -> Result<(), LedgerError> {
        let target = Target::Slot(kind);
        self.ensure_request_free(&new_request_id, None)?;
        let slot = self.slot_mut(kind)?;
        if slot.status != SlotStatus::Pending {
            return Err(invalid(&target, &slot.status, "take over"));
        }
        let generation = slot.generation;
        let old = std::mem::replace(&mut slot.request_id, new_request_id.clone());
        slot.fp = fp;
        slot.issue_date_requested = issue_date_requested;
        slot.attempts = 0;
        slot.last_attempt_at = None;
        self.requests.insert(old.clone(), RequestRef::Abandoned);
        self.requests
            .insert(new_request_id, RequestRef::Slot { kind, generation });
        self.push_history(HistoryEvent {
            kind: kind.into(),
            generation,
            request_id: Some(old),
            number: None,
            event: HistoryKind::Abandoned,
            by: None,
            origin: None,
            payments_before: Vec::new(),
        });
        Ok(())
    }

    /// Records the last foreign document seen under the order number.
    pub fn set_foreign_hint(
        &mut self,
        number: impl Into<String>,
        document_type: impl Into<String>,
    ) {
        self.foreign_hint = Some(ForeignHint::new(number, document_type));
    }

    /// An operator asserts that the recorded document `number` was reversed
    /// outside the service: the entry becomes `reversed{origin: operator}`
    /// and, for a slot, the generation is bumped. Returns the new generation
    /// (or the corrective's `cseq`).
    ///
    /// # Errors
    ///
    /// [`LedgerError::UnknownNumber`] when no entry carries the number;
    /// [`LedgerError::InvalidTransition`] when the entry is not `committed`
    /// or `reversal_unverified`.
    pub fn record_operator_reversal(
        &mut self,
        number: &str,
        storno_number: Option<String>,
    ) -> Result<u32, LedgerError> {
        let target = self
            .find_by_number(number)
            .ok_or_else(|| LedgerError::UnknownNumber(number.to_owned()))?;
        let status = self.entry_mut(&target)?.status().clone();
        if !matches!(
            status,
            SlotStatus::Committed | SlotStatus::ReversalUnverified
        ) {
            return Err(invalid(&target, &status, "record an operator reversal"));
        }
        let mut reversal = Reversal::new(ReversalOrigin::Operator);
        reversal.by = storno_number;
        self.mark_reversed(&target, reversal)
    }

    /// An operator asserts that the recorded document `number` is live: a
    /// `reversal_unverified` entry returns to `committed` with a
    /// `recorded_by_operator` history event; a `committed` entry is left as
    /// is.
    ///
    /// # Errors
    ///
    /// [`LedgerError::UnknownNumber`] when no entry carries the number;
    /// [`LedgerError::InvalidTransition`] when the entry is neither
    /// `committed` nor `reversal_unverified`.
    pub fn record_operator_live(&mut self, number: &str) -> Result<(), LedgerError> {
        let target = self
            .find_by_number(number)
            .ok_or_else(|| LedgerError::UnknownNumber(number.to_owned()))?;
        let (kind, generation, request_id) = self.identity(&target)?;
        let entry = self.entry_mut(&target)?;
        match entry.status() {
            SlotStatus::Committed => Ok(()),
            SlotStatus::ReversalUnverified => {
                entry.set_status(SlotStatus::Committed);
                self.push_history(HistoryEvent {
                    kind,
                    generation,
                    request_id: Some(request_id),
                    number: Some(number.to_owned()),
                    event: HistoryKind::RecordedByOperator,
                    by: None,
                    origin: None,
                    payments_before: Vec::new(),
                });
                Ok(())
            }
            other => Err(invalid(&target, other, "record live")),
        }
    }

    /// An operator drops a slot whose document szamlazz.hu no longer knows
    /// (or that is `blocked` / `reversal_unverified`): the slot becomes
    /// [`SlotStatus::Vacant`] with a bumped generation and a `forgotten`
    /// history event. Returns the new generation.
    ///
    /// # Errors
    ///
    /// [`LedgerError::MissingTarget`] or [`LedgerError::InvalidTransition`]
    /// when the slot is not `committed`, `blocked` or `reversal_unverified`.
    pub fn forget(&mut self, kind: DocumentKind) -> Result<u32, LedgerError> {
        let target = Target::Slot(kind);
        let slot = self.slot_mut(kind)?;
        if !matches!(
            slot.status,
            SlotStatus::Committed | SlotStatus::Blocked { .. } | SlotStatus::ReversalUnverified
        ) {
            return Err(invalid(&target, &slot.status, "forget"));
        }
        let generation = slot.generation;
        let request_id = slot.request_id.clone();
        let number = slot.number.clone();
        slot.status = SlotStatus::Vacant;
        slot.generation += 1;
        let next = slot.generation;
        self.push_history(HistoryEvent {
            kind: kind.into(),
            generation,
            request_id: Some(request_id),
            number,
            event: HistoryKind::Forgotten,
            by: None,
            origin: None,
            payments_before: Vec::new(),
        });
        Ok(next)
    }

    /// Records the supplier id seen in a query response.
    ///
    /// # Errors
    ///
    /// [`LedgerError::SupplierMismatch`] when a different id is already
    /// recorded.
    pub fn learn_supplier_id(&mut self, id: u64) -> Result<(), LedgerError> {
        match self.supplier_id {
            None => {
                self.supplier_id = Some(id);
                Ok(())
            }
            Some(recorded) if recorded == id => Ok(()),
            Some(recorded) => Err(LedgerError::SupplierMismatch { recorded, seen: id }),
        }
    }

    // ----- projection ------------------------------------------------------

    /// The serialisable projection of the ledger.
    #[must_use]
    pub fn snapshot(&self, freshness: Freshness) -> OrderSnapshot {
        let mut snapshot = OrderSnapshot::new(freshness);
        snapshot.supplier_id = self.supplier_id;
        snapshot.slots = SlotsSnapshot::default();
        snapshot.slots.proforma = self.slots.proforma.as_ref().map(slot_snapshot);
        snapshot.slots.invoice = self.slots.invoice.as_ref().map(slot_snapshot);
        snapshot.slots.prepayment = self.slots.prepayment.as_ref().map(slot_snapshot);
        snapshot.slots.r#final = self.slots.r#final.as_ref().map(slot_snapshot);
        snapshot.correctives = self
            .correctives()
            .into_iter()
            .map(|entry| {
                let mut projection = CorrectiveSnapshot::new(
                    entry.request_id.clone(),
                    entry.cseq,
                    entry.corrected_number.clone(),
                    entry.status.token(),
                );
                projection.number.clone_from(&entry.number);
                projection
            })
            .collect();
        snapshot.foreign_hint.clone_from(&self.foreign_hint);
        snapshot.history = self
            .history
            .iter()
            .map(|event| {
                let mut projection =
                    HistorySnapshot::new(event.kind, event.generation, event.event.token());
                projection.request_id.clone_from(&event.request_id);
                projection.number.clone_from(&event.number);
                projection.by.clone_from(&event.by);
                projection.origin = event.origin.map(|origin| origin.token().to_owned());
                projection
            })
            .collect();
        snapshot
    }

    // ----- helpers ---------------------------------------------------------

    fn ensure_request_free(
        &self,
        request_id: &RequestId,
        allowed: Option<&RequestRef>,
    ) -> Result<(), LedgerError> {
        match self.requests.get(request_id) {
            None => Ok(()),
            Some(existing) if Some(existing) == allowed => Ok(()),
            Some(_) => Err(LedgerError::RequestIdKnown(request_id.clone())),
        }
    }

    fn slot_mut(&mut self, kind: DocumentKind) -> Result<&mut Slot, LedgerError> {
        self.slots
            .get_mut(kind)
            .ok_or(LedgerError::MissingTarget(Target::Slot(kind)))
    }

    fn entry_mut(&mut self, target: &Target) -> Result<&mut dyn Entry, LedgerError> {
        match target {
            Target::Slot(kind) => Ok(self.slot_mut(*kind)?),
            Target::Corrective(request_id) => self
                .correctives
                .get_mut(request_id)
                .map(|entry| entry as &mut dyn Entry)
                .ok_or_else(|| LedgerError::MissingTarget(target.clone())),
        }
    }

    /// The history identity of an entry: kind, generation (or `cseq`) and
    /// owning request id.
    fn identity(&self, target: &Target) -> Result<(IssuedKind, u32, RequestId), LedgerError> {
        match target {
            Target::Slot(kind) => self
                .slots
                .get(*kind)
                .map(|slot| ((*kind).into(), slot.generation, slot.request_id.clone()))
                .ok_or_else(|| LedgerError::MissingTarget(target.clone())),
            Target::Corrective(request_id) => self
                .correctives
                .get(request_id)
                .map(|entry| (IssuedKind::Corrective, entry.cseq, entry.request_id.clone()))
                .ok_or_else(|| LedgerError::MissingTarget(target.clone())),
        }
    }

    fn close_proforma(
        &mut self,
        status: SlotStatus,
        event: HistoryKind,
        by: Option<String>,
        action: &'static str,
    ) -> Result<u32, LedgerError> {
        let kind = DocumentKind::Proforma;
        let target = Target::Slot(kind);
        let slot = self.slot_mut(kind)?;
        if slot.status != SlotStatus::Committed {
            return Err(invalid(&target, &slot.status, action));
        }
        let generation = slot.generation;
        let request_id = slot.request_id.clone();
        let number = slot.number.clone();
        slot.status = status;
        slot.generation += 1;
        let next = slot.generation;
        self.push_history(HistoryEvent {
            kind: kind.into(),
            generation,
            request_id: Some(request_id),
            number,
            event,
            by,
            origin: None,
            payments_before: Vec::new(),
        });
        Ok(next)
    }

    fn push_history(&mut self, event: HistoryEvent) {
        self.history.push(event);
        if self.history.len() > Self::HISTORY_CAP {
            let excess = self.history.len() - Self::HISTORY_CAP;
            self.history.drain(..excess);
        }
    }
}

/// The operations shared by slots and corrective entries.
trait Entry {
    fn status(&self) -> &SlotStatus;
    fn set_status(&mut self, status: SlotStatus);
    fn number(&self) -> Option<&str>;
    fn set_number(&mut self, number: Option<String>);
    fn record_attempt(&mut self, now: Timestamp);
    fn commit(&mut self, document: CommittedDocument);
    /// Bumps the generation and returns the new value; a corrective returns
    /// its `cseq` unchanged.
    fn bump_generation(&mut self) -> u32;
}

impl Entry for Slot {
    fn status(&self) -> &SlotStatus {
        &self.status
    }

    fn set_status(&mut self, status: SlotStatus) {
        self.status = status;
    }

    fn number(&self) -> Option<&str> {
        self.number.as_deref()
    }

    fn set_number(&mut self, number: Option<String>) {
        self.number = number;
    }

    fn record_attempt(&mut self, now: Timestamp) {
        self.attempts += 1;
        self.last_attempt_at = Some(now);
    }

    fn commit(&mut self, document: CommittedDocument) {
        self.status = SlotStatus::Committed;
        self.number = Some(document.number);
        self.gross = document.gross;
        self.net = document.net;
        self.test = document.test;
        self.origin = document.origin;
    }

    fn bump_generation(&mut self) -> u32 {
        self.generation += 1;
        self.generation
    }
}

impl Entry for CorrectiveEntry {
    fn status(&self) -> &SlotStatus {
        &self.status
    }

    fn set_status(&mut self, status: SlotStatus) {
        self.status = status;
    }

    fn number(&self) -> Option<&str> {
        self.number.as_deref()
    }

    fn set_number(&mut self, number: Option<String>) {
        self.number = number;
    }

    fn record_attempt(&mut self, now: Timestamp) {
        self.attempts += 1;
        self.last_attempt_at = Some(now);
    }

    fn commit(&mut self, document: CommittedDocument) {
        self.status = SlotStatus::Committed;
        self.number = Some(document.number);
        self.gross = document.gross;
        self.net = document.net;
        self.test = document.test;
    }

    fn bump_generation(&mut self) -> u32 {
        self.cseq
    }
}

fn invalid(target: &Target, from: &SlotStatus, action: &'static str) -> LedgerError {
    LedgerError::InvalidTransition {
        target: target.clone(),
        from: from.token().to_owned(),
        action,
    }
}

/// The storno number a `reversed` status carries.
fn status_by(status: &SlotStatus) -> Option<&str> {
    match status {
        SlotStatus::Reversed { by, .. } => by.as_deref(),
        SlotStatus::Consumed { by } => Some(by),
        _ => None,
    }
}

fn slot_snapshot(slot: &Slot) -> SlotSnapshot {
    let mut projection = SlotSnapshot::new(
        slot.generation,
        slot.request_id.clone(),
        slot.status.token(),
        slot.attempts,
    );
    projection.number.clone_from(&slot.number);
    projection.gross = slot.gross;
    projection.origin = match &slot.status {
        SlotStatus::Reversed { origin, .. } => Some(origin.token().to_owned()),
        SlotStatus::Committed
        | SlotStatus::ReversalUnverified
        | SlotStatus::Consumed { .. }
        | SlotStatus::Deleted => Some(slot.origin.token().to_owned()),
        SlotStatus::Pending
        | SlotStatus::Blocked { .. }
        | SlotStatus::Rejected { .. }
        | SlotStatus::Vacant => None,
    };
    projection
}

#[cfg(test)]
mod tests;
