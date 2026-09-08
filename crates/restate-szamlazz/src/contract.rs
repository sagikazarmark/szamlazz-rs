//! SDK-independent request and response contract of the `Szamlazz.Order` and
//! `Szamlazz.Agent` services.
//!
//! Everything here is plain data with a stable JSON shape: domain outcomes are
//! returned as values with HTTP 200 (see [`Outcome`] and [`ConflictReason`]),
//! while the [`TerminalCode`]s are reserved for faults. Three of the seven
//! codes mean "outcome unknown: retry with a new `Idempotency-Key`, or read
//! `Szamlazz.Order.get`" (`outcome_unknown`,
//! `unavailable`, `credentials_rejected`); the rest are settled: the same
//! request never succeeds, or szamlazz.hu's own answer is passed through
//! ([`TerminalCode`] says which). The module depends on
//! [`identity`](crate::identity) alone (one way: `identity` imports nothing
//! of it) and compiles without `restate-sdk`; with the `schemars` feature the
//! types also derive JSON Schemas for the `OpenAPI` export.
//!
//! Every request type refuses a field it does not know
//! (`#[serde(deny_unknown_fields)]`, `additionalProperties: false` in the
//! schema): a misspelt `reissue` or `additive` is an error naming the field,
//! never a silent default. Response types stay open: a client must tolerate
//! fields added later.
//!
//! The submodules mirror the handler modules of [`service`](crate::service),
//! one per handler family, each holding its requests beside its responses:
//!
//! - [`document`]: the per-call document input (buyer, line items, payment
//!   method, overrides) and its conversion to `szamlazz_agent` types; shared
//!   by every issuing handler.
//! - [`create`]: the issuing handlers: `create_proforma`, `create_invoice`,
//!   `create_prepayment`, `create_final` and `correct_invoice`.
//! - [`storno`]: `storno_invoice`, `delete_proforma` and the `get` live view
//!   ([`OrderStatus`]); the storno contract is shared with
//!   `Szamlazz.Agent.storno`.
//! - [`agent`]: the rest of `Szamlazz.Agent`: `query`, `query_taxpayer`,
//!   `set_payments` and `check_account`.

use std::fmt;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

pub mod agent;
pub mod create;
pub mod document;
pub mod storno;

pub use agent::{
    CheckAccountResponse, CheckedAccount, CredentialsCheck, InvalidTaxNumber, PaymentEntry,
    PaymentRecord, QueryRequest, QueryResponse, QueryTaxpayerRequest, QueryTaxpayerResponse,
    Selector, SetPaymentsRequest, SetPaymentsResponse, TaxpayerAddress,
};
pub use create::{
    ConflictReason, CorrectRequest, CreateOptions, CreateRequest, CreateResponse, Outcome,
    ProformaLink, Warning,
};
pub use document::{
    BuyerInput, DocumentInput, DocumentOverrides, ExchangeRateInput, LineItemInput, PaymentMethod,
    PostalAddressInput, TaxpayerStatus,
};
pub use storno::{
    DeleteProformaRequest, DeleteProformaResponse, DocumentState, DocumentStatus, OrderStatus,
    StornoOutcome, StornoRequest, StornoResponse,
};

/// The caller-supplied identities the requests carry, defined in
/// [`identity`](crate::identity) because each is a segment of the external
/// id: re-exported here as part of the contract.
pub use crate::identity::{
    CorrectionId, DocumentKind, InvalidCorrectionId, InvalidInvoiceNumber, InvoiceNumber,
    IssuedKind,
};

use crate::identity::{ExternalId, OrderKey};

