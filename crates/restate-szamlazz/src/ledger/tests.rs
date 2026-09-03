//! Ledger transition tests: every transition, the generation-bump rule, the
//! allocate refusal table, take-over bookkeeping, `our_numbers`, serde.

use jiff::Timestamp;
use jiff::civil::date;
use rust_decimal::dec;
use serde_json::json;

use super::*;

fn rid(id: &str) -> RequestId {
    id.parse().expect("valid request id")
}

fn fp(tag: &str) -> Fingerprint {
    Fingerprint::compute(b"secret", tag, dec!(100), None, None, None)
}

fn now() -> Timestamp {
    Timestamp::from_second(1_800_000_000).expect("valid timestamp")
}

fn committed(number: &str) -> CommittedDocument {
    CommittedDocument::issued(number)
        .with_totals(Some(dec!(1270)), Some(dec!(1000)))
        .with_test(true)
}

/// A ledger with the invoice slot committed as `SZ-1` under `r-1`.
fn with_invoice() -> Ledger {
    let mut ledger = Ledger::new();
    ledger
        .allocate_intent(DocumentKind::Invoice, rid("r-1"), fp("a"), None)
        .expect("allocate");
    ledger
        .record_attempt(&Target::Slot(DocumentKind::Invoice), now())
        .expect("attempt");
    ledger
        .commit(&Target::Slot(DocumentKind::Invoice), committed("SZ-1"))
        .expect("commit");
    ledger
}

fn with_proforma() -> Ledger {
    let mut ledger = Ledger::new();
    ledger
        .allocate_intent(DocumentKind::Proforma, rid("p-1"), fp("p"), None)
        .expect("allocate");
    ledger
        .commit(&Target::Slot(DocumentKind::Proforma), committed("D-1"))
        .expect("commit");
    ledger
}

fn status(ledger: &Ledger, kind: DocumentKind) -> &SlotStatus {
    &ledger.slot(kind).expect("slot").status
}

#[test]
fn new_ledger_is_empty() {
    let ledger = Ledger::new();
    assert_eq!(ledger, Ledger::default());
    assert_eq!(ledger.version(), 1);
    assert_eq!(ledger.supplier_id(), None);
    assert_eq!(ledger.next_cseq(), 1);
    for kind in DocumentKind::ALL {
        assert!(ledger.slot(kind).is_none());
    }
    assert!(ledger.correctives().is_empty());
    assert!(ledger.history().is_empty());
    assert!(ledger.our_numbers().is_empty());
    assert_eq!(ledger.foreign_hint(), None);
}

#[test]
fn allocate_records_intent_and_request() {
    let mut ledger = Ledger::new();
    let generation = ledger
        .allocate_intent(
            DocumentKind::Invoice,
            rid("r-1"),
            fp("a"),
            Some(date(2026, 9, 3)),
        )
        .expect("allocate");
    assert_eq!(generation, 0);
    let slot = ledger.slot(DocumentKind::Invoice).expect("slot");
    assert_eq!(slot.generation, 0);
    assert_eq!(slot.request_id, rid("r-1"));
    assert_eq!(slot.status, SlotStatus::Pending);
    assert_eq!(slot.fp, fp("a"));
    assert_eq!(slot.issue_date_requested, Some(date(2026, 9, 3)));
    assert_eq!(slot.attempts, 0);
    assert_eq!(slot.number, None);
    assert_eq!(
        ledger.lookup_request(&rid("r-1")),
        Some(&RequestRef::Slot {
            kind: DocumentKind::Invoice,
            generation: 0
        })
    );
    assert_eq!(ledger.lookup_request(&rid("r-2")), None);
    assert!(ledger.history().is_empty(), "allocation is not an event");
}

#[test]
fn allocate_refuses_known_request_id_elsewhere() {
    let mut ledger = with_invoice();
    assert_eq!(
        ledger.allocate_intent(DocumentKind::Proforma, rid("r-1"), fp("a"), None),
        Err(LedgerError::RequestIdKnown(rid("r-1")))
    );
    assert_eq!(
        ledger.allocate_corrective(rid("r-1"), "SZ-1", fp("c")),
        Err(LedgerError::RequestIdKnown(rid("r-1")))
    );
}

/// A step that moves a freshly allocated invoice slot into a busy status.
type Prepare = Box<dyn Fn(&mut Ledger, &Target)>;

#[test]
fn allocate_refuses_busy_slots() {
    let invoice = Target::Slot(DocumentKind::Invoice);
    let busy: Vec<(&str, Prepare)> = vec![
        (
            "pending",
            Box::new(|_ledger: &mut Ledger, _target: &Target| {}),
        ),
        (
            "committed",
            Box::new(|ledger: &mut Ledger, target: &Target| {
                ledger.commit(target, committed("SZ-1")).expect("commit");
            }),
        ),
        (
            "blocked",
            Box::new(|ledger: &mut Ledger, _target: &Target| {
                ledger
                    .mark_blocked(DocumentKind::Invoice, Some("SZ-9".to_owned()))
                    .expect("block");
            }),
        ),
        (
            "reversal_unverified",
            Box::new(|ledger: &mut Ledger, target: &Target| {
                ledger.commit(target, committed("SZ-1")).expect("commit");
                ledger.mark_reversal_unverified(target).expect("unverified");
            }),
        ),
    ];
    for (expected, prepare) in busy {
        let mut ledger = Ledger::new();
        ledger
            .allocate_intent(DocumentKind::Invoice, rid("r-1"), fp("a"), None)
            .expect("allocate");
        prepare(&mut ledger, &invoice);
        assert_eq!(
            ledger.allocate_intent(DocumentKind::Invoice, rid("r-2"), fp("b"), None),
            Err(LedgerError::SlotBusy {
                kind: DocumentKind::Invoice,
                status: expected.to_owned(),
            }),
            "{expected}"
        );
        assert_eq!(status(&ledger, DocumentKind::Invoice).token(), expected);
    }

    let mut ledger = with_proforma();
    ledger.mark_consumed("SZ-7").expect("consume");
    assert_eq!(
        ledger.allocate_intent(DocumentKind::Proforma, rid("p-2"), fp("q"), None),
        Err(LedgerError::SlotBusy {
            kind: DocumentKind::Proforma,
            status: "consumed".to_owned(),
        })
    );
}

