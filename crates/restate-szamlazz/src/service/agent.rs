//! The stateless `Szamlazz.Agent` service handlers: `query`, `set_payments`
//! and `storno` by document number.

use std::sync::Arc;

use restate_sdk::errors::HandlerError;
use restate_sdk::prelude::Context;
use serde::{Deserialize, Serialize};
use szamlazz_agent::ops::query_xml::InvoiceDocument;

use super::Agent;
use super::support::service::{lookup_storno, run_once, run_retrying};
use super::support::{Fault, terminal};
use crate::contract::{
    QueryRequest, QueryResponse, SetPaymentsRequest, SetPaymentsResponse, StornoOutcome,
    StornoRequest, StornoResponse,
};
use crate::gateway::{
    InvoiceDocumentExt as _, QueryError, QueryOutcome, SetPaymentsOutcome, StornoAttempt,
    StornoLookupOutcome, StornoOutcome as GatewayStorno,
};
use crate::identity::ExternalId;

/// The journaled result of the `query` handler's run.
#[derive(Debug, Clone, Serialize, Deserialize)]
enum QueryRun {
    Found(Box<QueryResponse>),
    NotFound,
    CredentialsRejected { code: String, message: String },
    Api { code: String, message: String },
    Unavailable(String),
    Transport(String),
}

impl From<Result<InvoiceDocument, QueryError>> for QueryRun {
    fn from(result: Result<InvoiceDocument, QueryError>) -> Self {
        match result {
            Ok(document) => Self::Found(Box::new(QueryResponse::from(&document))),
            Err(QueryError::NotFound) => Self::NotFound,
            Err(QueryError::CredentialsRejected { code, message }) => {
                Self::CredentialsRejected { code, message }
            }
            Err(QueryError::Api { code, message }) => Self::Api { code, message },
            Err(QueryError::Unavailable(message)) => Self::Unavailable(message),
            Err(QueryError::Transport(message)) => Self::Transport(message),
        }
    }
}

impl Agent {
    pub(super) async fn query_request(
        &self,
        ctx: &Context<'_>,
        request: QueryRequest,
    ) -> Result<QueryResponse, HandlerError> {
        let gateway = Arc::clone(&self.gateway);
        let selector = request.selector;
        let run = run_once(ctx, "query", move || async move {
            QueryRun::from(gateway.query_document(&selector).await)
        })
        .await?;
        match run {
            QueryRun::Found(response) => Ok(*response),
            QueryRun::NotFound => Err(terminal(
                404,
                "not_found",
                "szamlazz.hu does not know the document (code 7)",
            )),
            QueryRun::CredentialsRejected { code, message } => {
                Err(Fault::credentials_rejected(&self.config.namespace, code, message).into())
            }
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
        let gateway = Arc::clone(&self.gateway);
        let number = invoice_number.clone();
        let outcome = run_once(
            ctx,
            format!("set-payments-{invoice_number}"),
            move || async move { gateway.set_payments(&number, &entries, additive).await },
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
            SetPaymentsOutcome::CredentialsRejected { code, message } => {
                Err(Fault::credentials_rejected(&self.config.namespace, code, message).into())
            }
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
            let gateway = Arc::clone(&self.gateway);
            let number = number.clone();
            run_once(ctx, format!("verify-{number}"), move || async move {
                gateway.verify(&number).await
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
            QueryOutcome::CredentialsRejected { code, message } => {
                return Err(
                    Fault::credentials_rejected(&self.config.namespace, code, message).into(),
                );
            }
        };
        if let Some(order) = found
            .info
            .order_number
            .as_deref()
            .map(str::trim)
            .filter(|order| !order.is_empty())
        {
            return Ok(
                StornoResponse::new(StornoOutcome::ManagedByOrder, number).with_order_key(order)
            );
        }
        if found.info.reversed == Some(true) {
            return Ok(StornoResponse::new(StornoOutcome::Reversed, number));
        }
        let e_invoice = found
            .e_invoice()
            .unwrap_or(self.gateway.account().defaults.e_invoice);
        let external_id = ExternalId::for_unmanaged_storno(&self.config.namespace, &number);

        // The lookup step: a storno of ours already under the id.
        match lookup_storno(ctx, &self.gateway, &external_id, &number).await? {
            StornoLookupOutcome::Absent => {}
            StornoLookupOutcome::AlreadyReversed { storno_number } => {
                return Ok(StornoResponse::new(StornoOutcome::Reversed, number)
                    .with_storno_number(storno_number));
            }
            StornoLookupOutcome::CredentialsRejected { code, message } => {
                return Err(
                    Fault::credentials_rejected(&self.config.namespace, code, message).into(),
                );
            }
            StornoLookupOutcome::Transport(message) => {
                return Err(Fault::unavailable(message).into());
            }
        }

        // The storno step, under the issue policy: query-first on every
        // execution; its exhaustion is `outcome_unknown`.
        let outcome = self
            .storno_step(ctx, &number, &external_id, comment, e_invoice)
            .await?;
        match outcome {
            GatewayStorno::Reversed(storno) => {
                Ok(StornoResponse::new(StornoOutcome::Reversed, number)
                    .with_storno_number(storno.invoice_number.as_str()))
            }
            GatewayStorno::AlreadyReversed { storno_number } => {
                Ok(StornoResponse::new(StornoOutcome::Reversed, number)
                    .with_storno_number(storno_number))
            }
            GatewayStorno::NotStornoable => {
                Ok(StornoResponse::new(StornoOutcome::Rejected, number)
                    .with_code("not_stornoable")
                    .with_message(
                        "szamlazz.hu echoed the document unchanged: it cannot be reversed",
                    ))
            }
            GatewayStorno::Rejected { code, message } => {
                Ok(StornoResponse::new(StornoOutcome::Rejected, number)
                    .with_code(code)
                    .with_message(message))
            }
            GatewayStorno::CredentialsRejected { code, message } => {
                Err(Fault::credentials_rejected(&self.config.namespace, code, message).into())
            }
        }
    }

    /// The storno step under the issue policy's run retry policy (design §6
    /// step 3). Any `Err` from the run — exhaustion or cancellation — is
    /// `outcome_unknown`: the next call's lookup finds whatever landed.
    async fn storno_step(
        &self,
        ctx: &Context<'_>,
        number: &str,
        external_id: &ExternalId,
        comment: Option<String>,
        e_invoice: bool,
    ) -> Result<GatewayStorno, HandlerError> {
        let gateway = Arc::clone(&self.gateway);
        let invoice_number = number.to_owned();
        let external_id = external_id.clone();
        run_retrying(
            ctx,
            format!("storno-{number}"),
            self.config.issue.run_retry_policy(),
            move || async move {
                gateway
                    .storno(StornoAttempt {
                        invoice_number: &invoice_number,
                        external_id: &external_id,
                        comment: comment.as_deref(),
                        e_invoice,
                    })
                    .await
            },
        )
        .await
        .map_err(|error| {
            Fault::outcome_unknown(format!(
                "the storno step ended without a confirmed outcome ({}): {}; call storno again",
                error.code(),
                error.message()
            ))
            .into()
        })
    }
}