/// The code of a `TerminalError` any handler of either service may raise.
///
/// Every fault either service raises carries one of these tokens in `code`,
/// with the HTTP status of [`status`](Self::status). Three of them mean
/// "outcome unknown: retry with a new `Idempotency-Key`, or read
/// `Szamlazz.Order.get`": `outcome_unknown`, `unavailable` and
/// `credentials_rejected`. The rest are settled: the same request never
/// succeeds (`invalid_input`, `unknown_account`, `not_found`) or
/// szamlazz.hu's own answer is passed through
/// (`szamlazz_error`, whose szamlazz.hu code travels in the fault's separate
/// `szamlazz_code` field; `code` is always one of these tokens).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TerminalCode {
    /// The create or storno step ran out of the issue policy while a document
    /// may or may not have been issued; the next call's external-id query
    /// finds whatever landed. HTTP 500.
    OutcomeUnknown,
    /// szamlazz.hu did not answer a read-only step through every execution
    /// the read policy allows (or answered it with a code nothing can be
    /// concluded from), or the account resolver or credential store could not
    /// answer. Nothing was sent by the execution that raised it. HTTP 503.
    Unavailable,
    /// The caller's request: a malformed body, an untrimmed order key, a tax
    /// number in neither accepted form, an option the handler does not take,
    /// or a request the wire contract cannot carry (a sixth credit entry).
    /// The same request never succeeds. HTTP 400.
    InvalidInput,
    /// szamlazz.hu rejected the account's agent credentials (codes 3, 135,
    /// 136, 164): the worker's configuration is wrong, not the request. The
    /// request that drew the code was not acted on (szamlazz.hu answers
    /// these codes before acting), but the code may have come to a post-send
    /// re-query, and an earlier execution may have landed with a lost reply,
    /// which is why this is a fault and not a `rejected` outcome. Fix the
    /// key, then retry with a new `Idempotency-Key`. HTTP 503.
    CredentialsRejected,
    /// The request names no account of this deployment: it arrived unscoped
    /// where accounts are reachable by scope only, or under a scope no account
    /// is reachable by. Raised before anything is issued; the same request
    /// never succeeds, so the caller must fix the scope, not retry. HTTP 400.
    UnknownAccount,
    /// The document the request names by number is not known to szamlazz.hu
    /// (code 7): `Szamlazz.Agent.query`'s selector, the invoice of
    /// `Szamlazz.Agent.storno` / `Szamlazz.Order.storno_invoice`, the base of
    /// `correct_invoice`. Nothing was sent; the same request never succeeds.
    /// HTTP 404.
    NotFound,
    /// szamlazz.hu answered the request with an error code of its own that
    /// the handler passes through rather than concludes from: on
    /// `Szamlazz.Agent.query`, `query_taxpayer` (szamlazz.hu's code or NAV's
    /// relayed one) and `set_payments` (the credit entries refused). The
    /// szamlazz.hu code is in the fault's `szamlazz_code`, the message is
    /// szamlazz.hu's. HTTP 422.
    SzamlazzError,
}

impl TerminalCode {
    /// Every code, in the order of the fault tables the READMEs carry.
    /// (`account_mismatch`, 409, was the eighth until the account pins were
    /// dropped: no handler compares a found document with the account any
    /// more, so nothing could raise it.)
    pub const ALL: [Self; 7] = [
        Self::InvalidInput,
        Self::UnknownAccount,
        Self::NotFound,
        Self::SzamlazzError,
        Self::OutcomeUnknown,
        Self::Unavailable,
        Self::CredentialsRejected,
    ];

    /// The snake-case token carried in the error.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OutcomeUnknown => "outcome_unknown",
            Self::Unavailable => "unavailable",
            Self::InvalidInput => "invalid_input",
            Self::CredentialsRejected => "credentials_rejected",
            Self::UnknownAccount => "unknown_account",
            Self::NotFound => "not_found",
            Self::SzamlazzError => "szamlazz_error",
        }
    }

    /// The HTTP status the ingress reports for a fault with this code.
    #[must_use]
    pub const fn status(self) -> u16 {
        match self {
            // The caller's request: the same request never succeeds.
            Self::InvalidInput | Self::UnknownAccount => 400,
            Self::NotFound => 404,
            // szamlazz.hu's own answer, passed through.
            Self::SzamlazzError => 422,
            Self::OutcomeUnknown => 500,
            // The worker's misconfiguration or szamlazz.hu not answering, not
            // the caller's request: the same request succeeds once the key
            // is fixed or szamlazz.hu answers, so neither a 4xx ("do not
            // retry") nor 401/403 ("you are unauthenticated") fits.
            Self::Unavailable | Self::CredentialsRejected => 503,
        }
    }
}