#[test]
fn allocate_accepts_open_slots() {
    let invoice = Target::Slot(DocumentKind::Invoice);
    // Rejected and vacant reuse the generation, reversed and deleted use the
    // bumped one.
    let mut ledger = Ledger::new();
    ledger
        .allocate_intent(DocumentKind::Invoice, rid("r-1"), fp("a"), None)
        .expect("allocate");
    ledger
        .mark_rejected(&invoice, "259", "net mismatch")
        .expect("reject");
    assert_eq!(
        ledger.allocate_intent(DocumentKind::Invoice, rid("r-2"), fp("b"), None),
        Ok(0)
    );
    ledger.clear_pending(DocumentKind::Invoice).expect("clear");
    assert_eq!(status(&ledger, DocumentKind::Invoice), &SlotStatus::Vacant);
    ledger
        .allocate_intent(DocumentKind::Proforma, rid("p-1"), fp("p"), None)
        .expect("allocate proforma");
    assert_eq!(
        ledger.allocate_intent(DocumentKind::Invoice, rid("p-1"), fp("b"), None),
        Err(LedgerError::RequestIdKnown(rid("p-1"))),
        "a request id that owns another entry cannot allocate"
    );
    assert_eq!(
        ledger.allocate_intent(DocumentKind::Invoice, rid("r-2"), fp("b"), None),
        Ok(0),
        "a vacant slot may be re-allocated under the same request id"
    );

    for origin in [
        ReversalOrigin::Service,
        ReversalOrigin::External,
        ReversalOrigin::Operator,
    ] {
        let mut ledger = with_invoice();
        assert_eq!(ledger.mark_reversed(&invoice, Reversal::new(origin)), Ok(1));
        assert_eq!(
            ledger.allocate_intent(DocumentKind::Invoice, rid("r-2"), fp("b"), None),
            Ok(1),
            "{origin:?}"
        );
        assert_eq!(
            ledger.lookup_request(&rid("r-1")),
            Some(&RequestRef::Slot {
                kind: DocumentKind::Invoice,
                generation: 0
            }),
            "the old request keeps pointing at its generation"
        );
    }

    let mut ledger = with_proforma();
    assert_eq!(ledger.mark_deleted(), Ok(1));
    assert_eq!(
        ledger.allocate_intent(DocumentKind::Proforma, rid("p-2"), fp("q"), None),
        Ok(1)
    );
}

#[test]
fn attempts_are_counted_only_while_pending() {
    let mut ledger = Ledger::new();
    let invoice = Target::Slot(DocumentKind::Invoice);
    assert_eq!(
        ledger.record_attempt(&invoice, now()),
        Err(LedgerError::MissingTarget(invoice.clone()))
    );
    ledger
        .allocate_intent(DocumentKind::Invoice, rid("r-1"), fp("a"), None)
        .expect("allocate");
    ledger.record_attempt(&invoice, now()).expect("first");
    ledger.record_attempt(&invoice, now()).expect("second");
    let slot = ledger.slot(DocumentKind::Invoice).expect("slot");
    assert_eq!(slot.attempts, 2);
    assert_eq!(slot.last_attempt_at, Some(now()));
    ledger.commit(&invoice, committed("SZ-1")).expect("commit");
    assert_eq!(
        ledger.record_attempt(&invoice, now()),
        Err(LedgerError::InvalidTransition {
            target: invoice,
            from: "committed".to_owned(),
            action: "record an attempt",
        })
    );
}

#[test]
fn commit_records_document_and_history() {
    let ledger = with_invoice();
    let slot = ledger.slot(DocumentKind::Invoice).expect("slot");
    assert_eq!(slot.status, SlotStatus::Committed);
    assert_eq!(slot.number.as_deref(), Some("SZ-1"));
    assert_eq!(slot.gross, Some(dec!(1270)));
    assert_eq!(slot.net, Some(dec!(1000)));
    assert_eq!(slot.test, Some(true));
    assert_eq!(slot.origin, Origin::Service);
    assert_eq!(slot.attempts, 1);
    let events = ledger.history();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event, HistoryKind::Issued);
    assert_eq!(events[0].kind, IssuedKind::Invoice);
    assert_eq!(events[0].generation, 0);
    assert_eq!(events[0].request_id, Some(rid("r-1")));
    assert_eq!(events[0].number.as_deref(), Some("SZ-1"));
    assert_eq!(
        ledger.find_by_number("SZ-1"),
        Some(Target::Slot(DocumentKind::Invoice))
    );
    assert_eq!(ledger.find_by_number("SZ-2"), None);
}

