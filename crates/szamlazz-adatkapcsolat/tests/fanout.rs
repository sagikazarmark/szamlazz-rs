//! Fan-out handler tests: delivery to all members, failure aggregation, ack
//! merging.

mod common;

use std::future::ready;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::{OUTGOING_INVOICE, RECEIPT_BATCH};
use szamlazz_adatkapcsolat::{
    Ack, Fanout, Handler as _, InvoiceAck, InvoiceDirection, InvoiceDocument, MaybeSend,
    ReceiptBatch,
};

fn invoice() -> InvoiceDocument {
    common::outgoing(OUTGOING_INVOICE).expect("parse")
}

fn receipt_batch() -> ReceiptBatch {
    common::receipts(RECEIPT_BATCH).expect("parse")
}

/// A member's failure with a cause, so the fan-out's report can be checked
/// for the `source()` chain.
#[derive(Debug)]
struct ProbeError {
    cause: std::io::Error,
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("probe failed")
    }
}

impl std::error::Error for ProbeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

fn probe_error() -> ProbeError {
    ProbeError {
        cause: std::io::Error::new(std::io::ErrorKind::TimedOut, "database down"),
    }
}

/// Counts deliveries; optionally fails, tags a registration number, or
/// answers a control code.
#[derive(Clone, Default)]
struct Probe {
    calls: Arc<AtomicUsize>,
    fail: bool,
    registration: Option<&'static str>,
    disconnect: bool,
}

impl szamlazz_adatkapcsolat::Handler for Probe {
    type Error = ProbeError;

    fn outgoing_invoice(
        &self,
        invoice: InvoiceDocument,
    ) -> impl Future<Output = Result<InvoiceAck, ProbeError>> + MaybeSend {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return ready(Err(probe_error()));
        }
        if self.disconnect {
            return ready(Ok(InvoiceAck::disconnect()));
        }
        let ack = InvoiceAck::accept(invoice.info.id);
        ready(Ok(match self.registration {
            Some(registration) => ack.with_registration_number(registration),
            None => ack,
        }))
    }

    async fn incoming_invoice(&self, invoice: InvoiceDocument) -> Result<InvoiceAck, ProbeError> {
        self.outgoing_invoice(invoice).await
    }

    fn bank_transaction(
        &self,
        _tx: szamlazz_adatkapcsolat::BankTransaction,
    ) -> impl Future<Output = Result<Ack, ProbeError>> + MaybeSend {
        self.calls.fetch_add(1, Ordering::SeqCst);
        ready(Ok(Ack::accept()))
    }

    fn receipts(
        &self,
        _batch: ReceiptBatch,
    ) -> impl Future<Output = Result<Ack, ProbeError>> + MaybeSend {
        self.calls.fetch_add(1, Ordering::SeqCst);
        ready(Ok(Ack::accept()))
    }
}

fn xml(ack: &InvoiceAck) -> String {
    String::from_utf8(ack.to_xml(InvoiceDirection::Outgoing).expect("valid Ack")).expect("utf-8")
}

#[tokio::test]
async fn delivers_to_all_handlers_in_order() {
    let first = Probe::default();
    let second = Probe::default();
    let fanout = Fanout::new().with(first.clone()).with(second.clone());

    let ack = fanout.outgoing_invoice(invoice()).await.expect("ack");
    assert_eq!(first.calls.load(Ordering::SeqCst), 1);
    assert_eq!(second.calls.load(Ordering::SeqCst), 1);
    assert!(xml(&ack).contains("<id>123456</id>"));
}

#[tokio::test]
async fn failure_does_not_stop_other_handlers() {
    let failing = Probe {
        fail: true,
        ..Probe::default()
    };
    let healthy = Probe::default();
    let fanout = Fanout::new().with(failing.clone()).with(healthy.clone());

    let error = fanout.outgoing_invoice(invoice()).await.expect_err("error");
    // The healthy handler still ran…
    assert_eq!(healthy.calls.load(Ordering::SeqCst), 1);
    // …and the report names the failing one, with its error kept whole:
    // the member's own type, and its cause behind it.
    assert_eq!(error.failures.len(), 1);
    assert!(error.failures[0].handler.contains("Probe"));
    assert!(error.to_string().contains("probe failed"));
    let failure = &error.failures[0].error;
    assert!(failure.is::<ProbeError>());
    let cause = std::error::Error::source(failure.as_ref()).expect("the cause is kept");
    assert_eq!(cause.to_string(), "database down");
}

#[tokio::test]
async fn merges_first_registration_number() {
    let plain = Probe::default();
    let registering = Probe {
        registration: Some("IKT-42"),
        ..Probe::default()
    };
    let fanout = Fanout::new().with(plain).with(registering);

    let ack = fanout.outgoing_invoice(invoice()).await.expect("ack");
    let xml = xml(&ack);
    assert!(xml.contains("<id>123456</id>"));
    assert!(xml.contains("<iktatoszam>IKT-42</iktatoszam>"));
}

#[tokio::test]
async fn control_codes_escalate() {
    let plain = Probe::default();
    let disconnecting = Probe {
        disconnect: true,
        ..Probe::default()
    };
    let fanout = Fanout::new().with(plain).with(disconnecting);

    let ack = fanout.outgoing_invoice(invoice()).await.expect("ack");
    let xml = xml(&ack);
    assert!(xml.contains("<hibakod>KEY_DEL</hibakod>"));
    assert!(!xml.contains("<alap>"));
}

#[tokio::test]
async fn empty_fanout_rejects_delivery() {
    let error = Fanout::new()
        .outgoing_invoice(invoice())
        .await
        .expect_err("error");
    assert!(error.to_string().contains("no handlers configured"));
}

#[tokio::test]
async fn explicitly_implemented_methods_participate() {
    let probe = Probe::default();
    let fanout = Fanout::new().with(probe.clone());
    let ack = fanout.receipts(receipt_batch()).await.expect("ack");
    assert!(
        !String::from_utf8(ack.to_receipts_xml())
            .expect("utf-8")
            .contains("hibakod")
    );
}