impl fmt::Display for TerminalCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The body of a fault: what a `TerminalError` either service raises carries,
/// as the caller receives it inside Restate's ingress envelope.
///
/// `code` is always a [`TerminalCode`] token (a szamlazz.hu code never
/// travels in it); `szamlazz_code` is the szamlazz.hu code when szamlazz.hu's
/// answer is what the fault is about; `order`, `kind` and `external_id`
/// identify the document the fault is about when the handler knows one (the
/// `Szamlazz.Order` handlers' faults; a by-number fault of `Szamlazz.Agent`
/// carries none). The HTTP status the ingress reports is the code's,
/// [`TerminalCode::status`].
///
/// On the wire the fault is the JSON **string** inside Restate's ingress
/// envelope: `{"code": <HTTP status>, "message": "<fault JSON>", "source":
/// "invocation"}` under `x-restate-error-source: invocation` (server 1.7.8;
/// the SDK carries a terminal error as a code and a message and offers no
/// other channel), so a caller parses `message` a second time, into this
/// type. A response type: open (a client tolerates fields added later) and
/// `#[non_exhaustive]`, built with [`Fault::new`] and the setters; the
/// optional fields are omitted when absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct Fault {
    /// The fault's code: one of the seven [`TerminalCode`] tokens.
    pub code: TerminalCode,
    /// What happened and what the caller does next, in prose.
    pub message: String,
    /// The szamlazz.hu code, when szamlazz.hu's answer is what the fault is
    /// about (`szamlazz_error` always; `credentials_rejected`; `unavailable`
    /// on an inconclusive code).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub szamlazz_code: Option<String>,
    /// The order the fault is about (the `Szamlazz.Order` key), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
    /// The kind of the document the fault is about, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<IssuedKind>,
    /// The external id of the document the fault is about, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
}

impl Fault {
    /// A fault of `code` with `message` and nothing else.
    pub fn new(code: TerminalCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            szamlazz_code: None,
            order: None,
            kind: None,
            external_id: None,
        }
    }

    /// The same fault carrying the szamlazz.hu code its answer had.
    #[must_use]
    pub fn with_szamlazz_code(mut self, szamlazz_code: impl Into<String>) -> Self {
        self.szamlazz_code = Some(szamlazz_code.into());
        self
    }

    /// The same fault about the document identified by `order` (the
    /// `Szamlazz.Order` key), `kind` and `external_id`.
    #[must_use]
    pub fn about(
        mut self,
        order: &OrderKey,
        kind: Option<IssuedKind>,
        external_id: &ExternalId,
    ) -> Self {
        self.order = Some(order.as_str().to_owned());
        self.kind = kind;
        self.external_id = Some(external_id.as_str().to_owned());
        self
    }

    /// The HTTP status the ingress reports for the fault: its code's.
    #[must_use]
    pub const fn status(&self) -> u16 {
        self.code.status()
    }
}

/// `gross − Σ payments`, when the gross total is known and the arithmetic
/// fits a decimal. The one definition of the outstanding amount both
/// `create_*` and `query` report. Checked: the amounts are szamlazz.hu's, but
/// a panic would run on the SDK's connection task.
pub(crate) fn outstanding(gross: Option<Decimal>, payments: &[Decimal]) -> Option<Decimal> {
    let paid = payments
        .iter()
        .try_fold(Decimal::ZERO, |sum, amount| sum.checked_add(*amount))?;
    gross?.checked_sub(paid)
}

#[cfg(test)]
mod tests {
    use rust_decimal::dec;

    use super::*;