#[test]
fn commit_variants_and_preconditions() {
    let invoice = Target::Slot(DocumentKind::Invoice);
    let mut ledger = Ledger::new();
    ledger
        .allocate_intent(DocumentKind::Invoice, rid("r-1"), fp("a"), None)
        .expect("allocate");
    ledger
        .commit(
            &invoice,
            CommittedDocument::reconciled("SZ-1", Origin::Service),
        )
        .expect("reconcile");
    assert_eq!(ledger.history()[0].event, HistoryKind::Reconciled);
    assert_eq!(
        ledger.commit(&invoice, committed("SZ-1")),
        Err(LedgerError::InvalidTransition {
            target: invoice.clone(),
            from: "committed".to_owned(),
            action: "commit",
        })
    );

    let mut ledger = Ledger::new();
    ledger
        .allocate_intent(DocumentKind::Invoice, rid("r-1"), fp("a"), None)
        .expect("allocate");
    ledger
        .mark_blocked(DocumentKind::Invoice, None)
        .expect("block");
    ledger
        .commit(
            &invoice,
            CommittedDocument::reconciled("SZ-1", Origin::Adopted),
        )
        .expect("a blocked slot may be reconciled");
    assert_eq!(ledger.history()[0].event, HistoryKind::Adopted);
    assert_eq!(
        ledger.slot(DocumentKind::Invoice).expect("slot").origin,
        Origin::Adopted
    );
}

#[test]
fn rejected_blocked_and_cleared_keep_the_generation() {
    let invoice = Target::Slot(DocumentKind::Invoice);
    let mut ledger = with_invoice();
    ledger
        .mark_reversed(&invoice, Reversal::new(ReversalOrigin::Service))
        .expect("reverse");
    ledger
        .allocate_intent(DocumentKind::Invoice, rid("r-2"), fp("b"), None)
        .expect("allocate at gen 1");

    let mut rejected = ledger.clone();
    rejected
        .mark_rejected(&invoice, "152", "duplicate")
        .expect("reject");
    assert_eq!(
        status(&rejected, DocumentKind::Invoice),
        &SlotStatus::Rejected {
            code: "152".to_owned(),
            message: "duplicate".to_owned(),
        }
    );
    assert_eq!(
        rejected
            .slot(DocumentKind::Invoice)
            .expect("slot")
            .generation,
        1
    );
    assert_eq!(
        rejected.mark_rejected(&invoice, "1", "again"),
        Err(LedgerError::InvalidTransition {
            target: invoice.clone(),
            from: "rejected".to_owned(),
            action: "reject",
        })
    );

    let mut blocked = ledger.clone();
    blocked
        .mark_blocked(DocumentKind::Invoice, Some("SZ-77".to_owned()))
        .expect("block");
    assert_eq!(
        status(&blocked, DocumentKind::Invoice),
        &SlotStatus::Blocked {
            existing_number: Some("SZ-77".to_owned())
        }
    );
    assert_eq!(
        blocked
            .slot(DocumentKind::Invoice)
            .expect("slot")
            .generation,
        1
    );
    assert!(blocked.mark_blocked(DocumentKind::Invoice, None).is_err());
    assert!(blocked.mark_blocked(DocumentKind::Final, None).is_err());

    let mut cleared = ledger;
    cleared.clear_pending(DocumentKind::Invoice).expect("clear");
    assert_eq!(status(&cleared, DocumentKind::Invoice), &SlotStatus::Vacant);
    assert_eq!(
        cleared
            .slot(DocumentKind::Invoice)
            .expect("slot")
            .generation,
        1
    );
    assert_eq!(cleared.history().len(), 2, "clearing is not an event");
    assert!(cleared.clear_pending(DocumentKind::Invoice).is_err());
    assert_eq!(
        cleared.allocate_intent(DocumentKind::Invoice, rid("r-3"), fp("c"), None),
        Ok(1),
        "the next allocation reuses generation 1"
    );
}

#[test]
fn reversal_bumps_generation_and_records_payments() {
    let invoice = Target::Slot(DocumentKind::Invoice);
    let mut ledger = with_invoice();
    let next = ledger
        .mark_reversed(
            &invoice,
            Reversal::new(ReversalOrigin::External)
                .with_by("SS-1")
                .with_payments_before(vec![dec!(500), dec!(770)]),
        )
        .expect("reverse");
    assert_eq!(next, 1);
    let slot = ledger.slot(DocumentKind::Invoice).expect("slot");
    assert_eq!(slot.generation, 1);
    assert_eq!(
        slot.status,
        SlotStatus::Reversed {
            by: Some("SS-1".to_owned()),
            origin: ReversalOrigin::External,
        }
    );
    assert_eq!(slot.number.as_deref(), Some("SZ-1"), "the number stays");
    let event = ledger.history().last().expect("event");
    assert_eq!(event.event, HistoryKind::Reversed);
    assert_eq!(event.generation, 0);
    assert_eq!(event.number.as_deref(), Some("SZ-1"));
    assert_eq!(event.by.as_deref(), Some("SS-1"));
    assert_eq!(event.origin, Some(ReversalOrigin::External));
    assert_eq!(event.payments_before, vec![dec!(500), dec!(770)]);
    assert_eq!(ledger.history_reversed_numbers(), vec!["SZ-1".to_owned()]);
    assert_eq!(
        ledger.mark_reversed(&invoice, Reversal::new(ReversalOrigin::Service)),
        Err(LedgerError::InvalidTransition {
            target: invoice,
            from: "reversed".to_owned(),
            action: "reverse",
        }),
        "a reversal is recorded once"
    );
}

