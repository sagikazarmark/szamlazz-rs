//! The stateless `Szamlazz.Agent` service handlers: `query`, `set_payments`
//! and `storno` by document number, `query_taxpayer` by tax number, and the
//! `check_account` probe.
//!
//! `query` and `storno` check the document they find against the account the
//! invocation resolved to (`support::check_pins`) before they answer or
//! send; `set_payments` and `query_taxpayer` find no document and are
//! exempt. Every read — the probe, `query`, `query_taxpayer`, the verify and
//! the storno lookup — runs under the read policy; `set_payments` is a write
//! without a retry of its own, and with `additive: true` an at-least-once
//! one (see [`SetPaymentsRequest::additive`]).

use std::sync::Arc;

use restate_sdk::errors::HandlerError;
use restate_sdk::prelude::Context;
use szamlazz_agent::ops::taxpayer::TaxpayerPrefix;

use super::prologue::Execution;
use super::support::service::{lookup_storno, run_once, run_reading, storno_step};
use super::support::{Fault, StornoIntent, check_pins, storno_response, terminal};
use crate::contract::{
    CheckAccountResponse, CheckedAccount, CredentialsCheck, QueryRequest, QueryResponse,
    QueryTaxpayerRequest, QueryTaxpayerResponse, SetPaymentsRequest, SetPaymentsResponse,
    StornoOutcome, StornoRequest, StornoResponse,
};
use crate::gateway::{
    ProbeOutcome, QueryOutcome, SetPaymentsOutcome, StornoLookupOutcome, TaxpayerOutcome,
};
use crate::identity::ExternalId;

/// The prefix `query_taxpayer` asks NAV about, or the `invalid_input` fault
/// for a tax number in neither accepted form. Decided before the prologue:
/// the same request never succeeds, so nothing is journaled or sent for it.
pub(super) fn taxpayer_prefix(request: &QueryTaxpayerRequest) -> Result<TaxpayerPrefix, Fault> {
    request
        .prefix()
        .map_err(|error| Fault::invalid_input(error.to_string()))
}

/// The name of `query_taxpayer`'s one durable step: `taxpayer-{prefix}`. The
/// prefix, not the tax number as sent, so the stem and the full number name
/// the same entry.
pub(super) fn taxpayer_step(prefix: &TaxpayerPrefix) -> String {
    format!("taxpayer-{}", prefix.as_str())
}

/// What the probe step settled, as `check_account`'s `credentials`. Every
/// probe outcome is data: a wrong key is `rejected`, reporting it is the
/// probe's purpose. (An exchange that settled nothing never reaches here — it
/// is the read's `Unanswered`, retried by the read policy and `unavailable`
/// on exhaustion.)
pub(super) fn credentials_check(outcome: ProbeOutcome) -> CredentialsCheck {
    match outcome {
        ProbeOutcome::Accepted => CredentialsCheck::Ok,
        ProbeOutcome::CredentialsRejected { code, message } => {
            CredentialsCheck::Rejected { code, message }
        }
    }
}

/// The `outcome_unknown` fault of `set_payments` after a lost reply. What the
/// caller does next depends on `additive`: a replacing call is idempotent and
/// is simply repeated; an additive one is at-least-once — the lost send may
/// have appended the entries — so the caller queries the invoice first.
fn set_payments_unknown(additive: bool, message: &str) -> Fault {
    let next = if additive {
        "the entries are additive and may have landed — query the invoice before re-sending"
    } else {
        "call set_payments again"
    };
    Fault::outcome_unknown(format!(
        "credit entry registration outcome unknown: {message}; {next}"
    ))
}

