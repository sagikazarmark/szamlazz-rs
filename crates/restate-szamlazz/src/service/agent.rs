//! The stateless `Szamlazz.Agent` service handlers: `query` and
//! `set_payments` by document number, `query_taxpayer` by tax number, and
//! the `check_account` probe. The by-number `storno` is the storno
//! protocol's second shell, in `service::storno`.
//!
//! No handler compares the document it finds with the account the invocation
//! resolved to: the worker holds no account pin; which account a key opens
//! is the operator's go-live check. Every read (the probe, `query`,
//! `query_taxpayer`) runs under the read policy; `set_payments` is a write
//! without a retry of its own, and with `additive: true` an at-least-once
//! one (see [`SetPaymentsRequest::additive`]).

use std::sync::Arc;

use restate_sdk::errors::HandlerError;
use restate_sdk::prelude::Context;
use szamlazz_agent::ops::taxpayer::TaxpayerPrefix;

use super::prologue::Execution;
use super::support::{AnsweredCode, Fault, run_once, run_reading};
use crate::contract::{
    CheckAccountResponse, CheckedAccount, CredentialsCheck, QueryRequest, QueryResponse,
    QueryTaxpayerRequest, QueryTaxpayerResponse, SetPaymentsRequest, SetPaymentsResponse,
};
use crate::gateway::{
    ProbeOutcome, QueryOutcome, RejectionCode, SetPaymentsOutcome, SzamlazzAnswer, TaxpayerOutcome,
};
use crate::identity::{ExternalId, Namespace};

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
/// probe's purpose. (An exchange that settled nothing never reaches here: it
/// is the read's `Unanswered`, retried by the read policy and `unavailable`
/// on exhaustion.)
pub(super) fn credentials_check(outcome: ProbeOutcome) -> CredentialsCheck {
    match outcome {
        ProbeOutcome::Accepted => CredentialsCheck::Ok,
        ProbeOutcome::CredentialsRejected(answer) => CredentialsCheck::Rejected {
            code: answer.code,
            message: answer.message,
        },
    }
}

/// The `outcome_unknown` fault of `set_payments` after a lost reply. What the
/// caller does next depends on `additive`: a replacing call is idempotent and
/// is simply repeated; an additive one is at-least-once (the lost send may
/// have appended the entries), so the caller queries the invoice first.
fn set_payments_unknown(additive: bool, message: &str) -> Fault {
    let next = if additive {
        "the entries are additive and may have landed; query the invoice before re-sending"
    } else {
        "call set_payments again"
    };
    Fault::outcome_unknown(format!(
        "credit entry registration outcome unknown: {message}; {next}"
    ))
}

/// What `query` answers from what its one step settled: the projection of
/// the found document; 404 `not_found` on code 7; a credential code as
/// `credentials_rejected`; any other szamlazz.hu code passed through as
/// `szamlazz_error` (422), the code in `szamlazz_code`.
fn query_response(outcome: QueryOutcome, namespace: &Namespace) -> Result<QueryResponse, Fault> {
    match outcome {
        QueryOutcome::Found(found) => Ok(QueryResponse::from(&*found)),
        QueryOutcome::NotFound => Err(Fault::not_found(
            "szamlazz.hu does not know the document (code 7)",
        )),
        QueryOutcome::CredentialsRejected(answer) => {
            Err(AnsweredCode::CredentialsRejected(answer).into_fault(namespace))
        }
        QueryOutcome::Api(answer) => Err(AnsweredCode::PassedThrough(answer).into_fault(namespace)),
    }
}

/// What `query_taxpayer` answers from what its one step settled: NAV's record
/// as data (`valid: false` included); a credential code as
/// `credentials_rejected`; any other `funcCode ≠ OK` (szamlazz.hu's own or
/// NAV's relayed one) passed through as `szamlazz_error` (422).
fn taxpayer_response(
    outcome: TaxpayerOutcome,
    namespace: &Namespace,
) -> Result<QueryTaxpayerResponse, Fault> {
    match outcome {
        TaxpayerOutcome::Found(taxpayer) => Ok(taxpayer),
        TaxpayerOutcome::CredentialsRejected(answer) => {
            Err(AnsweredCode::CredentialsRejected(answer).into_fault(namespace))
        }
        TaxpayerOutcome::Api(answer) => {
            Err(AnsweredCode::PassedThrough(answer).into_fault(namespace))
        }
    }
}