#[test]
fn pending_slot_found_reversed_learns_its_number() {
    let invoice = Target::Slot(DocumentKind::Invoice);
    let mut ledger = Ledger::new();
    ledger
        .allocate_intent(DocumentKind::Invoice, rid("r-1"), fp("a"), None)
        .expect("allocate");
    let next = ledger
        .mark_reversed(
            &invoice,
            Reversal::new(ReversalOrigin::External).with_number("SZ-3"),
        )
        .expect("reverse pending");
    assert_eq!(next, 1);
    let slot = ledger.slot(DocumentKind::Invoice).expect("slot");
    assert_eq!(slot.number.as_deref(), Some("SZ-3"));
    assert_eq!(ledger.history()[0].number.as_deref(), Some("SZ-3"));
    assert_eq!(ledger.our_numbers(), vec!["SZ-3".to_owned()]);
}

#[test]
fn blocked_slot_found_reversed_learns_its_number() {
    let invoice = Target::Slot(DocumentKind::Invoice);
    let mut ledger = Ledger::new();
    ledger
        .allocate_intent(DocumentKind::Invoice, rid("r-1"), fp("a"), None)
        .expect("allocate");
    ledger
        .mark_blocked(DocumentKind::Invoice, None)
        .expect("block");
    let next = ledger
        .mark_reversed(
            &invoice,
            Reversal::new(ReversalOrigin::External).with_number("SZ-4"),
        )
        .expect("a blocked slot whose document turns up reversed is reversed");
    assert_eq!(next, 1);
    let slot = ledger.slot(DocumentKind::Invoice).expect("slot");
    assert_eq!(slot.number.as_deref(), Some("SZ-4"));
    assert_eq!(
        slot.status,
        SlotStatus::Reversed {
            by: None,
            origin: ReversalOrigin::External,
        }
    );
    assert_eq!(slot.generation, 1);
    let event = ledger.history().last().expect("event");
    assert_eq!(event.event, HistoryKind::Reversed);
    assert_eq!(event.generation, 0);
    assert_eq!(event.number.as_deref(), Some("SZ-4"));
}

#[test]
fn reversal_unverified_round_trip() {
    let invoice = Target::Slot(DocumentKind::Invoice);
    let mut ledger = with_invoice();
    ledger
        .mark_reversal_unverified(&invoice)
        .expect("unverified");
    assert_eq!(
        status(&ledger, DocumentKind::Invoice),
        &SlotStatus::ReversalUnverified
    );
    assert_eq!(
        ledger.slot(DocumentKind::Invoice).expect("slot").generation,
        0,
        "no bump"
    );
    assert!(ledger.mark_reversal_unverified(&invoice).is_err());

    let mut confirmed = ledger.clone();
    assert_eq!(
        confirmed.mark_reversed(
            &invoice,
            Reversal::new(ReversalOrigin::Service).with_by("SS-1")
        ),
        Ok(1)
    );

    ledger.record_operator_live("SZ-1").expect("live");
    assert_eq!(
        status(&ledger, DocumentKind::Invoice),
        &SlotStatus::Committed
    );
    assert_eq!(
        ledger.history().last().expect("event").event,
        HistoryKind::RecordedByOperator
    );
    ledger
        .record_operator_live("SZ-1")
        .expect("live on committed is a no-op");
    assert_eq!(ledger.history().len(), 2);
    assert_eq!(
        ledger.record_operator_live("SZ-404"),
        Err(LedgerError::UnknownNumber("SZ-404".to_owned()))
    );
}

#[test]
fn operator_reversal_bumps_and_is_strict() {
    let mut ledger = with_invoice();
    assert_eq!(
        ledger.record_operator_reversal("SZ-1", Some("SS-9".to_owned())),
        Ok(1)
    );
    assert_eq!(
        status(&ledger, DocumentKind::Invoice),
        &SlotStatus::Reversed {
            by: Some("SS-9".to_owned()),
            origin: ReversalOrigin::Operator,
        }
    );
    let event = ledger.history().last().expect("event");
    assert_eq!(event.origin, Some(ReversalOrigin::Operator));
    assert!(event.payments_before.is_empty());
    assert_eq!(
        ledger.record_operator_reversal("SZ-1", None),
        Err(LedgerError::InvalidTransition {
            target: Target::Slot(DocumentKind::Invoice),
            from: "reversed".to_owned(),
            action: "record an operator reversal",
        })
    );
    assert_eq!(
        ledger.record_operator_reversal("SZ-404", None),
        Err(LedgerError::UnknownNumber("SZ-404".to_owned()))
    );
    assert!(ledger.record_operator_live("SZ-1").is_err());
}

#[test]
fn proforma_deleted_and_consumed_bump() {
    let mut deleted = with_proforma();
    assert_eq!(deleted.mark_deleted(), Ok(1));
    assert_eq!(
        status(&deleted, DocumentKind::Proforma),
        &SlotStatus::Deleted
    );
    let event = deleted.history().last().expect("event");
    assert_eq!(event.event, HistoryKind::Deleted);
    assert_eq!(event.kind, IssuedKind::Proforma);
    assert_eq!(event.generation, 0);
    assert_eq!(event.number.as_deref(), Some("D-1"));
    assert!(deleted.mark_deleted().is_err());
    assert!(deleted.mark_consumed("SZ-1").is_err());

    let mut consumed = with_proforma();
    assert_eq!(consumed.mark_consumed("SZ-4"), Ok(1));
    assert_eq!(
        status(&consumed, DocumentKind::Proforma),
        &SlotStatus::Consumed {
            by: "SZ-4".to_owned()
        }
    );
    let event = consumed.history().last().expect("event");
    assert_eq!(event.event, HistoryKind::Consumed);
    assert_eq!(event.by.as_deref(), Some("SZ-4"));
    assert!(consumed.our_numbers().contains(&"SZ-4".to_owned()));

    let mut empty = Ledger::new();
    assert_eq!(
        empty.mark_deleted(),
        Err(LedgerError::MissingTarget(Target::Slot(
            DocumentKind::Proforma
        )))
    );
}

