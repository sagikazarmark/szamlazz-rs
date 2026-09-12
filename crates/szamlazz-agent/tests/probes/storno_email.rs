//! Recipient experiments, deliberately separate from routine live acceptance.
use futures_util::FutureExt;
use szamlazz_agent::{SellerEmail, ops::invoice::InvoiceKind, ops::storno::StornoInvoice};

use super::live_support::{Run, document, verify_reversal};

async fn observe(electronic: bool, explicit: bool) {
    let recipient = std::env::var("SZAMLAZZ_STORNO_EMAIL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .expect("set SZAMLAZZ_STORNO_EMAIL to an operator-controlled mailbox");
    let mut run = Run::new();
    let result = std::panic::AssertUnwindSafe(async {
        let case = format!(
            "{}-{}",
            if electronic { "electronic" } else { "paper" },
            if explicit { "explicit" } else { "omitted" }
        );
        let subject = format!("Storno email probe {case} {}", run.order);
        let mut request = document(InvoiceKind::invoice());
        request.e_invoice = electronic;
        request.buyer.email = Some(recipient.clone());
        request.buyer.send_email = Some(false);
        let created = run.create(request, "original").await;
        let original = run.by_number(&created.invoice_number).await;
        assert_eq!(original.info.test, Some(true), "requires a test account");
        assert_eq!(original.info.appearance.is_e_invoice(), electronic);
        let request = StornoInvoice {
            e_invoice: electronic,
            fulfillment_date: original.info.fulfillment_date,
            external_id: Some(run.external_id("storno")),
            buyer_email: explicit.then_some(recipient),
            seller_email: Some(SellerEmail {
                subject: Some(subject.clone()),
                body: Some("Controlled notification experiment; no payment is due.".into()),
                ..SellerEmail::default()
            }),
            ..StornoInvoice::new(created.invoice_number.clone())
        };
        run.sending(format!("storno {} external_id={} case={case} subject={subject}", created.invoice_number, run.external_id("storno")));
        let sent = run.client.send(&request).await;
        let reversal = run.answered(sent).into_numbered().expect("numbered storno");
        eprintln!(
            "PROBE case={case} original={} storno={} notification_delivery_failed={} subject={subject}",
            created.invoice_number, reversal.invoice_number, reversal.notification_delivery_failed
        );
        verify_reversal(&run.client, &created.invoice_number, &reversal)
            .await
            .expect("matching storno and reversed original");
        run.unresolved = None;
        let stored = run.by_number(&reversal.invoice_number).await;
        assert_eq!(stored.info.fulfillment_date, original.info.fulfillment_date);
        assert_eq!(stored.info.appearance.is_e_invoice(), electronic);
        eprintln!("PROBE verified reversal; notification attempt and mailbox receipt require independent observation. Test-account routing may override the requested recipient.");
    })
    .catch_unwind()
    .await;
    run.finish(result).await;
}

#[tokio::test]
#[ignore = "requires test-mode SZAMLAZZ_AGENT_KEY and controlled SZAMLAZZ_STORNO_EMAIL"]
async fn paper_omitted() {
    observe(false, false).await;
}

#[tokio::test]
#[ignore = "requires test-mode SZAMLAZZ_AGENT_KEY and controlled SZAMLAZZ_STORNO_EMAIL"]
async fn paper_explicit() {
    observe(false, true).await;
}

#[tokio::test]
#[ignore = "requires test-mode SZAMLAZZ_AGENT_KEY and controlled SZAMLAZZ_STORNO_EMAIL"]
async fn electronic_omitted() {
    observe(true, false).await;
}

#[tokio::test]
#[ignore = "requires test-mode SZAMLAZZ_AGENT_KEY and controlled SZAMLAZZ_STORNO_EMAIL"]
async fn electronic_explicit() {
    observe(true, true).await;
}
