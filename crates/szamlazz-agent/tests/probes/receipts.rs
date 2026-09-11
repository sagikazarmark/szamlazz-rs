//! Separately selected receipt experiments, with known-number cleanup.
use super::live_support::Run;
use futures_util::FutureExt;
use rust_decimal::dec;
use std::any::Any;
use std::panic::AssertUnwindSafe;
use szamlazz_agent::ops::receipt::{
    CreateReceipt, QueryReceipt, Receipt, ReceiptEmail, ReceiptPayment, ReceiptSelector,
    SendReceipt, StornoReceipt,
};
use szamlazz_agent::{
    ClientError, Currency, ErrorCode, ExchangeRate, LineItem, PaymentMethod, Pdf, ReceiptNumber,
    ReceiptType, VatRate,
};

fn assert_pdf(pdf: Option<&Pdf>) {
    assert!(
        pdf.expect("requested PDF").as_bytes().starts_with(b"%PDF-"),
        "artifact must have a PDF signature (not a full rendering check)"
    );
}

fn setting(name: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| panic!("selected receipt probe requires {name}"))
}

struct ReceiptRun {
    run: Run,
    prefix: String,
    known: Vec<ReceiptNumber>,
}

impl ReceiptRun {
    fn new() -> Self {
        let prefix = setting("SZAMLAZZ_RECEIPT_PREFIX");
        Self {
            run: Run::new(),
            prefix,
            known: Vec::new(),
        }
    }

    fn request(&self, currency: Currency) -> CreateReceipt {
        let mut request = CreateReceipt::new(
            &self.prefix,
            PaymentMethod::Cash,
            currency,
            vec![LineItem::new(
                "Receipt probe",
                dec!(1),
                "db",
                dec!(787.40),
                VatRate::percent(27),
                dec!(787.40),
                dec!(212.60),
                dec!(1000),
            )],
        );
        // The call id and order are printed before the first send and retained
        // in nextest output. Never regenerate them to retry an unresolved run.
        request.call_id = Some(self.run.order.clone());
        request.order_number = Some(self.run.order.clone());
        request.download_pdf = true;
        request.payments = vec![ReceiptPayment::new("készpénz", dec!(1000))];
        request
    }

    fn record(&mut self, receipt: &Receipt) {
        eprintln!(
            "PROBE receipt={} id={} call_id={:?} order={:?}",
            receipt.receipt_number, receipt.id, receipt.call_id, receipt.order_number
        );
        if !self.known.contains(&receipt.receipt_number) {
            self.known.push(receipt.receipt_number.clone());
        }
    }

    async fn create(&mut self, request: &CreateReceipt) -> Receipt {
        self.run.sending(format!(
            "receipt create call_id={:?} order={:?} currency={}",
            request.call_id, request.order_number, request.currency
        ));
        let sent = self.run.client.send(request).await;
        let receipt = self.run.answered(sent);
        self.record(&receipt);
        self.run.unresolved = None;
        assert_eq!(receipt.test, Some(true));
        assert_eq!(receipt.document_type, ReceiptType::Receipt);
        assert_eq!(receipt.call_id, request.call_id);
        assert_eq!(receipt.order_number, request.order_number);
        assert!(!receipt.reversed);
        receipt
    }

    async fn query(&self, selector: ReceiptSelector) -> Receipt {
        let request = QueryReceipt {
            download_pdf: true,
            ..QueryReceipt::new(selector)
        };
        self.run.client.send(&request).await.expect("receipt query")
    }

    async fn by_number(&self, number: &ReceiptNumber) -> Receipt {
        let receipt = self
            .query(ReceiptSelector::ReceiptNumber(number.clone()))
            .await;
        assert_eq!(&receipt.receipt_number, number);
        assert_eq!(receipt.test, Some(true));
        receipt
    }