/// What `set_payments` answers from what its one step settled: the totals on
/// success; a rejection that never reached szamlazz.hu (the wire contract
/// takes at most five entries, and a replacing request with none would clear
/// the invoice's payments, [`RejectionCode::Request`]) as `invalid_input`, the
/// caller's request; szamlazz.hu refusing the entries passed through as
/// `szamlazz_error` (422) naming the invoice; a credential code as
/// `credentials_rejected`; a lost reply as `outcome_unknown`, conditional on
/// `additive`.
fn set_payments_response(
    outcome: SetPaymentsOutcome,
    invoice_number: String,
    additive: bool,
    namespace: &Namespace,
) -> Result<SetPaymentsResponse, Fault> {
    match outcome {
        SetPaymentsOutcome::Done { outstanding, gross } => {
            let mut response = SetPaymentsResponse::new(invoice_number);
            response.outstanding = outstanding;
            response.gross_total = gross;
            Ok(response)
        }
        SetPaymentsOutcome::Rejected(rejection) => Err(match rejection.code {
            RejectionCode::Request => Fault::invalid_input(format!(
                "the credit entries cannot be sent: {}; nothing was sent",
                rejection.message
            )),
            RejectionCode::Szamlazz(code) => Fault::szamlazz_error_on(
                format!("the credit entries on invoice {invoice_number} were refused"),
                SzamlazzAnswer::new(code, rejection.message),
            ),
        }),
        SetPaymentsOutcome::CredentialsRejected(answer) => {
            Err(AnsweredCode::CredentialsRejected(answer).into_fault(namespace))
        }
        SetPaymentsOutcome::Transport(message) => Err(set_payments_unknown(additive, &message)),
    }
}

impl Execution {
    /// The `check_account` probe: the prologue has resolved whatever scope
    /// the SDK saw to an account (or refused the request as
    /// `unknown_account`); this runs one durable step (`probe`) under the
    /// read policy (a query of the sentinel external id), and reports that
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
    /// (the document as szamlazz.hu returned it, the same entry `verify`
    /// writes), then the projection. The projection carries `test` (`teszt`)
    /// as szamlazz.hu reported it: the go-live check reads it off a known
    /// document here, since the worker compares it with nothing.
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
        query_response(outcome, &self.config.namespace).map_err(HandlerError::from)
    }

    /// The `query_taxpayer` handler: one durable step (`taxpayer-{prefix}`)
    /// under the read policy (NAV's answer as szamlazz.hu relayed it,
    /// projected onto the crate-owned response), then the projection as is.
    /// `valid: false` is the answer, not a fault. Any other `funcCode ≠ OK`
    /// (szamlazz.hu's code or NAV's relayed one) is an answer: passed through
    /// as `szamlazz_error` (422, the code in `szamlazz_code`) like `query`'s,
    /// never retried; a NAV outage therefore surfaces as a terminal 422 the
    /// caller may retry with a new `Idempotency-Key`.
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
        taxpayer_response(outcome, &self.config.namespace).map_err(HandlerError::from)
    }

    /// The `set_payments` handler: one durable step (`set-payments-{number}`)
    /// that registers the credit entries without a preceding query; a verify
    /// round trip (about a second per credit entry) would establish nothing
    /// the send does not, and a credit entry is not a legal document.
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
        let invoice_number = String::from(invoice_number);
        let gateway = Arc::clone(&self.gateway);
        let number = invoice_number.clone();
        let outcome = run_once(
            ctx,
            format!("set-payments-{invoice_number}"),
            move || async move { gateway.set_payments(&number, &entries, additive).await },
        )
        .await?;
        set_payments_response(outcome, invoice_number, additive, &self.config.namespace)
            .map_err(HandlerError::from)
    }
}

#[cfg(test)]
mod tests {
    use restate_sdk::errors::TerminalError;

    use super::*;
    use crate::gateway::Rejection;

    fn namespace() -> Namespace {
        "acct".parse().expect("namespace")
    }

    fn fault_body(fault: Fault) -> (u16, serde_json::Value) {
        let error = TerminalError::from(fault);
        let body = serde_json::from_str(error.message()).expect("json body");
        (error.code(), body)
    }