#[test]
fn issuing_the_invoice_consumes_the_proforma() {
    let invoice = Target::Slot(DocumentKind::Invoice);
    let mut ledger = with_proforma();
    ledger
        .allocate_intent(DocumentKind::Invoice, rid("r-1"), fp("a"), None)
        .expect("allocate");
    assert_eq!(
        ledger.mark_consumed("SZ-1"),
        Ok(1),
        "the proforma is consumed as soon as the converting invoice is committed"
    );
    ledger.commit(&invoice, committed("SZ-1")).expect("commit");

    let proforma = ledger.slot(DocumentKind::Proforma).expect("proforma");
    assert_eq!(
        proforma.status,
        SlotStatus::Consumed {
            by: "SZ-1".to_owned()
        }
    );
    assert_eq!(proforma.generation, 1);
    assert_eq!(proforma.number.as_deref(), Some("D-1"), "the number stays");
    assert_eq!(
        status(&ledger, DocumentKind::Invoice),
        &SlotStatus::Committed
    );
    let events: Vec<_> = ledger.history().iter().map(|event| event.event).collect();
    assert_eq!(
        events,
        [
            HistoryKind::Issued,
            HistoryKind::Consumed,
            HistoryKind::Issued
        ]
    );
    assert_eq!(ledger.history()[1].by.as_deref(), Some("SZ-1"));
    assert_eq!(
        ledger.allocate_intent(DocumentKind::Proforma, rid("p-2"), fp("q"), None),
        Err(LedgerError::SlotBusy {
            kind: DocumentKind::Proforma,
            status: "consumed".to_owned(),
        }),
        "a consumed proforma is terminal for the order"
    );
    assert!(ledger.mark_consumed("SZ-2").is_err(), "consumed once");
}

#[test]
fn take_over_bookkeeping() {
    let mut ledger = Ledger::new();
    ledger
        .allocate_intent(
            DocumentKind::Invoice,
            rid("r-1"),
            fp("a"),
            Some(date(2026, 9, 1)),
        )
        .expect("allocate");
    let invoice = Target::Slot(DocumentKind::Invoice);
    ledger.record_attempt(&invoice, now()).expect("attempt");
    ledger.record_attempt(&invoice, now()).expect("attempt");

    ledger
        .take_over(
            DocumentKind::Invoice,
            rid("r-2"),
            fp("b"),
            Some(date(2026, 9, 3)),
        )
        .expect("take over");
    let slot = ledger.slot(DocumentKind::Invoice).expect("slot");
    assert_eq!(slot.request_id, rid("r-2"));
    assert_eq!(slot.generation, 0, "same generation, same external id");
    assert_eq!(slot.status, SlotStatus::Pending);
    assert_eq!(slot.fp, fp("b"));
    assert_eq!(slot.issue_date_requested, Some(date(2026, 9, 3)));
    assert_eq!(slot.attempts, 0);
    assert_eq!(slot.last_attempt_at, None);
    assert_eq!(
        ledger.lookup_request(&rid("r-1")),
        Some(&RequestRef::Abandoned)
    );
    assert_eq!(
        ledger.lookup_request(&rid("r-2")),
        Some(&RequestRef::Slot {
            kind: DocumentKind::Invoice,
            generation: 0
        })
    );
    let event = ledger.history().last().expect("event");
    assert_eq!(event.event, HistoryKind::Abandoned);
    assert_eq!(event.request_id, Some(rid("r-1")));
    assert_eq!(event.generation, 0);

    assert_eq!(
        ledger.take_over(DocumentKind::Invoice, rid("r-1"), fp("c"), None),
        Err(LedgerError::RequestIdKnown(rid("r-1"))),
        "an abandoned id cannot come back"
    );
    ledger.commit(&invoice, committed("SZ-1")).expect("commit");
    assert_eq!(
        ledger.take_over(DocumentKind::Invoice, rid("r-3"), fp("c"), None),
        Err(LedgerError::InvalidTransition {
            target: invoice,
            from: "committed".to_owned(),
            action: "take over",
        })
    );
    assert_eq!(
        ledger.take_over(DocumentKind::Final, rid("r-3"), fp("c"), None),
        Err(LedgerError::MissingTarget(Target::Slot(
            DocumentKind::Final
        )))
    );
}