    async fn reverse(&mut self, number: &ReceiptNumber) -> Receipt {
        let original = self.by_number(number).await;
        assert_eq!(original.document_type, ReceiptType::Receipt);
        assert_eq!(
            original.order_number.as_deref(),
            Some(self.run.order.as_str())
        );
        assert!(!original.reversed);
        let request = StornoReceipt {
            call_id: Some(uuid::Uuid::new_v4().to_string()),
            download_pdf: true,
            ..StornoReceipt::new(number.clone())
        };
        self.run.sending(format!(
            "receipt storno original={number} call_id={:?}",
            request.call_id
        ));
        let sent = self.run.client.send(&request).await;
        let reply = self.run.answered(sent);
        eprintln!(
            "PROBE receipt reversal={} original={number}",
            reply.receipt_number
        );
        // Leave uncertainty set until both the reversal and original read back.
        let reversal = self.by_number(&reply.receipt_number).await;
        assert_eq!(reversal.document_type, ReceiptType::Storno);
        assert_ne!(&reversal.receipt_number, number);
        assert_eq!(reversal.reversed_receipt_number.as_ref(), Some(number));
        assert!(self.by_number(number).await.reversed);
        self.run.unresolved = None;
        assert_pdf(reply.pdf.as_ref());
        assert_pdf(reversal.pdf.as_ref());
        reversal
    }

    async fn finish(mut self, result: Result<(), Box<dyn Any + Send>>) {
        // Run also owns the unresolved-write diagnostic. Receipt cleanup runs
        // only after settled writes and stops on its first failure.
        let cleanup = AssertUnwindSafe(async {
            if self.run.unresolved.is_none() {
                while let Some(number) = self.known.pop() {
                    let receipt = self.by_number(&number).await;
                    if !receipt.reversed {
                        self.reverse(&number).await;
                    }
                }
            }
        })
        .catch_unwind()
        .await;
        if cleanup.is_err() {
            eprintln!(
                "PROBE receipt cleanup failed; use printed numbers and call ids to reconcile"
            );
        }
        // Preserve the scenario failure; cleanup failures still fail an
        // otherwise successful scenario. Run prints unresolved intent first.
        self.run.finish(result.and(cleanup)).await;
    }
}

#[tokio::test]
#[ignore = "requires test-mode SZAMLAZZ_AGENT_KEY and SZAMLAZZ_RECEIPT_PREFIX; creates/reverses receipts"]
async fn receipt_lifecycle() {
    let mut run = ReceiptRun::new();
    let result = AssertUnwindSafe(async {
        let request = run.request(Currency::HUF);
        let receipt = run.create(&request).await;
        assert_pdf(receipt.pdf.as_ref());
        let by_number = run.by_number(&receipt.receipt_number).await;
        let by_order = run
            .query(ReceiptSelector::OrderNumber(run.run.order.clone()))
            .await;
        for stored in [&by_number, &by_order] {
            assert_eq!(stored.receipt_number, receipt.receipt_number);
            assert_eq!(stored.id, receipt.id);
            assert_eq!(stored.call_id, request.call_id);
            assert_eq!(stored.order_number, request.order_number);
            assert_eq!(stored.document_type, ReceiptType::Receipt);
            assert_eq!(stored.test, Some(true));
            assert!(!stored.reversed);
            assert_eq!(stored.currency, Currency::HUF);
            assert_eq!(stored.totals.total.net, dec!(787.40));
            assert_eq!(stored.totals.total.vat, dec!(212.60));
            assert_eq!(stored.totals.total.gross, dec!(1000));
            assert_eq!(
                stored
                    .payments
                    .iter()
                    .map(|p| p.amount)
                    .sum::<rust_decimal::Decimal>(),
                dec!(1000)
            );
            assert_pdf(stored.pdf.as_ref());
        }

        // Deliberate duplicate of a completed, verified create only. Never a
        // recovery resend of a timed-out creation.
        run.run.sending(format!(
            "duplicate completed receipt call_id={:?}",
            request.call_id
        ));
        match run.run.client.send(&request).await {
            Err(ClientError::Api(api)) if api.code == ErrorCode::DuplicateReceiptCallId => {
                run.run.unresolved = None;
                eprintln!("PROBE receipt duplicate code={}", api.code);
            }
            Ok(other) => {
                run.record(&other);
                run.run.unresolved = None;
                panic!("duplicate call id unexpectedly returned a receipt");
            }
            Err(error) => {
                run.run.answered::<Receipt>(Err(error));
            }
        }
        let after_duplicate = run
            .query(ReceiptSelector::OrderNumber(run.run.order.clone()))
            .await;
        assert_eq!(after_duplicate.receipt_number, receipt.receipt_number);
        assert_eq!(after_duplicate.id, receipt.id);
        run.reverse(&receipt.receipt_number).await;
    })
    .catch_unwind()
    .await;
    run.finish(result).await;
}