impl Execution {
    /// The `check_account` probe: the prologue has resolved whatever scope
    /// the SDK saw to an account (or refused the request as
    /// `unknown_account`); this runs one durable step (`probe`) under the
    /// read policy — a query of the sentinel external id — and reports that
    /// scope, the configured account, the pinned namespace and szamlazz.hu's
    /// verdict on the credentials. `scope` is what the SDK saw, not what the
    /// caller sent: `None` under a scoped call means the server did not
    /// forward the scope. Issues nothing.
    pub(super) async fn check_account_request(
        &self,
        ctx: &Context<'_>,
    ) -> Result<CheckAccountResponse, HandlerError> {
        let outcome = {
            let gateway = Arc::clone(&self.gateway);
            let external_id = ExternalId::for_probe(&self.config.namespace);
            run_reading(ctx, "probe", self, move || async move {
                gateway.probe(&external_id).await
            })
            .await?
        };
        Ok(CheckAccountResponse::new(
            ctx.scope().map(str::to_owned),
            CheckedAccount::from(self.gateway.account()),
            self.config.namespace.as_str(),
            credentials_check(outcome),
        ))
    }

    /// The `query` handler: one durable step (`query`) under the read policy
    /// — the document as szamlazz.hu returned it, the same entry `verify`
    /// writes — then the account check every handler that finds a document
    /// runs, and the projection. A document that is not the resolved
    /// account's is `account_mismatch`, not a projection that looks fine: on
    /// a freshly onboarded account the first found document is most likely a
    /// read, and a 409 naming the observed `teszt` and supplier id is the
    /// louder signal.
    pub(super) async fn query_request(
        &self,
        ctx: &Context<'_>,
        request: QueryRequest,
    ) -> Result<QueryResponse, HandlerError> {
        let gateway = Arc::clone(&self.gateway);
        let selector = request.selector;
        let outcome = run_reading(ctx, "query", self, move || async move {
            gateway.query(&selector).await
        })
        .await?;
        match outcome {
            QueryOutcome::Found(found) => {
                check_pins(self.gateway.account(), &found)?;
                Ok(QueryResponse::from(&*found))
            }
            QueryOutcome::NotFound => Err(terminal(
                404,
                "not_found",
                "szamlazz.hu does not know the document (code 7)",
            )),
            QueryOutcome::CredentialsRejected { code, message } => {
                Err(Fault::credentials_rejected(&self.config.namespace, code, message).into())
            }
            QueryOutcome::Api { code, message } => Err(terminal(
                422,
                &code,
                format!("szamlazz.hu error {code}: {message}"),
            )),
        }
    }

    /// The `query_taxpayer` handler: one durable step (`taxpayer-{prefix}`)
    /// under the read policy — NAV's answer as szamlazz.hu relayed it,
    /// projected onto the crate-owned response — then the projection as is.
    /// `valid: false` is the answer, not a fault. Finds no document, so like
    /// `set_payments` it runs no account check: a taxpayer record is NAV's,
    /// not the account's, and carries no pins. Any other `funcCode ≠ OK` —
    /// szamlazz.hu's code or NAV's relayed one — is an answer: passed through
    /// as 422 like `query`'s, never retried; a NAV outage therefore surfaces
    /// as a terminal 422 the caller may retry with a new `Idempotency-Key`.
    pub(super) async fn query_taxpayer_request(
        &self,
        ctx: &Context<'_>,
        prefix: TaxpayerPrefix,
    ) -> Result<QueryTaxpayerResponse, HandlerError> {
        let gateway = Arc::clone(&self.gateway);
        let step = taxpayer_step(&prefix);
        let outcome = run_reading(ctx, step, self, move || async move {
            gateway.query_taxpayer(&prefix).await
        })
        .await?;
        match outcome {
            TaxpayerOutcome::Found(taxpayer) => Ok(taxpayer),
            TaxpayerOutcome::CredentialsRejected { code, message } => {
                Err(Fault::credentials_rejected(&self.config.namespace, code, message).into())
            }
            TaxpayerOutcome::Api { code, message } => Err(terminal(
                422,
                &code,
                format!("szamlazz.hu error {code}: {message}"),
            )),
        }
    }

