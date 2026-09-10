//! The synthetic fixtures every test file shares, each literal in one place
//! (`tests/synthetic/`), and the projections the tests read them through.
//!
//! Included by path (`mod common;`) from each integration test binary, so
//! whatever one binary does not use is dead code there.

#![allow(dead_code)]

#[cfg(feature = "axum")]
pub mod axum;

use szamlazz_adatkapcsolat::{
    BankTransaction, Document, InvoiceDocument, ParseError, ReceiptBatch,
};

/// An outgoing invoice (`<szamla>`): id 123456, number 2015-123, issued
/// 2015-12-01, one line item, one credit entry, an empty `<pdf>`.
pub const OUTGOING_INVOICE: &str = include_str!("../synthetic/szamla.xml");

/// A bank transaction (`<banktranz>`): id 987, value date 2026-07-03, an
/// incoming 12700.0 HUF with a partner and a memo.
pub const BANK_TRANSACTION: &str = include_str!("../synthetic/banktranz.xml");

/// A receipt batch (`<xmlnyugtaarchiv>`) of two receipts issued 2026-07-03:
/// `NYGTA-2026-1` (id 1: a decimal quantity of 2.0 at 10000, gross 25400.0,
/// with ledger data, a special VAT category and a payment) and
/// `NYGTA-2026-2` (id 2: plain, one item at 100, gross 127).
pub const RECEIPT_BATCH: &str = include_str!("../synthetic/xmlnyugtaarchiv.xml");

/// The outgoing invoice fixture rewritten as an incoming one (`<szamlabe>`
/// in its own namespace); the two wire shapes are otherwise the same.
pub fn incoming_invoice() -> String {
    as_incoming(OUTGOING_INVOICE)
}

/// Rewrites an outgoing invoice body as an incoming one.
pub fn as_incoming(outgoing: &str) -> String {
    outgoing
        .replace(
            "http://www.szamlazz.hu/szamla",
            "http://www.szamlazz.hu/szamlabe",
        )
        .replace("<szamla xmlns=", "<szamlabe xmlns=")
        .replace("</szamla>", "</szamlabe>")
}

/// `fixture` with its first `element` replaced by `replacement`, asserting the
/// fixture still carries the element: a test whose substitution finds
/// nothing would test the unmodified fixture and prove nothing.
pub fn with(fixture: &str, element: &str, replacement: &str) -> String {
    assert!(
        fixture.contains(element),
        "fixture drifted: {element} is not in it"
    );
    fixture.replacen(element, replacement, 1)
}

/// `fixture` with its first `element` removed; see [`with`].
pub fn without(fixture: &str, element: &str) -> String {
    with(fixture, element, "")
}

/// Parses `body` as an outgoing invoice; any other kind is a test bug.
pub fn outgoing(body: &str) -> Result<InvoiceDocument, ParseError> {
    match Document::parse(body.as_bytes())? {
        Document::OutgoingInvoice(invoice) => Ok(invoice),
        other => panic!("expected outgoing invoice, got {other:?}"),
    }
}

/// Parses `body` as an incoming invoice; any other kind is a test bug.
pub fn incoming(body: &str) -> Result<InvoiceDocument, ParseError> {
    match Document::parse(body.as_bytes())? {
        Document::IncomingInvoice(invoice) => Ok(invoice),
        other => panic!("expected incoming invoice, got {other:?}"),
    }
}

/// Parses `body` as a bank transaction; any other kind is a test bug.
pub fn bank_transaction(body: &str) -> Result<BankTransaction, ParseError> {
    match Document::parse(body.as_bytes())? {
        Document::BankTransaction(transaction) => Ok(transaction),
        other => panic!("expected bank transaction, got {other:?}"),
    }
}

/// Parses `body` as a receipt batch; any other kind is a test bug.
pub fn receipts(body: &str) -> Result<ReceiptBatch, ParseError> {
    match Document::parse(body.as_bytes())? {
        Document::Receipts(batch) => Ok(batch),
        other => panic!("expected receipts, got {other:?}"),
    }
}
