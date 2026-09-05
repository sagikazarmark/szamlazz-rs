//! The stateless `Szamlazz.Agent` service handlers: `query`, `set_payments`
//! and `storno` by document number, and the `check_account` probe.
//!
//! `query` and `storno` check the document they find against the account the
//! invocation resolved to (`support::check_pins`) before they answer or
//! send; `set_payments` finds no document and is exempt.

use std::sync::Arc;

use restate_sdk::errors::HandlerError;
use restate_sdk::prelude::Context;
use serde::{Deserialize, Serialize};
use szamlazz_agent::ops::query_xml::InvoiceDocument;

use super::prologue::Execution;
use super::support::service::{lookup_storno, run_once, storno_step};
use super::support::{Fault, StornoIntent, check_pins, storno_response, terminal};
use crate::contract::{
    CheckAccountResponse, CheckedAccount, CredentialsCheck, QueryRequest, QueryResponse,
    SetPaymentsRequest, SetPaymentsResponse, StornoOutcome, StornoRequest, StornoResponse,
};
use crate::gateway::{
    InvoiceDocumentExt as _, ProbeOutcome, QueryError, QueryOutcome, SetPaymentsOutcome,
    StornoLookupOutcome,
};
use crate::identity::ExternalId;

/// What the probe step settled, as `check_account`'s `credentials` — or the
/// one answer that settles nothing.
///
/// # Errors
///
/// The `unavailable` fault of a probe whose exchange failed: szamlazz.hu's
/// verdict on the credentials is not known, so the handler reports neither
/// `ok` nor `rejected`.
pub(super) fn credentials_check(outcome: ProbeOutcome) -> Result<CredentialsCheck, Fault> {
    match outcome {
        ProbeOutcome::Accepted => Ok(CredentialsCheck::Ok),
        ProbeOutcome::CredentialsRejected { code, message } => {
            Ok(CredentialsCheck::Rejected { code, message })
        }
        ProbeOutcome::Transport(message) => Err(Fault::unavailable(format!(
            "the account probe's exchange with szamlazz.hu failed ({message}): the credentials were neither accepted nor rejected; call check_account again"
        ))),
    }
}

/// The journaled result of the `query` handler's run: the document as found,
/// checked against the account and projected after the journal — the same
/// entry `verify` writes, so the check reads one shape everywhere. (Before
/// #32 the entry was the projection; a `query` in flight across that deploy
/// replays an undecodable entry and is killed — accepted for a one-step
/// read-only handler with a one-day journal retention.)
#[derive(Debug, Clone, Serialize, Deserialize)]
enum QueryRun {
    Found(Box<InvoiceDocument>),
    NotFound,
    CredentialsRejected { code: String, message: String },
    Api { code: String, message: String },
    Unavailable(String),
    Transport(String),
}