#[test]
fn forget_vacates_with_a_bumped_generation() {
    let mut ledger = with_invoice();
    assert_eq!(ledger.forget(DocumentKind::Invoice), Ok(1));
    let slot = ledger.slot(DocumentKind::Invoice).expect("slot");
    assert_eq!(slot.status, SlotStatus::Vacant);
    assert_eq!(slot.generation, 1);
    let event = ledger.history().last().expect("event");
    assert_eq!(event.event, HistoryKind::Forgotten);
    assert_eq!(event.generation, 0);
    assert_eq!(event.number.as_deref(), Some("SZ-1"));
    assert_eq!(
        ledger.allocate_intent(DocumentKind::Invoice, rid("r-2"), fp("b"), None),
        Ok(1)
    );
    assert_eq!(
        ledger.forget(DocumentKind::Invoice),
        Err(LedgerError::InvalidTransition {
            target: Target::Slot(DocumentKind::Invoice),
            from: "pending".to_owned(),
            action: "forget",
        })
    );
    assert_eq!(
        ledger.forget(DocumentKind::Final),
        Err(LedgerError::MissingTarget(Target::Slot(
            DocumentKind::Final
        )))
    );

    let mut blocked = Ledger::new();
    blocked
        .allocate_intent(DocumentKind::Invoice, rid("r-1"), fp("a"), None)
        .expect("allocate");
    blocked
        .mark_blocked(DocumentKind::Invoice, None)
        .expect("block");
    assert_eq!(blocked.forget(DocumentKind::Invoice), Ok(1));
}

#[test]
fn retry_with_forgotten_request_id_is_a_conflict() {
    // After `forget` the request id still points at the generation it
    // allocated, which the slot has moved past: the ledger refuses to
    // re-allocate under it and the service answers from the history.
    let mut ledger = with_invoice();
    assert_eq!(ledger.forget(DocumentKind::Invoice), Ok(1));
    assert_eq!(
        ledger.lookup_request(&rid("r-1")),
        Some(&RequestRef::Slot {
            kind: DocumentKind::Invoice,
            generation: 0
        }),
        "the forgotten id keeps pointing at its closed generation"
    );
    let slot = ledger.slot(DocumentKind::Invoice).expect("slot");
    assert_eq!(slot.request_id, rid("r-1"));
    assert_eq!(slot.status, SlotStatus::Vacant);
    assert_ne!(
        slot.generation, 0,
        "the slot's generation differs from the one the id owns"
    );
    assert_eq!(
        ledger.allocate_intent(DocumentKind::Invoice, rid("r-1"), fp("a"), None),
        Err(LedgerError::RequestIdKnown(rid("r-1"))),
        "a forgotten id never re-allocates"
    );
    let forgotten = ledger
        .history()
        .iter()
        .rev()
        .find(|event| {
            event.kind == IssuedKind::Invoice
                && event.generation == 0
                && event.event == HistoryKind::Forgotten
        })
        .expect("the forgotten event the service answers from");
    assert_eq!(forgotten.request_id, Some(rid("r-1")));
    assert_eq!(forgotten.number.as_deref(), Some("SZ-1"));
}

#[test]
fn correctives_have_their_own_sequence() {
    let mut ledger = with_invoice();
    let cseq = ledger
        .allocate_corrective(rid("c-1"), "SZ-1", fp("c"))
        .expect("allocate");
    assert_eq!(cseq, 1);
    assert_eq!(ledger.next_cseq(), 2);
    assert_eq!(
        ledger.lookup_request(&rid("c-1")),
        Some(&RequestRef::Corrective { cseq: 1 })
    );
    let corrective = Target::Corrective(rid("c-1"));
    ledger.record_attempt(&corrective, now()).expect("attempt");
    ledger
        .commit(
            &corrective,
            CommittedDocument::issued("HS-1").with_totals(Some(dec!(-1270)), Some(dec!(-1000))),
        )
        .expect("commit");
    let entry = ledger.corrective(&rid("c-1")).expect("entry");
    assert_eq!(entry.status, SlotStatus::Committed);
    assert_eq!(entry.number.as_deref(), Some("HS-1"));
    assert_eq!(entry.gross, Some(dec!(-1270)));
    assert_eq!(entry.attempts, 1);
    assert_eq!(entry.corrected_number, "SZ-1");
    let event = ledger.history().last().expect("event");
    assert_eq!(event.kind, IssuedKind::Corrective);
    assert_eq!(event.generation, 1, "cseq stands in for the generation");
    assert_eq!(
        ledger.find_by_number("HS-1"),
        Some(Target::Corrective(rid("c-1")))
    );

    let second = ledger
        .allocate_corrective(rid("c-2"), "SZ-1", fp("d"))
        .expect("allocate");
    assert_eq!(second, 2);
    ledger
        .mark_rejected(&Target::Corrective(rid("c-2")), "259", "net")
        .expect("reject");
    assert_eq!(
        ledger
            .correctives()
            .iter()
            .map(|e| e.cseq)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );

    // Reversing a corrective does not bump anything.
    assert_eq!(
        ledger.mark_reversed(
            &corrective,
            Reversal::new(ReversalOrigin::External).with_by("SS-2")
        ),
        Ok(1)
    );
    assert_eq!(ledger.corrective(&rid("c-1")).expect("entry").cseq, 1);
    assert_eq!(
        ledger.record_attempt(&Target::Corrective(rid("c-9")), now()),
        Err(LedgerError::MissingTarget(Target::Corrective(rid("c-9"))))
    );
}