    #[test]
    fn outstanding_needs_a_gross_total() {
        assert_eq!(outstanding(None, &[dec!(1)]), None);
        assert_eq!(outstanding(Some(dec!(100)), &[]), Some(dec!(100)));
        assert_eq!(
            outstanding(Some(dec!(100)), &[dec!(30), dec!(80)]),
            Some(dec!(-10))
        );
    }

    /// The amounts come from szamlazz.hu, but a sum that does not fit a
    /// decimal is `None` (unknown), never a panic on the connection task.
    #[test]
    fn outstanding_that_does_not_fit_a_decimal_is_unknown_not_a_panic() {
        assert_eq!(
            outstanding(Some(dec!(1)), &[Decimal::MAX, Decimal::MAX]),
            None
        );
        assert_eq!(outstanding(Some(Decimal::MAX), &[Decimal::MIN]), None);
    }

    /// The bound reaches every request that names a document by number, and
    /// it is applied where the body is decoded: a refused number is a
    /// *Malformed body*, `invalid_input` before the Prologue with serde's
    /// message naming the rule.
    #[test]
    fn every_by_number_request_refuses_an_invoice_number_outside_the_bound() {
        fn refused<T: serde::de::DeserializeOwned + std::fmt::Debug>(
            name: &str,
            body: serde_json::Value,
        ) {
            let error = serde_json::from_value::<T>(body)
                .expect_err(name)
                .to_string();
            assert!(
                error.contains("invoice number is 41 bytes long, at most 40 are allowed"),
                "{name}: names the rule: {error}"
            );
        }

        let too_long = "x".repeat(InvoiceNumber::MAX_LEN + 1);
        let document = serde_json::to_value(document::tests::sample_document()).expect("json");
        refused::<StornoRequest>(
            "StornoRequest",
            serde_json::json!({"invoice_number": too_long}),
        );
        refused::<CorrectRequest>(
            "CorrectRequest",
            serde_json::json!({
                "invoice_number": too_long, "correction_id": "c-1", "document": document
            }),
        );
        refused::<SetPaymentsRequest>(
            "SetPaymentsRequest",
            serde_json::json!({"invoice_number": too_long, "entries": []}),
        );
        refused::<QueryRequest>(
            "QueryRequest",
            serde_json::json!({"selector": {"invoice_number": too_long}}),
        );
        refused::<CreateRequest>(
            "CreateRequest.options.proforma",
            serde_json::json!({
                "document": document, "options": {"proforma": {"number": too_long}}
            }),
        );
        let ok: StornoRequest =
            serde_json::from_value(serde_json::json!({"invoice_number": "SZ-1"})).expect("valid");
        assert_eq!(ok.invoice_number.as_str(), "SZ-1");
        assert_eq!(
            serde_json::to_value(&ok).expect("json")["invoice_number"],
            "SZ-1",
            "serialises as the plain string"
        );
    }

    /// The fault body is the public contract: it round-trips through JSON
    /// with the optional fields omitted when absent, its status is its
    /// code's, and a fault written by the services reads back as this type
    /// (what the e2e harness decodes).
    #[test]
    fn fault_round_trips_and_omits_absent_fields() {
        let bare = Fault::new(TerminalCode::InvalidInput, "malformed request body");
        let json = serde_json::to_value(&bare).expect("json");
        assert_eq!(
            json,
            serde_json::json!({"code": "invalid_input", "message": "malformed request body"})
        );
        assert_eq!(bare.status(), 400);
        assert_eq!(serde_json::from_value::<Fault>(json).expect("back"), bare);

        let about = Fault::new(TerminalCode::NotFound, "invoice SZ-9 is not known (code 7)")
            .with_szamlazz_code("7")
            .about(
                &OrderKey::parse("ORD-1").expect("key"),
                Some(IssuedKind::Invoice),
                &ExternalId::new("acct:ORD-1:invoice"),
            );
        let json = serde_json::to_value(&about).expect("json");
        assert_eq!(
            json,
            serde_json::json!({
                "code": "not_found",
                "message": "invoice SZ-9 is not known (code 7)",
                "szamlazz_code": "7",
                "order": "ORD-1",
                "kind": "invoice",
                "external_id": "acct:ORD-1:invoice",
            })
        );
        assert_eq!(about.status(), 404);
        assert_eq!(serde_json::from_value::<Fault>(json).expect("back"), about);

        // Open: a field added later does not fail an older reader.
        let newer: Fault = serde_json::from_value(serde_json::json!({
            "code": "unavailable", "message": "m", "hint": "later"
        }))
        .expect("tolerates unknown fields");
        assert_eq!(newer.code, TerminalCode::Unavailable);
    }