    /// The `set_payments` handler: one durable step (`set-payments-{number}`)
    /// that registers the credit entries without a preceding query.
    /// Deliberately without the account check of a found document (with
    /// `query_taxpayer`, one of the two handlers exempt from it): it finds
    /// none — a verify round trip (about a second per credit entry) to catch
    /// a misconfiguration every other found document already catches is not
    /// worth it, and a credit entry is not a legal document.
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
            SetPaymentsOutcome::Transport(message) => {
                Err(set_payments_unknown(additive, &message).into())
            }
        }
    }

    /// The `storno` handler: verify by number and check the found document
    /// against the resolved account, then — for a document carrying no order
    /// number — the lookup and storno steps of design §6 under the by-number
    /// storno external id. A document carrying an order number is answered
    /// as `managed_by_order` after the check, never before it.
    pub(super) async fn storno_request(
        &self,
        ctx: &Context<'_>,
        request: StornoRequest,
    ) -> Result<StornoResponse, HandlerError> {
        let StornoRequest {
            invoice_number: number,
            comment,
        } = request;

        // Query first: everything below is about the document as found.
        let found = {
            let gateway = Arc::clone(&self.gateway);
            let number = number.clone();
            run_reading(ctx, format!("verify-{number}"), self, move || async move {
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
            QueryOutcome::Api { code, message } => {
                return Err(Fault::inconclusive_answer(code, message).into());
            }
            QueryOutcome::CredentialsRejected { code, message } => {
                return Err(
                    Fault::credentials_rejected(&self.config.namespace, code, message).into(),
                );
            }
        };
        // This is the handler that issues a legal document by number: the
        // document must be the resolved account's before anything is said or
        // sent about it — even "it is an order's": the document is in hand,
        // and another account's order number must not be echoed.
        check_pins(self.gateway.account(), &found)?;
        if let Some(order) = found
            .info
            .order_number
            .as_deref()
            .map(str::trim)
            .filter(|order| !order.is_empty())
        {
            // `Szamlazz.Order`'s document; this service never calls into it.
            return Ok(
                StornoResponse::new(StornoOutcome::ManagedByOrder, number).with_order_key(order)
            );
        }
        if found.info.reversed == Some(true) {
            return Ok(StornoResponse::new(StornoOutcome::Reversed, number));
        }
        // The intent is a pure function of the verified document: a `telj`
        // it does not carry is a fault after every answer that needs no send
        // (ADR 0007). No document type pre-check here — the echo tells.
        let intent = StornoIntent::from_verified(
            &found,
            self.gateway.account(),
            number.clone(),
            ExternalId::for_unmanaged_storno(&self.config.namespace, &number),
            comment,
        )?;

        // The lookup step: a storno of ours already under the id.
        match lookup_storno(ctx, self, &intent).await? {
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
            StornoLookupOutcome::Api { code, message } => {
                return Err(Fault::inconclusive_answer(code, message).into());
            }
        }

        // The storno step, under the issue policy: query-first on every
        // execution; any `Err` from the run — exhaustion or cancellation — is
        // `outcome_unknown`, and the next call's lookup finds whatever landed.
        let outcome = storno_step(ctx, self, &intent).await.map_err(|error| {
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

    /// `set_payments` with `additive: true` is at-least-once: a lost reply
    /// may have appended the entries, so the fault tells the caller to query
    /// the invoice before re-sending; a replacing call is repeated as is.
    #[test]
    fn the_set_payments_fault_tells_an_additive_caller_to_query_first() {
        let additive = TerminalError::from(set_payments_unknown(true, "connection reset"));
        assert_eq!(additive.code(), 500);
        assert!(
            additive.message().contains("connection reset"),
            "{}",
            additive.message()
        );
        assert!(
            additive
                .message()
                .contains("query the invoice before re-sending"),
            "{}",
            additive.message()
        );
        assert!(
            !additive.message().contains("call set_payments again"),
            "{}",
            additive.message()
        );

        let replacing = TerminalError::from(set_payments_unknown(false, "connection reset"));
        assert_eq!(replacing.code(), 500);
        assert!(
            replacing.message().contains("call set_payments again"),
            "{}",
            replacing.message()
        );
        assert!(
            !replacing.message().contains("query the invoice"),
            "{}",
            replacing.message()
        );
    }

    /// The bare stem and the full tax number are one request to szamlazz.hu
    /// and one journal entry: the same prefix, the same step name — a caller
    /// that sends the full number and one that sends the stem replay each
    /// other's step.
    #[test]
    fn a_full_tax_number_and_its_stem_name_the_same_step() {
        let stem = taxpayer_prefix(&QueryTaxpayerRequest::new("12345678")).expect("stem");
        let full = taxpayer_prefix(&QueryTaxpayerRequest::new("12345678-2-42")).expect("full");
        assert_eq!(stem, full);
        assert_eq!(taxpayer_step(&stem), "taxpayer-12345678");
        assert_eq!(taxpayer_step(&full), "taxpayer-12345678");
    }

    /// A tax number in neither accepted form is the caller's request:
    /// `invalid_input` (400) naming the input and the accepted forms, decided
    /// before the prologue — nothing journaled, nothing sent — for every
    /// rejected form alike (whitespace, a wrong length, a partial or wrong
    /// suffix, another separator, a non-digit, non-ASCII digits).
    #[test]
    fn a_tax_number_in_neither_form_is_invalid_input() {
        for tax_number in [
            "",
            "1234567",
            "123456789",
            " 12345678",
            "12345678 ",
            "12345678-2",
            "12345678-2-4",
            "12345678-2-423",
            "12345678-24-2",
            "12345678_2_42",
            "1234567a",
            "12345678-a-42",
            "１２３４５６７８",
            "12 345 678",
        ] {
            let fault =
                taxpayer_prefix(&QueryTaxpayerRequest::new(tax_number)).expect_err(tax_number);
            let error = TerminalError::from(fault);
            assert_eq!(error.code(), 400, "{tax_number:?}");
            let body: serde_json::Value = serde_json::from_str(error.message()).expect("json");
            assert_eq!(body["code"], "invalid_input", "{tax_number:?}: {body}");
            let message = body["message"].as_str().expect("message");
            assert!(
                message.contains(&format!("{tax_number:?}")),
                "{tax_number:?} is named: {message}"
            );
            assert!(
                message.contains("(12345678)") && message.contains("(12345678-2-42)"),
                "{tax_number:?} names both accepted forms: {message}"
            );
            assert_eq!(
                body.get("order"),
                None,
                "a by-number fault carries no order"
            );
        }
    }

    /// An exhausted read of the taxpayer step is the `unavailable` fault
    /// naming the step by its prefix — `taxpayer-{prefix}` — and the last
    /// failure, never the tax number as the caller sent it.
    #[test]
    fn an_exhausted_taxpayer_read_is_unavailable_naming_the_step() {
        use super::super::support::read_exhausted;

        let prefix = taxpayer_prefix(&QueryTaxpayerRequest::new("12345678-2-42")).expect("full");
        let last = TerminalError::new_with_code(500, "szamlazz.hu is unavailable: maintenance");
        let error = TerminalError::from(read_exhausted(&taxpayer_step(&prefix), &last));
        assert_eq!(error.code(), 503);
        let body: serde_json::Value = serde_json::from_str(error.message()).expect("json");
        assert_eq!(body["code"], "unavailable", "{body}");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains("taxpayer-12345678"), "{message}");
        assert!(
            message.contains("maintenance"),
            "names the last failure: {message}"
        );
        assert!(
            !message.contains("12345678-2-42"),
            "the step, not the input: {message}"
        );
    }

    /// Every probe outcome is data: a wrong key is `credentials: rejected`,
    /// not a fault. (An exchange that settled nothing is not an outcome at
    /// all — it is the read's `Unanswered`, retried by the read policy and
    /// `unavailable` on exhaustion.)
    #[test]
    fn the_probe_outcome_is_data() {
        assert_eq!(
            credentials_check(ProbeOutcome::Accepted),
            CredentialsCheck::Ok
        );
        assert_eq!(
            credentials_check(ProbeOutcome::CredentialsRejected {
                code: "3".to_owned(),
                message: "Sikertelen bejelentkezés.".to_owned(),
            }),
            CredentialsCheck::Rejected {
                code: "3".to_owned(),
                message: "Sikertelen bejelentkezés.".to_owned(),
            }
        );
    }
}
