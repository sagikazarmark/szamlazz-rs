//! Fan-out handler tests: delivery to all members, failure aggregation, ack
//! merging.

use std::future::ready;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use szamlazz_adatkapcsolat::{
    Ack, Document, Fanout, Handler as _, InvoiceAck, InvoiceDirection, InvoiceDocument, MaybeSend,
    ReceiptBatch,
};

const OUTGOING_INVOICE: &[u8] = include_bytes!("synthetic/szamla.xml");

fn invoice() -> InvoiceDocument {
    match Document::parse(OUTGOING_INVOICE).expect("parse") {
        Document::OutgoingInvoice(invoice) => invoice,
        other => panic!("expected outgoing invoice, got {other:?}"),
    }
}

/// A minimal valid receipt batch, built the same way a receiver gets one.
fn receipt_batch() -> ReceiptBatch {
    let body = br#"<xmlnyugtaarchiv xmlns="http://www.szamlazz.hu/xmlnyugtaarchiv"><nyugta><alap><id>1</id><nyugtaszam>N-1</nyugtaszam><tipus>NY</tipus><stornozott>false</stornozott><kelt>2026-01-01</kelt><fizmod>cash</fizmod><penznem>HUF</penznem><teszt>false</teszt><adoszam>12345678-1-42</adoszam></alap><tetelek><tetel><megnevezes>Item</megnevezes><nettoEgysegar>1</nettoEgysegar><mennyiseg>1</mennyiseg><mennyisegiEgyseg>db</mennyisegiEgyseg><netto>1</netto><afakulcs>27</afakulcs><afa>0.27</afa><brutto>1.27</brutto></tetel></tetelek><osszegek><afakulcsossz><afakulcs>27</afakulcs><netto>1</netto><afa>0.27</afa><brutto>1.27</brutto></afakulcsossz><totalossz><netto>1</netto><afa>0.27</afa><brutto>1.27</brutto></totalossz></osszegek></nyugta></xmlnyugtaarchiv>"#;
    match Document::parse(body).expect("parse") {
        Document::Receipts(batch) => batch,
        other => panic!("expected receipt batch, got {other:?}"),
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
    type Error = String;

    fn outgoing_invoice(
        &self,
        invoice: InvoiceDocument,
    ) -> impl Future<Output = Result<InvoiceAck, String>> + MaybeSend {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return ready(Err("probe failed".to_owned()));
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

    async fn incoming_invoice(&self, invoice: InvoiceDocument) -> Result<InvoiceAck, String> {
        self.outgoing_invoice(invoice).await
    }

    fn bank_transaction(
        &self,
        _tx: szamlazz_adatkapcsolat::BankTransaction,
    ) -> impl Future<Output = Result<Ack, String>> + MaybeSend {
        self.calls.fetch_add(1, Ordering::SeqCst);
        ready(Ok(Ack::accept()))
    }

    fn receipts(
        &self,
        _batch: ReceiptBatch,
    ) -> impl Future<Output = Result<Ack, String>> + MaybeSend {
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
    // …and the report names the failing one.
    assert_eq!(error.failures.len(), 1);
    assert!(error.failures[0].handler.contains("Probe"));
    assert!(error.to_string().contains("probe failed"));
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