    /// Every fault either service raises is one of these seven codes, each
    /// with the HTTP status the ingress reports for it; the token is the
    /// snake-case variant name and round-trips through serde.
    #[test]
    fn terminal_code_tokens() {
        let expected = [
            (TerminalCode::OutcomeUnknown, "outcome_unknown", 500),
            (TerminalCode::Unavailable, "unavailable", 503),
            (TerminalCode::InvalidInput, "invalid_input", 400),
            (
                TerminalCode::CredentialsRejected,
                "credentials_rejected",
                503,
            ),
            (TerminalCode::UnknownAccount, "unknown_account", 400),
            (TerminalCode::NotFound, "not_found", 404),
            (TerminalCode::SzamlazzError, "szamlazz_error", 422),
        ];
        assert_eq!(TerminalCode::ALL.len(), expected.len());
        for (code, token, status) in expected {
            assert!(TerminalCode::ALL.contains(&code), "{token} is in ALL");
            assert_eq!(code.as_str(), token);
            assert_eq!(code.status(), status, "{token}");
            let json = serde_json::to_string(&code).expect("serialize");
            assert_eq!(json, format!("\"{token}\""));
            assert_eq!(
                serde_json::from_str::<TerminalCode>(&json).expect("deserialize"),
                code
            );
        }
    }

    /// The crate README's fault table lists every code with its status, as a
    /// row `` | `code` | status | ``. A new variant fails here until the
    /// table carries it.
    #[test]
    fn every_terminal_code_is_in_the_fault_table() {
        let readme = include_str!("../README.md");
        for code in TerminalCode::ALL {
            let row = format!("| `{}` | {} |", code.as_str(), code.status());
            assert!(
                readme.contains(&row),
                "README.md lists `{}` with status {}",
                code.as_str(),
                code.status()
            );
        }
    }

    /// A by-number request's schema references the bounded invoice-number
    /// type (whose own schema is pinned in `identity`).
    #[cfg(feature = "schemars")]
    #[test]
    fn by_number_request_schemas_reference_the_bounded_invoice_number() {
        let storno = serde_json::to_value(schemars::schema_for!(StornoRequest)).expect("json");
        assert!(
            storno["$defs"]["InvoiceNumber"].is_object(),
            "the request schema references the bounded type: {storno}"
        );
    }

    #[cfg(feature = "schemars")]
    #[test]
    fn contract_types_have_schemas() {
        let schema = schemars::schema_for!(CreateRequest);
        let json = serde_json::to_value(&schema).expect("serialize");
        assert_eq!(json["title"], "CreateRequest");
        assert!(json["properties"]["document"].is_object());
        assert!(json["properties"]["options"].is_object());
        assert!(json["$defs"]["DocumentInput"].is_object());

        let correct = serde_json::to_value(schemars::schema_for!(CorrectRequest)).expect("json");
        assert!(correct["properties"]["correction_id"].is_object());
        assert!(correct["$defs"]["CorrectionId"].is_object());

        for schema in [
            schemars::schema_for!(CorrectRequest),
            schemars::schema_for!(StornoRequest),
            schemars::schema_for!(DeleteProformaRequest),
            schemars::schema_for!(QueryRequest),
            schemars::schema_for!(SetPaymentsRequest),
            schemars::schema_for!(CreateResponse),
            schemars::schema_for!(StornoResponse),
            schemars::schema_for!(DeleteProformaResponse),
            schemars::schema_for!(SetPaymentsResponse),
            schemars::schema_for!(QueryResponse),
            schemars::schema_for!(OrderStatus),
        ] {
            serde_json::to_string(&schema).expect("schema serializes");
        }

        let status = serde_json::to_value(schemars::schema_for!(OrderStatus)).expect("serialize");
        assert!(status["properties"]["final"].is_object());
        assert!(status["$defs"]["DocumentStatus"].is_object());
    }

