//! Focused vendor-live regression tests. See docs/testing.md for opt-in commands.
#[cfg(feature = "client-reqwest")]
mod live_support;

// A selected target without its transport must fail, not pass with zero tests.
#[cfg(not(feature = "client-reqwest"))]
#[test]
#[ignore = "live suite requires client-reqwest and SZAMLAZZ_AGENT_KEY"]
fn missing_transport() {
    panic!("select --all-features or --features client-reqwest");
}

#[cfg(feature = "client-reqwest")]
mod scenarios {
    use super::live_support::*;
    use futures_util::FutureExt;
    use rust_decimal::dec;
    use std::panic::AssertUnwindSafe;
    use szamlazz_agent::ops::credit_entry::{CreditEntry, RegisterCreditEntry};
    use szamlazz_agent::ops::invoice::InvoiceKind;
    use szamlazz_agent::ops::query_pdf::QueryInvoicePdf;
    use szamlazz_agent::ops::query_xml::{InvoiceAppearance, QueryInvoiceXml};
    use szamlazz_agent::ops::taxpayer::QueryTaxpayer;
    use szamlazz_agent::{Currency, DocumentType, InvoiceSelector, PaymentMethod};

    #[tokio::test]
    #[ignore = "requires SZAMLAZZ_AGENT_KEY; read-only NAV smoke"]
    async fn taxpayer_query() {
        let run = Run::new();
        let info = run
            .client
            .send(&QueryTaxpayer::new("13421739").expect("prefix"))
            .await
            .expect("NAV taxpayer dependency failed");
        assert_eq!(info.valid, Some(true));
        assert!(info.name.is_some_and(|name| !name.trim().is_empty()));
        assert_eq!(info.tax_number.as_deref(), Some("13421739"));
    }

    #[tokio::test]
    #[ignore = "requires SZAMLAZZ_AGENT_KEY; issues and reverses a test invoice"]
    #[allow(clippy::too_many_lines)] // One ordered business lifecycle, including cleanup.
    async fn invoice_lifecycle() {
        let mut run = Run::new();
        let result = AssertUnwindSafe(async {
            let created = run
                .create(document(InvoiceKind::invoice()), "invoice")
                .await;
            let number = created.invoice_number.clone();
            assert_pdf(created.pdf.as_ref());
            assert_eq!(created.net_total, Some(dec!(2469)));
            assert_eq!(created.gross_total, Some(dec!(3136)));
            let original = run.by_number(&number).await;
            assert_document(
                &original,
                &number,
                &run.order,
                DocumentType::Invoice,
                Currency::HUF,
                (dec!(2469), dec!(667), dec!(3136)),
            );
            assert_eq!(original.info.appearance, InvoiceAppearance::Paper);
            assert_eq!(original.info.fulfillment_date, Some(previous_month()));
            let mut query =
                QueryInvoiceXml::new(InvoiceSelector::ExternalId(run.external_id("invoice")));
            query.include_pdf = true;
            let by_id = run
                .client
                .send(&query)
                .await
                .expect("query external id with PDF");
            assert_eq!(by_id.info.id, original.info.id);
            assert_eq!(by_id.info.invoice_number, number);
            assert_pdf(by_id.pdf.as_ref());
            let fetched = run
                .client
                .send(&QueryInvoicePdf::new(InvoiceSelector::InvoiceNumber(
                    number.clone(),
                )))
                .await
                .expect("standalone invoice PDF query");
            assert_eq!(fetched.invoice_number.as_ref(), Some(&number));
            assert_pdf(Some(&fetched.pdf));

            for (amount, additive, expected, outstanding) in [
                (dec!(100), false, vec![dec!(100)], dec!(3036)),
                (dec!(200), false, vec![dec!(200)], dec!(2936)),
                (dec!(50), true, vec![dec!(50), dec!(200)], dec!(2886)),
            ] {
                let mut request = RegisterCreditEntry::new(number.clone());
                request.additive = additive;
                request
                    .entries
                    .push(CreditEntry::new(today(), PaymentMethod::Transfer, amount))
                    .expect("one entry");
                run.sending(format!(
                    "credit entries {number} amount={amount} additive={additive}"
                ));
                let sent = run.client.send(&request).await;
                let balance = run.answered(sent);
                run.unresolved = None;
                assert_eq!(balance.invoice_number.as_ref(), Some(&number));
                assert_eq!(balance.outstanding, Some(outstanding));
                let stored = run.by_number(&number).await;
                let mut amounts: Vec<_> = stored
                    .credit_entries
                    .iter()
                    .map(|entry| entry.amount)
                    .collect();
                amounts.sort();
                assert_eq!(amounts, expected);
            }
            let reversal = run.reverse(&number, false, run.external_id("storno")).await;
            assert_ne!(reversal.invoice_number, number);
            let reversed = run.by_number(&number).await;
            assert_eq!(reversed.info.reversed, Some(true));
            assert!(
                reversed.credit_entries.is_empty(),
                "storno removes the original's registered credit entries"
            );
            let storno = run.by_number(&reversal.invoice_number).await;
            assert_document(
                &storno,
                &reversal.invoice_number,
                &run.order,
                DocumentType::Storno,
                Currency::HUF,
                (dec!(-2469), dec!(-667), dec!(-3136)),
            );
            assert_eq!(storno.info.referenced_invoice_number, Some(number.clone()));
            assert_eq!(storno.info.fulfillment_date, original.info.fulfillment_date);
            assert_eq!(storno.info.appearance, original.info.appearance);
            assert_eq!(
                run.query(InvoiceSelector::ExternalId(run.external_id("storno")))
                    .await
                    .info
                    .id,
                storno.info.id
            );
            let repeated = run.reverse(&number, false, run.external_id("storno")).await;
            assert_eq!(repeated.invoice_number, reversal.invoice_number);
        })
        .catch_unwind()
        .await;
        run.finish(result).await;
    }

    #[tokio::test]
    #[ignore = "requires SZAMLAZZ_AGENT_KEY; creates and deletes a test proforma"]
    async fn proforma_lifecycle() {
        let mut run = Run::new();
        let result = AssertUnwindSafe(async {
            let created = run
                .create(document(InvoiceKind::Proforma), "proforma")
                .await;
            let number = created.invoice_number;
            let stored = run.by_number(&number).await;
            assert_document(
                &stored,
                &number,
                &run.order,
                DocumentType::Proforma,
                Currency::HUF,
                (dec!(2469), dec!(667), dec!(3136)),
            );
            assert_eq!(
                run.query(InvoiceSelector::ExternalId(run.external_id("proforma")))
                    .await
                    .info
                    .id,
                stored.info.id
            );
            run.delete(&number).await;
            run.absent(InvoiceSelector::InvoiceNumber(number)).await;
            run.absent(InvoiceSelector::ExternalId(run.external_id("proforma")))
                .await;
        })
        .catch_unwind()
        .await;
        run.finish(result).await;
    }
}