    /// A sixth credit entry never reaches szamlazz.hu (the wire contract
    /// takes at most five), and is the caller's request: `invalid_input`
    /// (400) naming the limit, with no szamlazz.hu code to carry. Not a
    /// pass-through: szamlazz.hu answered nothing.
    #[test]
    fn a_sixth_credit_entry_is_invalid_input() {
        let outcome = SetPaymentsOutcome::Rejected(Rejection::request(
            "a credit-entry request can contain at most five entries",
        ));
        let fault = set_payments_response(outcome, "SZ-1".to_owned(), false, &namespace())
            .expect_err("a fault");
        let (status, body) = fault_body(fault);
        assert_eq!(status, 400, "{body}");
        assert_eq!(body["code"], "invalid_input", "{body}");
        assert_eq!(body.get("szamlazz_code"), None, "{body}");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains("at most five entries"), "{message}");
    }

    /// szamlazz.hu refusing the credit entries is its answer, passed through:
    /// `szamlazz_error` (422) with the szamlazz.hu code in `szamlazz_code`
    /// (never in `code`, which is the symbolic token), and its message.
    #[test]
    fn a_refused_credit_entry_is_a_szamlazz_error_carrying_the_code() {
        let outcome = SetPaymentsOutcome::Rejected(Rejection::from(SzamlazzAnswer::new(
            "259",
            "A számla nem található.",
        )));
        let fault = set_payments_response(outcome, "SZ-1".to_owned(), false, &namespace())
            .expect_err("a fault");
        let (status, body) = fault_body(fault);
        assert_eq!(status, 422, "{body}");
        assert_eq!(body["code"], "szamlazz_error", "{body}");
        assert_eq!(body["szamlazz_code"], "259", "{body}");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains("259"), "{message}");
        assert!(message.contains("A számla nem található."), "{message}");
        assert!(message.contains("SZ-1"), "names the invoice: {message}");
    }

    /// `query`: code 7 is 404 `not_found`; another szamlazz.hu code is the
    /// 422 pass-through with the code in `szamlazz_code`; a credential code
    /// is `credentials_rejected`.
    #[test]
    fn query_answers_a_miss_as_not_found_and_passes_another_code_through() {
        let (status, body) =
            fault_body(query_response(QueryOutcome::NotFound, &namespace()).expect_err("a fault"));
        assert_eq!(status, 404, "{body}");
        assert_eq!(body["code"], "not_found", "{body}");
        assert_eq!(body.get("szamlazz_code"), None, "{body}");

        let outcome = QueryOutcome::Api(SzamlazzAnswer::new("57", "Hibás számlaszám."));
        let (status, body) =
            fault_body(query_response(outcome, &namespace()).expect_err("a fault"));
        assert_eq!(status, 422, "{body}");
        assert_eq!(body["code"], "szamlazz_error", "{body}");
        assert_eq!(body["szamlazz_code"], "57", "{body}");
        assert!(
            body["message"]
                .as_str()
                .expect("message")
                .contains("Hibás számlaszám."),
            "{body}"
        );

        let outcome = QueryOutcome::CredentialsRejected(SzamlazzAnswer::new(
            "3",
            "Sikertelen bejelentkezés.",
        ));
        let (status, body) =
            fault_body(query_response(outcome, &namespace()).expect_err("a fault"));
        assert_eq!(status, 503, "{body}");
        assert_eq!(body["code"], "credentials_rejected", "{body}");
        assert_eq!(body["szamlazz_code"], "3", "{body}");
    }

    /// `query_taxpayer`: any `funcCode ≠ OK` (szamlazz.hu's or NAV's relayed
    /// one) is the same 422 pass-through, the code in `szamlazz_code`.
    #[test]
    fn query_taxpayer_passes_a_nav_code_through() {
        let outcome = TaxpayerOutcome::Api(SzamlazzAnswer::new(
            "NAV_ERROR",
            "A NAV szolgáltatás nem elérhető.",
        ));
        let (status, body) =
            fault_body(taxpayer_response(outcome, &namespace()).expect_err("a fault"));
        assert_eq!(status, 422, "{body}");
        assert_eq!(body["code"], "szamlazz_error", "{body}");
        assert_eq!(body["szamlazz_code"], "NAV_ERROR", "{body}");
    }

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
    /// and one journal entry: the same prefix, the same step name, so a caller
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
    /// before the prologue (nothing journaled, nothing sent) for every
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
    /// naming the step by its prefix (`taxpayer-{prefix}`), and the last
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
    /// all: it is the read's `Unanswered`, retried by the read policy and
    /// `unavailable` on exhaustion.)
    #[test]
    fn the_probe_outcome_is_data() {
        assert_eq!(
            credentials_check(ProbeOutcome::Accepted),
            CredentialsCheck::Ok
        );
        assert_eq!(
            credentials_check(ProbeOutcome::CredentialsRejected(SzamlazzAnswer::new(
                "3",
                "Sikertelen bejelentkezés."
            ))),
            CredentialsCheck::Rejected {
                code: "3".to_owned(),
                message: "Sikertelen bejelentkezés.".to_owned(),
            }
        );
    }
}