#[tokio::test]
#[ignore = "requires test-mode SZAMLAZZ_AGENT_KEY, SZAMLAZZ_RECEIPT_PREFIX and EUR; probes automatic MNB"]
async fn receipt_automatic_mnb() {
    let mut run = ReceiptRun::new();
    let result = AssertUnwindSafe(async {
        let mut request = run.request(Currency::EUR);
        request.exchange_rate = Some(ExchangeRate::automatic_mnb());
        let receipt = run.create(&request).await;
        let stored = run.by_number(&receipt.receipt_number).await;
        eprintln!(
            "PROBE receipt={} currency={} bank={:?} rate={:?}",
            stored.receipt_number, stored.currency, stored.exchange_bank, stored.exchange_rate
        );
        assert_eq!(stored.currency, Currency::EUR);
        assert_eq!(stored.exchange_bank.as_deref(), Some("MNB"));
        assert!(stored.exchange_rate.is_some_and(|rate| rate > dec!(0)));
        assert_eq!(stored.totals.total.gross, dec!(1000));
    })
    .catch_unwind()
    .await;
    run.finish(result).await;
}

#[tokio::test]
#[ignore = "requires test-mode key, receipt prefix and SZAMLAZZ_RECEIPT_EMAIL; sends two emails"]
async fn receipt_email_resend() {
    // Validate the operator-controlled inbox setting before creating anything.
    let inbox = setting("SZAMLAZZ_RECEIPT_EMAIL");
    let mut run = ReceiptRun::new();
    let result = AssertUnwindSafe(async {
        let receipt = run.create(&run.request(Currency::HUF)).await;
        let stored = run.by_number(&receipt.receipt_number).await;
        assert_eq!(stored.id, receipt.id);
        assert_eq!(stored.call_id, receipt.call_id);
        assert_eq!(stored.order_number, receipt.order_number);
        assert_eq!(stored.document_type, ReceiptType::Receipt);
        assert!(!stored.reversed);
        let subject = format!("Receipt probe {}", run.run.order);
        let first = SendReceipt {
            email: Some(ReceiptEmail {
                to: Some(inbox.clone()),
                reply_to: Some(inbox.clone()),
                subject: Some(subject.clone()),
                body: Some("Receipt email probe".into()),
            }),
            ..SendReceipt::new(receipt.receipt_number.clone())
        };
        // Observed code 153 requires at least 15 seconds between notifications.
        // Space the two intended sends; never retry an unanswered email.
        for (stage, request, delay) in [
            ("first", first, std::time::Duration::ZERO),
            (
                "empty-block resend",
                SendReceipt::new(receipt.receipt_number.clone()),
                std::time::Duration::from_secs(16),
            ),
        ] {
            tokio::time::sleep(delay).await;
            run.run.sending(format!(
                "receipt email {stage} number={}",
                receipt.receipt_number
            ));
            let sent = run.run.client.send(&request).await;
            run.run.answered(sent);
            run.run.unresolved = None;
            eprintln!(
                "PROBE email acknowledged stage={stage} subject={subject}; check inbox manually"
            );
        }
    })
    .catch_unwind()
    .await;
    run.finish(result).await;
}