#[test]
fn our_numbers_covers_every_source() {
    let mut ledger = with_invoice();
    ledger
        .mark_reversed(
            &Target::Slot(DocumentKind::Invoice),
            Reversal::new(ReversalOrigin::Service).with_by("SS-1"),
        )
        .expect("reverse");
    ledger
        .allocate_intent(DocumentKind::Invoice, rid("r-2"), fp("b"), None)
        .expect("allocate");
    ledger
        .commit(&Target::Slot(DocumentKind::Invoice), committed("SZ-2"))
        .expect("commit");
    ledger
        .allocate_corrective(rid("c-1"), "SZ-2", fp("c"))
        .expect("allocate");
    ledger
        .commit(
            &Target::Corrective(rid("c-1")),
            CommittedDocument::issued("HS-1"),
        )
        .expect("commit");
    ledger
        .allocate_intent(DocumentKind::Proforma, rid("p-1"), fp("p"), None)
        .expect("allocate");
    ledger
        .commit(&Target::Slot(DocumentKind::Proforma), committed("D-1"))
        .expect("commit");
    ledger.mark_consumed("SZ-2").expect("consume");
    ledger.set_foreign_hint("SZ-999", "SZ");

    let mut numbers = ledger.our_numbers();
    numbers.sort();
    assert_eq!(numbers, vec!["D-1", "HS-1", "SS-1", "SZ-1", "SZ-2"]);
    assert_eq!(
        ledger.foreign_hint(),
        Some(&ForeignHint::new("SZ-999", "SZ"))
    );
}

#[test]
fn supplier_id_is_learned_once() {
    let mut ledger = Ledger::new();
    ledger.learn_supplier_id(972_720).expect("learn");
    ledger.learn_supplier_id(972_720).expect("same again");
    assert_eq!(ledger.supplier_id(), Some(972_720));
    assert_eq!(
        ledger.learn_supplier_id(1),
        Err(LedgerError::SupplierMismatch {
            recorded: 972_720,
            seen: 1,
        })
    );
    assert_eq!(ledger.supplier_id(), Some(972_720));
}

#[test]
fn history_is_capped() {
    let mut ledger = Ledger::new();
    for i in 0..(Ledger::HISTORY_CAP + 10) {
        let id = rid(&format!("r-{i}"));
        ledger
            .allocate_intent(DocumentKind::Invoice, id, fp("a"), None)
            .expect("allocate");
        ledger
            .commit(
                &Target::Slot(DocumentKind::Invoice),
                CommittedDocument::issued(format!("SZ-{i}")),
            )
            .expect("commit");
        ledger
            .mark_reversed(
                &Target::Slot(DocumentKind::Invoice),
                Reversal::new(ReversalOrigin::Service),
            )
            .expect("reverse");
    }
    assert_eq!(ledger.history().len(), Ledger::HISTORY_CAP);
    let last = ledger.history().last().expect("event");
    assert_eq!(last.event, HistoryKind::Reversed);
    assert_eq!(last.generation as usize, Ledger::HISTORY_CAP + 9);
    assert_eq!(
        ledger.history()[0].generation as usize,
        (Ledger::HISTORY_CAP + 10) - Ledger::HISTORY_CAP / 2,
        "the oldest events were dropped"
    );
}