    /// Every request type's schema (and every object it nests, the object
    /// variants of its enums included) is closed (`additionalProperties:
    /// false`), so the `OpenAPI` export tightens with the code. Response
    /// schemas stay open: a client must tolerate fields added later.
    #[cfg(feature = "schemars")]
    #[test]
    fn request_schemas_are_closed_and_response_schemas_are_open() {
        /// Every object schema under `schema` (the root, each `$defs` entry
        /// and each object variant of a `oneOf`) as `(title, schema)`.
        fn objects(schema: &serde_json::Value) -> Vec<(String, &serde_json::Value)> {
            fn is_object(schema: &serde_json::Value) -> bool {
                schema["type"] == "object" || schema["properties"].is_object()
            }
            fn collect<'a>(
                title: &str,
                schema: &'a serde_json::Value,
                found: &mut Vec<(String, &'a serde_json::Value)>,
            ) {
                if is_object(schema) {
                    found.push((title.to_owned(), schema));
                }
                if let Some(variants) = schema["oneOf"].as_array() {
                    for (index, variant) in variants.iter().enumerate() {
                        collect(&format!("{title}/oneOf/{index}"), variant, found);
                    }
                }
            }
            let title = schema["title"].as_str().unwrap_or("<root>");
            let mut found = Vec::new();
            collect(title, schema, &mut found);
            if let Some(defs) = schema["$defs"].as_object() {
                for (name, def) in defs {
                    collect(name, def, &mut found);
                }
            }
            found
        }

        let requests = [
            ("CreateRequest", schemars::schema_for!(CreateRequest)),
            ("CorrectRequest", schemars::schema_for!(CorrectRequest)),
            ("StornoRequest", schemars::schema_for!(StornoRequest)),
            (
                "DeleteProformaRequest",
                schemars::schema_for!(DeleteProformaRequest),
            ),
            ("QueryRequest", schemars::schema_for!(QueryRequest)),
            (
                "QueryTaxpayerRequest",
                schemars::schema_for!(QueryTaxpayerRequest),
            ),
            (
                "SetPaymentsRequest",
                schemars::schema_for!(SetPaymentsRequest),
            ),
        ];
        let mut nested = std::collections::BTreeSet::new();
        for (name, schema) in &requests {
            let json = serde_json::to_value(schema).expect("serialize");
            assert_eq!(json["title"], *name);
            for (title, object) in objects(&json) {
                assert_eq!(
                    object["additionalProperties"],
                    serde_json::Value::Bool(false),
                    "{name}: {title} must refuse unknown fields: {object}"
                );
                nested.insert(title);
            }
        }
        // The nested request objects are all there (none slipped through as
        // a bare `properties` map without the guard), and so are the object
        // variants of the request enums (`{"number": …}`, `{"other": …}`,
        // the selectors).
        for expected in [
            "CreateOptions",
            "DocumentInput",
            "BuyerInput",
            "PostalAddressInput",
            "LineItemInput",
            "DocumentOverrides",
            "ExchangeRateInput",
            "PaymentEntry",
            "ProformaLink/oneOf/2",
            "PaymentMethod/oneOf/7",
            "Selector/oneOf/0",
            "Selector/oneOf/1",
            "Selector/oneOf/2",
        ] {
            assert!(nested.contains(expected), "{expected} not in {nested:?}");
        }

        for schema in [
            schemars::schema_for!(CreateResponse),
            schemars::schema_for!(StornoResponse),
            schemars::schema_for!(QueryResponse),
            schemars::schema_for!(OrderStatus),
            schemars::schema_for!(CheckAccountResponse),
            schemars::schema_for!(QueryTaxpayerResponse),
        ] {
            let json = serde_json::to_value(&schema).expect("serialize");
            for (title, object) in objects(&json) {
                assert_eq!(
                    object.get("additionalProperties"),
                    None,
                    "{title} is a response and stays open: {object}"
                );
            }
        }
    }

