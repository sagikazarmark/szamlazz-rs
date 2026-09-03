//! The stateless `SzamlaAgent` service handlers: `query`, `set_payments` and
//! `storno` by document number.

use std::sync::Arc;

use restate_sdk::errors::HandlerError;
use restate_sdk::prelude::Context;
use serde::{Deserialize, Serialize};
use szamlazz_agent::ops::query_xml::InvoiceDocument;

use super::SzamlaAgentService;
use super::support::service::run_once;
use super::support::{Fault, outstanding, terminal};
use crate::contract::{
    PaymentRecord, QueryRequest, QueryResponse, SetPaymentsRequest, SetPaymentsResponse,
    StornoOutcome, StornoRequest, StornoResponse,
};
use crate::identity::ExternalId;
use crate::szamla_agent::{
    QueryError, QueryOutcome, SetPaymentsOutcome, StornoAttempt, StornoOutcome as AgentStorno,
};

/// The journaled result of the `query` handler's run.
#[derive(Debug, Clone, Serialize, Deserialize)]
enum QueryRun {
    Found(Box<QueryResponse>),
    NotFound,
    Api { code: String, message: String },
    Unavailable(String),
    Transport(String),
}

impl From<Result<InvoiceDocument, QueryError>> for QueryRun {
    fn from(result: Result<InvoiceDocument, QueryError>) -> Self {
        match result {
            Ok(document) => Self::Found(Box::new(project(&document))),
            Err(QueryError::NotFound) => Self::NotFound,
            Err(QueryError::Api { code, message }) => Self::Api { code, message },
            Err(QueryError::Unavailable(message)) => Self::Unavailable(message),
            Err(QueryError::Transport(message)) => Self::Transport(message),
        }
    }
}

impl SzamlaAgentService {
    pub(super) async fn query_request(
        &self,
        ctx: &Context<'_>,
        request: QueryRequest,
    ) -> Result<QueryResponse, HandlerError> {
        let agent = Arc::clone(&self.agent);
        let selector = request.selector;
        let run = run_once(ctx, "query", move || async move {
            QueryRun::from(agent.query_document(&selector).await)
        })
        .await?;
        match run {
            QueryRun::Found(response) => Ok(*response),
            QueryRun::NotFound => Err(terminal(
                404,
                "not_found",
                "szamlazz.hu does not know the document (code 7)",
            )),
            QueryRun::Api { code, message } => Err(terminal(
                422,
                &code,
                format!("szamlazz.hu error {code}: {message}"),
            )),
            QueryRun::Unavailable(message) | QueryRun::Transport(message) => {
                Err(Fault::unavailable(message).into())
            }
        }
    }

    pub(super) async fn set_payments_request(
        &self,
        ctx: &Context<'_>,
        request: SetPaymentsRequest,
    ) -> Result<SetPaymentsResponse, HandlerError> {
        let SetPaymentsRequest {
            invoice_number,
            entries,
            additive,
        } = request;
        let agent = Arc::clone(&self.agent);
        let number = invoice_number.clone();
        let outcome = run_once(
            ctx,
            format!("set-payments-{invoice_number}"),
            move || async move { agent.set_payments(&number, &entries, additive).await },
        )
        .await?;
        match outcome {
            SetPaymentsOutcome::Done { outstanding, gross } => {
                let mut response = SetPaymentsResponse::new(invoice_number);
                response.outstanding = outstanding;
                response.gross_total = gross;
                Ok(response)
            }
            SetPaymentsOutcome::Rejected { code, message } => Err(terminal(
                422,
                &code,
                format!("szamlazz.hu refused the credit entries ({code}): {message}"),
            )),
            SetPaymentsOutcome::Transport(message) => Err(Fault::outcome_unknown(format!(
                "credit entry registration outcome unknown: {message}; call set_payments again"
            ))
            .into()),
        }
    }