impl From<Result<InvoiceDocument, QueryError>> for QueryRun {
    fn from(result: Result<InvoiceDocument, QueryError>) -> Self {
        match result {
            Ok(document) => Self::Found(Box::new(document)),
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

impl Execution {
    /// The `check_account` probe: the prologue has resolved whatever scope
    /// the SDK saw to an account (or refused the request as
    /// `unknown_account`); this runs one durable step (`probe`) — a query of
    /// the sentinel external id — and reports that scope, the configured
    /// account, the pinned namespace and szamlazz.hu's verdict on the
    /// credentials. `scope` is what the SDK saw, not what the caller sent:
    /// `None` under a scoped call means the server did not forward the
    /// scope. Issues nothing.
    pub(super) async fn check_account_request(
        &self,
        ctx: &Context<'_>,
    ) -> Result<CheckAccountResponse, HandlerError> {
        let outcome = {
            let gateway = Arc::clone(&self.gateway);
            let external_id = ExternalId::for_probe(&self.config.namespace);
            run_once(ctx, "probe", move || async move {
                gateway.probe(&external_id).await
            })
            .await?
        };
        let credentials = credentials_check(outcome)?;
        Ok(CheckAccountResponse::new(
            ctx.scope().map(str::to_owned),
            CheckedAccount::from(self.gateway.account()),
            self.config.namespace.as_str(),
            credentials,
        ))
    }

    /// The `query` handler: one durable step (`query`) — the document as
    /// szamlazz.hu returned it — then the account check every handler that
    /// finds a document runs, and the projection. A document that is not the
    /// resolved account's is `account_mismatch`, not a projection that looks
    /// fine: on a freshly onboarded account the first found document is most
    /// likely a read, and a 409 naming the observed `teszt` and supplier id is
    /// the louder signal.
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
            QueryRun::Found(found) => {
                check_pins(self.gateway.account(), &found)?;
                Ok(QueryResponse::from(&*found))
            }
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

    /// The `set_payments` handler: one durable step (`set-payments-{number}`)
    /// that registers the credit entries without a preceding query.
    /// Deliberately the one handler without the account check of a found
    /// document: it finds none — a verify round trip (about a second per
    /// credit entry) to catch a misconfiguration every other found document
    /// already catches is not worth it, and a credit entry is not a legal
    /// document.
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

    /// The `storno` handler: verify by number, then — for a document carrying
    /// no order number that belongs to the resolved account — the lookup and
    /// storno steps of design §6 under the by-number storno external id.
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
            // `Szamlazz.Order`'s document: it checks the pins itself.
            return Ok(
                StornoResponse::new(StornoOutcome::ManagedByOrder, number).with_order_key(order)
            );
        }
        // This is the handler that issues a legal document by number: the
        // document must be the resolved account's before anything is sent.
        check_pins(self.gateway.account(), &found)?;
        if found.info.reversed == Some(true) {
            return Ok(StornoResponse::new(StornoOutcome::Reversed, number));
        }
        let e_invoice = found
            .e_invoice()
            .unwrap_or(self.gateway.account().defaults.e_invoice);
        let intent = StornoIntent {
            number: number.clone(),
            storno_id: ExternalId::for_unmanaged_storno(&self.config.namespace, &number),
            comment,
            e_invoice,
        };

        // The lookup step: a storno of ours already under the id.
        match lookup_storno(ctx, &self.gateway, &intent).await? {
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
        // execution; any `Err` from the run — exhaustion or cancellation — is
        // `outcome_unknown`, and the next call's lookup finds whatever landed.
        let outcome = storno_step(
            ctx,
            &self.gateway,
            self.config.issue.run_retry_policy(),
            &intent,
        )
        .await
        .map_err(|error| {
            Fault::outcome_unknown(format!(
                "the storno step ended without a confirmed outcome ({}): {}; call storno again",
                error.code(),
                error.message()
            ))
        })?;
        storno_response(outcome, number).map_err(|(code, message)| {
            Fault::credentials_rejected(&self.config.namespace, code, message).into()
        })
    }
}

#[cfg(test)]
mod tests {
    use restate_sdk::errors::TerminalError;

    use super::*;

    /// A wrong key is `credentials: rejected` — data — not a fault; only an
    /// exchange that settled nothing is one.
    #[test]
    fn the_probe_outcome_is_data_unless_nothing_was_settled() {
        assert_eq!(
            credentials_check(ProbeOutcome::Accepted).expect("data"),
            CredentialsCheck::Ok
        );
        assert_eq!(
            credentials_check(ProbeOutcome::CredentialsRejected {
                code: "3".to_owned(),
                message: "Sikertelen bejelentkezés.".to_owned(),
            })
            .expect("data"),
            CredentialsCheck::Rejected {
                code: "3".to_owned(),
                message: "Sikertelen bejelentkezés.".to_owned(),
            }
        );
        let error = TerminalError::from(
            credentials_check(ProbeOutcome::Transport("connection reset".to_owned()))
                .expect_err("fault"),
        );
        assert_eq!(error.code(), 503);
        let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
        assert_eq!(body["code"], "unavailable");
        assert!(
            body["message"]
                .as_str()
                .expect("message")
                .contains("check_account again"),
            "{body}"
        );
    }
}