#[test]
fn snapshot_projects_the_ledger() {
    let mut ledger = with_invoice();
    ledger.learn_supplier_id(972_720).expect("learn");
    ledger
        .mark_reversed(
            &Target::Slot(DocumentKind::Invoice),
            Reversal::new(ReversalOrigin::External).with_by("SS-1"),
        )
        .expect("reverse");
    ledger
        .allocate_intent(DocumentKind::Final, rid("f-1"), fp("f"), None)
        .expect("allocate");
    ledger
        .allocate_corrective(rid("c-1"), "SZ-1", fp("c"))
        .expect("allocate");
    ledger.set_foreign_hint("SZ-9", "SZ");

    let snapshot = ledger.snapshot(Freshness::Live);
    assert_eq!(snapshot.freshness, Freshness::Live);
    assert_eq!(snapshot.supplier_id, Some(972_720));
    let invoice = snapshot.slots.invoice.as_ref().expect("invoice");
    assert_eq!(invoice.generation, 1);
    assert_eq!(invoice.status, "reversed");
    assert_eq!(invoice.number.as_deref(), Some("SZ-1"));
    assert_eq!(invoice.gross, Some(dec!(1270)));
    assert_eq!(invoice.origin.as_deref(), Some("external"));
    assert_eq!(invoice.attempts, 1);
    let r#final = snapshot.slots.r#final.as_ref().expect("final");
    assert_eq!(r#final.status, "pending");
    assert_eq!(r#final.origin, None);
    assert!(snapshot.slots.proforma.is_none());
    assert_eq!(snapshot.correctives.len(), 1);
    assert_eq!(snapshot.correctives[0].cseq, 1);
    assert_eq!(snapshot.correctives[0].status, "pending");
    assert_eq!(snapshot.correctives[0].corrected_number, "SZ-1");
    assert_eq!(snapshot.foreign_hint, Some(ForeignHint::new("SZ-9", "SZ")));
    assert_eq!(snapshot.history.len(), 2);
    assert_eq!(snapshot.history[0].event, "issued");
    assert_eq!(snapshot.history[1].event, "reversed");
    assert_eq!(snapshot.history[1].by.as_deref(), Some("SS-1"));
    assert_eq!(snapshot.history[1].origin.as_deref(), Some("external"));

    let mut committed_origin = with_invoice();
    committed_origin
        .allocate_intent(DocumentKind::Proforma, rid("p-1"), fp("p"), None)
        .expect("allocate");
    committed_origin
        .commit(
            &Target::Slot(DocumentKind::Proforma),
            CommittedDocument::reconciled("D-1", Origin::Adopted),
        )
        .expect("commit");
    let snapshot = committed_origin.snapshot(Freshness::Snapshot);
    assert_eq!(
        snapshot
            .slots
            .invoice
            .as_ref()
            .expect("invoice")
            .origin
            .as_deref(),
        Some("service")
    );
    assert_eq!(
        snapshot
            .slots
            .proforma
            .as_ref()
            .expect("proforma")
            .origin
            .as_deref(),
        Some("adopted")
    );
    let empty = Ledger::new().snapshot(Freshness::Snapshot);
    assert_eq!(empty, OrderSnapshot::new(Freshness::Snapshot));
}

#[test]
fn serde_round_trip_of_a_populated_ledger() {
    let mut ledger = with_invoice();
    ledger.learn_supplier_id(972_720).expect("learn");
    ledger
        .mark_reversed(
            &Target::Slot(DocumentKind::Invoice),
            Reversal::new(ReversalOrigin::External)
                .with_by("SS-1")
                .with_payments_before(vec![dec!(100)]),
        )
        .expect("reverse");
    ledger
        .allocate_intent(
            DocumentKind::Invoice,
            rid("r-2"),
            fp("b"),
            Some(date(2026, 9, 3)),
        )
        .expect("allocate");
    ledger
        .record_attempt(&Target::Slot(DocumentKind::Invoice), now())
        .expect("attempt");
    ledger
        .allocate_intent(DocumentKind::Proforma, rid("p-1"), fp("p"), None)
        .expect("allocate");
    ledger
        .commit(&Target::Slot(DocumentKind::Proforma), committed("D-1"))
        .expect("commit");
    ledger.mark_consumed("SZ-1").expect("consume");
    ledger
        .allocate_corrective(rid("c-1"), "SZ-1", fp("c"))
        .expect("allocate");
    ledger
        .mark_rejected(&Target::Corrective(rid("c-1")), "221", "has corrective")
        .expect("reject");
    ledger
        .allocate_intent(DocumentKind::Final, rid("f-1"), fp("f"), None)
        .expect("allocate");
    ledger
        .mark_blocked(DocumentKind::Final, Some("VS-2".to_owned()))
        .expect("block");
    ledger.set_foreign_hint("SZ-9", "SZ");

    let json = serde_json::to_value(&ledger).expect("serialize");
    assert_eq!(json["v"], 1);
    assert_eq!(json["supplier_id"], 972_720);
    assert_eq!(json["slots"]["invoice"]["gen"], 1);
    assert_eq!(json["slots"]["invoice"]["status"], "pending");
    assert_eq!(
        json["slots"]["invoice"]["issue_date_requested"],
        "2026-09-03"
    );
    assert_eq!(
        json["slots"]["proforma"]["status"],
        json!({"consumed": {"by": "SZ-1"}})
    );
    assert_eq!(
        json["slots"]["final"]["status"],
        json!({"blocked": {"existing_number": "VS-2"}})
    );
    assert_eq!(
        json["requests"]["r-1"],
        json!({"slot": {"kind": "invoice", "gen": 0}})
    );
    assert_eq!(json["requests"]["c-1"], json!({"corrective": {"cseq": 1}}));
    assert_eq!(json["correctives"]["c-1"]["cseq"], 1);
    assert_eq!(json["next_cseq"], 2);
    assert_eq!(json["foreign_hint"]["number"], "SZ-9");
    assert_eq!(json["history"][1]["event"], "reversed");
    assert_eq!(json["history"][1]["payments_before"], json!(["100"]));
    assert_eq!(json["history"][0].get("payments_before"), None);
    assert!(json["slots"]["invoice"]["fp"].is_string());
    let text = serde_json::to_string(&json).expect("string");
    assert!(!text.contains("Kov"), "no buyer data in the ledger");

    let back: Ledger = serde_json::from_value(json).expect("deserialize");
    assert_eq!(back, ledger);
}

#[test]
fn empty_documents_deserialize_to_defaults() {
    for text in ["{}", r#"{"v":1}"#, r#"{"v":1,"slots":{}}"#] {
        let ledger: Ledger = serde_json::from_str(text).expect(text);
        assert_eq!(ledger, Ledger::new(), "{text}");
    }
    let partial: Ledger = serde_json::from_value(json!({
        "slots": {"invoice": {
            "gen": 2,
            "request_id": "r-1",
            "status": "committed",
            "number": "SZ-1",
            "fp": "00",
        }},
        "unknown_future_field": true,
    }))
    .expect("partial slot");
    let slot = partial.slot(DocumentKind::Invoice).expect("slot");
    assert_eq!(slot.generation, 2);
    assert_eq!(slot.attempts, 0);
    assert_eq!(slot.origin, Origin::Service);
    assert_eq!(slot.gross, None);
}

#[test]
fn status_tokens_and_display() {
    assert_eq!(SlotStatus::Pending.token(), "pending");
    assert_eq!(
        SlotStatus::ReversalUnverified.to_string(),
        "reversal_unverified"
    );
    assert_eq!(Target::Slot(DocumentKind::Final).to_string(), "final slot");
    assert_eq!(Target::Corrective(rid("c-1")).to_string(), "corrective c-1");
    assert_eq!(
        Target::from(DocumentKind::Invoice),
        Target::Slot(DocumentKind::Invoice)
    );
    assert_eq!(
        LedgerError::SlotBusy {
            kind: DocumentKind::Invoice,
            status: "pending".to_owned(),
        }
        .to_string(),
        "the invoice slot is pending and cannot be allocated"
    );
    assert_eq!(
        HistoryKind::RecordedByOperator.token(),
        "recorded_by_operator"
    );
    assert_eq!(Origin::Adopted.token(), "adopted");
    assert_eq!(ReversalOrigin::Operator.token(), "operator");
}