    pub(super) async fn storno_request(
        &self,
        ctx: &Context<'_>,
        request: StornoRequest,
    ) -> Result<StornoResponse, HandlerError> {
        let StornoRequest {
            invoice_number: number,
            comment,
        } = request;

        // Query first: a document with an order number is managed by an
        // `Order`; this service never calls into it.
        let found = {
            let agent = Arc::clone(&self.agent);
            let number = number.clone();
            run_once(ctx, format!("verify-{number}"), move || async move {
                agent.verify(&number).await
            })
            .await?
        };
        let found = match found {
            QueryOutcome::Found(found) => found,
            QueryOutcome::NotFound => {
                return Err(terminal(
                    404,
                    "not_found",
                    format!("szamlazz.hu does not know invoice {number} (code 7)"),
                ));
            }
            QueryOutcome::Transport(message) => return Err(Fault::unavailable(message).into()),
        };
        if let Some(order) = found
            .order_number
            .as_deref()
            .map(str::trim)
            .filter(|order| !order.is_empty())
        {
            return Ok(
                StornoResponse::new(StornoOutcome::ManagedByOrder, number).with_order_key(order)
            );
        }
        if found.reversed == Some(true) {
            return Ok(StornoResponse::new(StornoOutcome::Reversed, number));
        }
        let e_invoice = found.e_invoice.unwrap_or(self.config.defaults.e_invoice);
        let external_id = ExternalId::new(format!(
            "{}:by-number:{number}:storno",
            self.config.account.slug
        ));

        let outcome = {
            let agent = Arc::clone(&self.agent);
            let number = number.clone();
            let external_id = external_id.clone();
            let comment = comment.clone();
            run_once(ctx, format!("storno-{number}"), move || async move {
                agent
                    .storno(StornoAttempt {
                        invoice_number: &number,
                        external_id: &external_id,
                        comment: comment.as_deref(),
                        e_invoice,
                    })
                    .await
            })
            .await?
        };
        match outcome {
            AgentStorno::Reversed { storno_number, .. }
            | AgentStorno::AlreadyReversed { storno_number } => {
                Ok(StornoResponse::new(StornoOutcome::Reversed, number)
                    .with_storno_number(storno_number))
            }
            AgentStorno::NotStornoable => Ok(StornoResponse::new(StornoOutcome::Rejected, number)
                .with_code("not_stornoable")
                .with_message("szamlazz.hu echoed the document unchanged: it cannot be reversed")),
            AgentStorno::Rejected { code, message } => {
                Ok(StornoResponse::new(StornoOutcome::Rejected, number)
                    .with_code(code)
                    .with_message(message))
            }
            AgentStorno::Unknown { message, .. } | AgentStorno::Transport(message) => {
                Err(Fault::outcome_unknown(format!(
                    "storno outcome unknown: {message}; call storno again"
                ))
                .into())
            }
        }
    }
}

/// The [`QueryResponse`] projection of a queried document.
fn project(document: &InvoiceDocument) -> QueryResponse {
    let info = &document.info;
    let mut response = QueryResponse::new(info.invoice_number.as_str(), info.document_type.clone());
    response.reversed = info.reversed;
    response.referenced_invoice_number = info
        .referenced_invoice_number
        .as_ref()
        .map(|number| number.as_str().to_owned());
    response.referenced_proforma_number = info
        .referenced_proforma_number
        .as_ref()
        .map(|number| number.as_str().to_owned());
    response.order_number.clone_from(&info.order_number);
    response.issue_date = info.issue_date;
    response.fulfillment_date = info.fulfillment_date;
    response.due_date = info.due_date;
    response.currency.clone_from(&info.currency);
    response.net_total = Some(document.totals.total.net);
    response.vat_total = Some(document.totals.total.vat);
    response.gross_total = Some(document.totals.total.gross);
    response.payments = document
        .payments
        .iter()
        .map(|payment| {
            let mut record = PaymentRecord::new(payment.amount);
            record.date = Some(payment.date);
            record.title = Some(payment.title.clone());
            record.comment.clone_from(&payment.comment);
            record.bank_account.clone_from(&payment.bank_account);
            record
        })
        .collect();
    let amounts: Vec<_> = document
        .payments
        .iter()
        .map(|payment| payment.amount)
        .collect();
    response.outstanding = outstanding(response.gross_total, &amounts);
    response.supplier_id = document.supplier.id;
    response.test = info.test;
    response
}
