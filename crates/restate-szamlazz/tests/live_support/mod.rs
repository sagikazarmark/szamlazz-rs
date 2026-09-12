//! Package-local support for worker live journeys. Only direct observation and
//! best-effort cleanup live here; issuance runs through the Restate services.

use jiff::civil::Date;
use rust_decimal::Decimal;
use std::any::Any;
use szamlazz_agent::ops::proforma::{DeleteProforma, ProformaSelector};
use szamlazz_agent::ops::query_xml::{InvoiceDocument, QueryInvoiceXml};
use szamlazz_agent::ops::storno::StornoInvoice;
use szamlazz_agent::{
    Client, Credentials, Currency, DocumentType, InvoiceNumber, InvoiceSelector, OutcomeClass,
};

pub fn today() -> Date {
    jiff::Zoned::now()
        .with_time_zone(jiff::tz::TimeZone::get("Europe/Budapest").expect("Budapest timezone"))
        .date()
}

pub fn previous_month() -> Date {
    today()
        .first_of_month()
        .checked_sub(jiff::Span::new().days(1))
        .expect("previous month")
}

pub fn key() -> String {
    std::env::var("SZAMLAZZ_AGENT_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
        .expect("selected live test requires SZAMLAZZ_AGENT_KEY for a test-mode account")
}

pub struct Run {
    client: Client,
    pub order: String,
    known: Vec<InvoiceNumber>,
    pub unresolved: Option<String>,
}

impl Run {
    pub fn new() -> Self {
        let order = uuid::Uuid::new_v4().to_string();
        eprintln!(
            "LIVE run={} order={order}",
            std::env::var("SZAMLAZZ_LIVE_RUN_ID").unwrap_or_else(|_| "local".into())
        );
        Self {
            client: Client::new(Credentials::agent_key(key())).expect("client"),
            order,
            known: Vec::new(),
            unresolved: None,
        }
    }

    pub fn external_id(&self, kind: &str) -> String {
        format!("live:{}:{kind}", self.order)
    }

    pub fn record(&mut self, number: InvoiceNumber) {
        eprintln!("LIVE order={} document={number}", self.order);
        if !self.known.contains(&number) {
            self.known.push(number);
        }
    }

    pub fn sending(&mut self, intent: String) {
        eprintln!(
            "LIVE sending {intent}; an unanswered write must be reconciled, never rerun blindly"
        );
        self.unresolved = Some(intent);
    }

    pub async fn query(&self, selector: InvoiceSelector) -> InvoiceDocument {
        self.client
            .send(&QueryInvoiceXml::new(selector))
            .await
            .expect("vendor query")
    }

    pub async fn by_number(&self, number: &InvoiceNumber) -> InvoiceDocument {
        self.query(InvoiceSelector::InvoiceNumber(number.clone()))
            .await
    }

    pub async fn finish(mut self, result: Result<(), Box<dyn Any + Send>>) {
        let mut failures = Vec::new();
        if let Some(intent) = &self.unresolved {
            failures.push(format!(
                "UNRESOLVED {intent}; cleanup deferred: settle the exact write first"
            ));
        } else {
            // Last created first: final before prepayment, invoice before proforma.
            while let Some(number) = self.known.pop() {
                if let Err(error) = self.cleanup(&number).await {
                    failures.push(error);
                    break;
                }
            }
        }
        for failure in &failures {
            eprintln!("LIVE CLEANUP: {failure}");
        }
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
        assert!(failures.is_empty(), "live cleanup needs operator attention");
    }

    async fn cleanup(&self, number: &InvoiceNumber) -> Result<(), String> {
        let doc = match self
            .client
            .send(&QueryInvoiceXml::new(InvoiceSelector::InvoiceNumber(
                number.clone(),
            )))
            .await
        {
            Ok(doc) => doc,
            Err(error) if error.outcome_class() == OutcomeClass::NotFound => return Ok(()),
            Err(error) => return Err(format!("query {number}: {error}; linked cleanup stopped")),
        };
        if &doc.info.invoice_number != number {
            return Err(format!(
                "cleanup query returned another document for {number}"
            ));
        }
        if doc.info.reversed == Some(true) {
            return Ok(());
        }
        if doc.info.document_type == DocumentType::Proforma {
            self.client
                .send(&DeleteProforma::new(ProformaSelector::InvoiceNumber(
                    number.clone(),
                )))
                .await
                .map_err(|error| format!("delete {number}: {error}"))?;
            eprintln!("LIVE cleanup deleted {number}");
            return Ok(());
        }
        if !matches!(
            doc.info.document_type,
            DocumentType::Invoice | DocumentType::Prepayment | DocumentType::Final
        ) {
            return Err(format!("unsupported cleanup type for {number}"));
        }
        let mut request = StornoInvoice::new(number.clone());
        request.e_invoice = doc.info.appearance.is_e_invoice();
        request.fulfillment_date = doc.info.fulfillment_date;
        let reversal = self
            .client
            .send(&request)
            .await
            .map_err(|error| format!("unresolved cleanup storno {number}: {error}"))?
            .into_numbered()
            .map_err(|_| format!("unnumbered cleanup storno {number}"))?;
        eprintln!(
            "LIVE cleanup reversal={} original={number}",
            reversal.invoice_number
        );
        verify_reversal(&self.client, number, &reversal.invoice_number).await
    }
}

// Kept package-local so a packaged worker's tests need no sibling test sources.
// Like the Agent suite, cleanup requires both the exact candidate and a fresh
// reversed original; neither a bare acknowledgement nor absence unlocks it.
async fn verify_reversal(
    client: &Client,
    original: &InvoiceNumber,
    candidate: &InvoiceNumber,
) -> Result<(), String> {
    let reversal = client
        .send(&QueryInvoiceXml::new(InvoiceSelector::InvoiceNumber(
            candidate.clone(),
        )))
        .await
        .map_err(|error| format!("unverified reversal {candidate} of {original}: {error}"))?;
    if reversal.info.document_type != DocumentType::Storno
        || reversal.info.referenced_invoice_number.as_ref() != Some(original)
        || &reversal.info.invoice_number != candidate
        || candidate == original
    {
        return Err(format!(
            "unsupported/unverified reversal {candidate} of {original}"
        ));
    }
    let refreshed = client
        .send(&QueryInvoiceXml::new(InvoiceSelector::InvoiceNumber(
            original.clone(),
        )))
        .await
        .map_err(|error| format!("unverified original {original}: {error}"))?;
    if &refreshed.info.invoice_number != original || refreshed.info.reversed != Some(true) {
        return Err(format!("original {original} reversal is not established"));
    }
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
pub fn assert_document(
    doc: &InvoiceDocument,
    number: &InvoiceNumber,
    order: &str,
    kind: DocumentType,
    currency: Currency,
    totals: (Decimal, Decimal, Decimal),
) {
    assert_eq!(&doc.info.invoice_number, number);
    assert_eq!(doc.info.order_number.as_deref(), Some(order));
    assert_eq!(doc.info.document_type, kind);
    assert_eq!(
        doc.info.test,
        Some(true),
        "live suite requires a test-mode account"
    );
    assert_eq!(doc.info.currency, Some(currency));
    assert_eq!(
        (
            doc.totals.total.net,
            doc.totals.total.vat,
            doc.totals.total.gross
        ),
        totals
    );
}