    /// The doc comments of the contract types become the `description`s of
    /// the discovery manifest and the `OpenAPI` export, which render them as
    /// prose: rustdoc's link syntax, ``[`Type`]``, ``[text](path)``, would
    /// appear verbatim there. Every description of every schema (the root's,
    /// each property's, each `$defs` entry's and each enum variant's) is
    /// plain prose.
    #[cfg(feature = "schemars")]
    #[test]
    fn schema_descriptions_carry_no_rustdoc_link_syntax() {
        /// Every `(path, description)` under `value`.
        fn descriptions(path: &str, value: &serde_json::Value, found: &mut Vec<(String, String)>) {
            match value {
                serde_json::Value::Object(map) => {
                    for (key, child) in map {
                        let child_path = format!("{path}/{key}");
                        if key == "description"
                            && let Some(text) = child.as_str()
                        {
                            found.push((child_path, text.to_owned()));
                        } else {
                            descriptions(&child_path, child, found);
                        }
                    }
                }
                serde_json::Value::Array(items) => {
                    for (index, item) in items.iter().enumerate() {
                        descriptions(&format!("{path}/{index}"), item, found);
                    }
                }
                _ => {}
            }
        }

        let schemas = [
            ("CreateRequest", schemars::schema_for!(CreateRequest)),
            ("CorrectRequest", schemars::schema_for!(CorrectRequest)),
            ("StornoRequest", schemars::schema_for!(StornoRequest)),
            (
                "DeleteProformaRequest",
                schemars::schema_for!(DeleteProformaRequest),
            ),
            ("QueryRequest", schemars::schema_for!(QueryRequest)),
            (
                "QueryTaxpayerRequest",
                schemars::schema_for!(QueryTaxpayerRequest),
            ),
            (
                "SetPaymentsRequest",
                schemars::schema_for!(SetPaymentsRequest),
            ),
            ("CreateResponse", schemars::schema_for!(CreateResponse)),
            ("StornoResponse", schemars::schema_for!(StornoResponse)),
            (
                "DeleteProformaResponse",
                schemars::schema_for!(DeleteProformaResponse),
            ),
            ("QueryResponse", schemars::schema_for!(QueryResponse)),
            (
                "QueryTaxpayerResponse",
                schemars::schema_for!(QueryTaxpayerResponse),
            ),
            (
                "SetPaymentsResponse",
                schemars::schema_for!(SetPaymentsResponse),
            ),
            (
                "CheckAccountResponse",
                schemars::schema_for!(CheckAccountResponse),
            ),
            ("OrderStatus", schemars::schema_for!(OrderStatus)),
            ("DocumentKind", schemars::schema_for!(DocumentKind)),
            ("IssuedKind", schemars::schema_for!(IssuedKind)),
        ];
        let mut leaks = Vec::new();
        for (name, schema) in &schemas {
            let json = serde_json::to_value(schema).expect("serialize");
            let mut found = Vec::new();
            descriptions(name, &json, &mut found);
            assert!(!found.is_empty(), "{name}: the walk found no description");
            for (path, text) in found {
                if ["[`", "](", "`]"]
                    .iter()
                    .any(|marker| text.contains(marker))
                {
                    leaks.push(format!("{path}: {text}"));
                }
            }
        }
        assert!(
            leaks.is_empty(),
            "rustdoc link syntax leaks into these OpenAPI descriptions:\n{}",
            leaks.join("\n---\n")
        );
    }
}
