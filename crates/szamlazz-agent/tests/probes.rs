//! Investigative vendor behavior; never part of the core live suite.
#[cfg(feature = "client-reqwest")]
mod live_support;
#[cfg(feature = "client-reqwest")]
#[path = "probes/receipts.rs"]
mod receipts;
#[cfg(feature = "client-reqwest")]
#[path = "probes/storno_email.rs"]
mod storno_email;

#[cfg(not(feature = "client-reqwest"))]
#[test]
#[ignore = "probes require client-reqwest and SZAMLAZZ_AGENT_KEY"]
fn missing_transport() {
    panic!("select --all-features or --features client-reqwest");
}

#[cfg(feature = "client-reqwest")]
async fn mismatch(electronic: bool) {
    use futures_util::FutureExt;
    use live_support::{Run, document};
    use szamlazz_agent::{DocumentType, ops::invoice::InvoiceKind};
    let mut run = Run::new();
    let result = std::panic::AssertUnwindSafe(async {
        let mut request = document(InvoiceKind::invoice());
        request.e_invoice = electronic;
        let created = run.create(request, "appearance").await;
        let original = run.by_number(&created.invoice_number).await;
        assert_eq!(original.info.test, Some(true));
        assert_eq!(original.info.appearance.is_e_invoice(), electronic);
        let reversal = run
            .reverse(
                &created.invoice_number,
                !electronic,
                run.external_id("storno"),
            )
            .await;
        let storno = run.by_number(&reversal.invoice_number).await;
        eprintln!(
            "PROBE original={} appearance={:?} reversal={} appearance={:?}",
            created.invoice_number,
            original.info.appearance,
            reversal.invoice_number,
            storno.info.appearance
        );
        assert_eq!(storno.info.document_type, DocumentType::Storno);
        assert_eq!(
            storno.info.referenced_invoice_number,
            Some(created.invoice_number)
        );
        assert_eq!(storno.info.appearance.is_e_invoice(), !electronic);
    })
    .catch_unwind()
    .await;
    run.finish(result).await;
}

#[cfg(feature = "client-reqwest")]
#[tokio::test]
#[ignore = "requires test-mode SZAMLAZZ_AGENT_KEY; probes mismatching storno appearance"]
async fn electronic_original_paper_storno() {
    mismatch(true).await;
}

#[cfg(feature = "client-reqwest")]
#[tokio::test]
#[ignore = "requires test-mode SZAMLAZZ_AGENT_KEY; probes mismatching storno appearance"]
async fn paper_original_electronic_storno() {
    mismatch(false).await;
}

/// Tests the documented zero-entry shape on populated and already-empty
/// invoices. A failed expectation is a probe result, not permission to resend.
#[cfg(feature = "client-reqwest")]
async fn clearing(populated: bool) {
    use futures_util::FutureExt;
    use live_support::{Run, assert_reported_number, document, today};
    use rust_decimal::dec;
    use szamlazz_agent::PaymentMethod;
    use szamlazz_agent::ops::credit_entry::{ClearCreditEntries, CreditEntry, RegisterCreditEntry};
    use szamlazz_agent::ops::invoice::InvoiceKind;

    let mut run = Run::new();
    let result = std::panic::AssertUnwindSafe(async {
        let created = run.create(document(InvoiceKind::invoice()), "clear").await;
        let number = created.invoice_number;
        let original = run.by_number(&number).await;
        assert_eq!(original.info.test, Some(true));
        assert!(original.credit_entries.is_empty());
        if populated {
            let mut register = RegisterCreditEntry::new(number.clone());
            register
                .entries
                .push(CreditEntry::new(
                    today(),
                    PaymentMethod::Transfer,
                    dec!(100),
                ))
                .expect("one entry");
            run.sending(format!("register credit entry on {number}"));
            let sent = run.client.send(&register).await;
            let balance = run.answered(sent);
            run.unresolved = None;
            assert_reported_number(balance.invoice_number.as_ref(), &number);
            let stored = run.by_number(&number).await;
            assert_eq!(stored.credit_entries.len(), 1);
            assert_eq!(stored.credit_entries[0].amount, dec!(100));
        }

        let state = if populated {
            "populated"
        } else {
            "already-empty"
        };
        run.sending(format!("clear credit entries {number} state={state}"));
        let sent = run
            .client
            .send(&ClearCreditEntries::new(number.clone()))
            .await;
        let balance = run.answered(sent);
        run.unresolved = None;
        let stored = run.by_number(&number).await;
        eprintln!(
            "PROBE clear number={number} state={state} echoed={:?} outstanding={:?} entries={:?}",
            balance.invoice_number, balance.outstanding, stored.credit_entries
        );
        assert_reported_number(balance.invoice_number.as_ref(), &number);
        assert!(stored.credit_entries.is_empty());
        assert_eq!(balance.outstanding, Some(original.totals.total.gross));
    })
    .catch_unwind()
    .await;
    run.finish(result).await;
}

#[cfg(feature = "client-reqwest")]
#[tokio::test]
#[ignore = "requires test-mode SZAMLAZZ_AGENT_KEY; probes clearing populated credit entries"]
async fn clear_credit_entries_populated() {
    clearing(true).await;
}

#[cfg(feature = "client-reqwest")]
#[tokio::test]
#[ignore = "requires test-mode SZAMLAZZ_AGENT_KEY; probes clearing already-empty credit entries"]
async fn clear_credit_entries_already_empty() {
    clearing(false).await;
}
