//! Shared vendor-live fixtures and failure-safe cleanup, also used by the worker.
#![allow(dead_code)]

use jiff::civil::Date;
use rust_decimal::{Decimal, dec};
use std::any::Any;
use szamlazz_agent::ops::invoice::{
    Buyer, CreateInvoice, CreatedInvoice, InvoiceHeader, InvoiceKind,
};
use szamlazz_agent::ops::proforma::{DeleteProforma, ProformaSelector};
use szamlazz_agent::ops::query_xml::{InvoiceDocument, QueryInvoiceXml};
use szamlazz_agent::ops::storno::StornoInvoice;
use szamlazz_agent::{
    Client, ClientError, Credentials, Currency, DocumentType, InvoiceNumber, InvoiceSelector,
    Language, LineItem, OutcomeClass, PaymentMethod, Rounding, VatRate,
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

pub fn document(kind: InvoiceKind) -> CreateInvoice {
    let mut request = CreateInvoice::new(
        kind,
        InvoiceHeader::new(
            previous_month(),
            today(),
            PaymentMethod::Transfer,
            Currency::HUF,
            Language::Hungarian,
        ),
        Buyer::new("Teszt Vevő", "1010", "Budapest", "Teszt utca 1."),
        vec![
            LineItem::try_calculated(
                "Integrációs teszt tétel",
                dec!(2),
                "db",
                dec!(1234.25),
                VatRate::percent(27),
                Rounding::minor_unit(&Currency::HUF),
            )
            .expect("fits"),
        ],
    );
    request.download_pdf = true;
    request
}

pub struct Run {
    pub client: Client,
    pub order: String,
    known: Vec<InvoiceNumber>,
    pub unresolved: Option<String>,
}

impl Run {
    pub fn new() -> Self {
        // Fresh even when a caller accidentally reuses its Dagger run label.
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

    /// These direct sends execute once. Unlike a replayed worker run, a
    /// definitive refusal here cannot hide an earlier execution of this send.
    pub fn answered<T>(&mut self, result: Result<T, ClientError>) -> T {
        match result {
            Ok(answer) => answer,
            Err(error) => {
                if error.outcome_class() == OutcomeClass::Rejected {
                    self.unresolved = None;
                }
                panic!("vendor write: {error}");
            }
        }
    }

    pub async fn create(&mut self, mut request: CreateInvoice, kind: &str) -> CreatedInvoice {
        request.header.order_number = Some(self.order.clone());
        request.external_id = Some(self.external_id(kind));
        self.sending(format!("create {}", self.external_id(kind)));
        let sent = self.client.send(&request).await;
        let result = self
            .answered(sent)
            .into_issued()
            .expect("numbered document");
        self.record(result.invoice_number.clone());
        self.unresolved = None;
        result
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

    pub async fn absent(&self, selector: InvoiceSelector) {
        let error = self
            .client
            .send(&QueryInvoiceXml::new(selector))
            .await
            .expect_err("document must be absent");
        assert_eq!(error.outcome_class(), OutcomeClass::NotFound, "{error}");
    }

    pub async fn reverse(
        &mut self,
        number: &InvoiceNumber,
        electronic: bool,
        external_id: String,
    ) -> CreatedInvoice {
        let mut request = StornoInvoice::new(number.clone());
        request.e_invoice = electronic;
        request.fulfillment_date = self.by_number(number).await.info.fulfillment_date;
        request.external_id = Some(external_id.clone());
        self.sending(format!("storno {number} external_id={external_id}"));
        let sent = self.client.send(&request).await;
        let result = self.answered(sent);
        eprintln!("LIVE reversal={} original={number}", result.invoice_number);
        verify_reversal(&self.client, number, &result)
            .await
            .expect("unverified storno; reconcile before cleanup");
        self.unresolved = None;
        result
    }

    pub async fn delete(&mut self, number: &InvoiceNumber) {
        self.sending(format!("delete proforma {number}"));
        let sent = self
            .client
            .send(&DeleteProforma::new(ProformaSelector::InvoiceNumber(
                number.clone(),
            )))
            .await;
        self.answered(sent);
        self.unresolved = None;
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
                let queried = self
                    .client
                    .send(&QueryInvoiceXml::new(InvoiceSelector::InvoiceNumber(
                        number.clone(),
                    )))
                    .await;
                let doc = match queried {
                    Ok(doc) => doc,
                    Err(error) if error.outcome_class() == OutcomeClass::NotFound => continue,
                    Err(error) => {
                        failures.push(format!("query {number}: {error}; linked cleanup stopped"));
                        break;
                    }
                };
                if doc.info.reversed == Some(true) {
                    continue;
                }
                if doc.info.document_type == DocumentType::Proforma {
                    if let Err(error) = self
                        .client
                        .send(&DeleteProforma::new(ProformaSelector::InvoiceNumber(
                            number.clone(),
                        )))
                        .await
                    {
                        failures.push(format!("delete {number}: {error}"));
                        break;
                    }
                    eprintln!("LIVE cleanup deleted {number}");
                } else if matches!(
                    doc.info.document_type,
                    DocumentType::Invoice | DocumentType::Prepayment | DocumentType::Final
                ) {
                    let mut request = StornoInvoice::new(number.clone());
                    request.e_invoice = doc.info.appearance.is_e_invoice();
                    request.fulfillment_date = doc.info.fulfillment_date;
                    match self.client.send(&request).await {
                        Ok(reversal) if reversal.invoice_number != number => {
                            eprintln!(
                                "LIVE cleanup reversal={} original={number}",
                                reversal.invoice_number
                            );
                            if let Err(error) =
                                verify_reversal(&self.client, &number, &reversal).await
                            {
                                failures.push(error);
                                break;
                            }
                        }
                        other => {
                            failures.push(format!(
                                "unsupported/unresolved cleanup storno {number}: {other:?}"
                            ));
                            break;
                        }
                    }
                } else {
                    failures.push(format!("unsupported cleanup type for {number}"));
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
}

async fn verify_reversal(
    client: &Client,
    original: &InvoiceNumber,
    reply: &CreatedInvoice,
) -> Result<(), String> {
    let reversal = client
        .send(&QueryInvoiceXml::new(InvoiceSelector::InvoiceNumber(
            reply.invoice_number.clone(),
        )))
        .await
        .map_err(|error| {
            format!(
                "unverified reversal {} of {original}: {error}",
                reply.invoice_number
            )
        })?;
    if reversal.info.document_type == DocumentType::Storno
        && reversal.info.referenced_invoice_number.as_ref() == Some(original)
        && &reversal.info.invoice_number != original
    {
        Ok(())
    } else {
        Err(format!(
            "unsupported/unverified reversal {} of {original}",
            reply.invoice_number
        ))
    }
}

// Expected values read naturally as inline domain tokens at each scenario.
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
