//! Investigative appearance cases from #73; never part of the core live suite.
#[cfg(feature = "client-reqwest")]
mod live_support;

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
