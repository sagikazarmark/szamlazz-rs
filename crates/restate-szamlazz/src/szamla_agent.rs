//! The low-level layer: owns the [`szamlazz_agent::Client`] and the account
//! configuration and exposes the Számla Agent operations as plain async
//! functions with **outcome as data** — an expected szamlazz.hu outcome
//! (rejection, duplicate order number, not found, no-op storno) is a value,
//! never an `Err`.
//!
//! `Order` calls these inside `ctx.run`; the `SzamlaAgent` Restate service is
//! a thin facade over the same functions. Neither Restate service calls the
//! other.
//!
//! Every query result is validated before it is called ours (design §3):
//! external ids are not unique server-side and the order-number hint returns
//! the most recently issued document of any kind.
//!
//! Tracing events carry external ids, kinds, numbers and codes — never buyer
//! data.
//!
//! The outcome types derive `serde` so that the Restate services can journal
//! them as the result of a `ctx.run`.

use std::sync::Arc;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use szamlazz_agent::client::BuildError;
use szamlazz_agent::ops::credit_entry::{CreditEntries, CreditEntry, RegisterCreditEntry};
use szamlazz_agent::ops::invoice::CreateInvoice;
use szamlazz_agent::ops::proforma::{DeleteProforma, ProformaSelector};
use szamlazz_agent::ops::query_pdf::InvoiceSelector;
use szamlazz_agent::ops::query_xml::{InvoiceAppearance, InvoiceDocument, QueryInvoiceXml};
use szamlazz_agent::ops::storno::StornoInvoice;
use szamlazz_agent::{Client, ClientError, Credentials, ErrorCode, InvoiceNumber};
use tracing::Instrument as _;

use crate::config::Config;
use crate::contract::{DocumentKind, IssuedKind, PaymentEntry, Selector};
use crate::identity::{ExternalId, OrderKey};

pub mod build;

pub use build::{DocumentRefs, InputError, gross_total};

/// The low-level Számla Agent layer of one deployment.
#[derive(Debug, Clone)]
pub struct SzamlaAgent {
    client: Client,
    config: Arc<Config>,
}

/// One issuing attempt (design §6 step 4): pre-query by external id, optional
/// order-number hint, create, and the duplicate-order-number re-query.
#[derive(Debug, Clone)]
pub struct IssueRequest<'a> {
    /// The external id the document carries and is looked up by.
    pub external_id: &'a ExternalId,
    /// The kind being issued; a found document must have the matching `tipus`.
    pub kind: IssuedKind,
    /// The order the document belongs to.
    pub order: &'a OrderKey,
    /// The create request built by [`SzamlaAgent::build_create`].
    pub create: &'a CreateInvoice,
    /// Query the order number before creating to detect foreign documents and
    /// to adopt a conversion of our proforma.
    pub check_hint: bool,
    /// Every number the ledger knows as ours; hint results among them are
    /// ignored.
    pub our_numbers: &'a [String],
    /// The proforma number recorded in the ledger, when one is live; a hint
    /// document of the issued kind that references it is adopted.
    pub our_proforma: Option<&'a str>,
    /// The supplier id to expect on a found document, when known.
    pub expect_supplier_id: Option<u64>,
    /// The `teszt` flag to expect on a found document.
    pub expect_test: bool,
}

/// The result of one issuing attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueOutcome {
    /// szamlazz.hu issued the document (or replayed a byte-identical earlier
    /// create — indistinguishable and committed either way).
    Issued(IssuedDocument),
    /// A document of ours was found before or instead of creating: under the
    /// external id, or (with `adopted`) under the order number referencing
    /// our proforma. The caller checks `reversed` before committing.
    Found(FoundDocument),
    /// The external id resolves to a document that fails validation (another
    /// order, kind, account mode or supplier). Nothing was created.
    Collision(FoundDocument),
    /// A live invoice-kind document the ledger does not own exists under the
    /// order number. Nothing was created.
    Foreign(FoundDocument),
    /// szamlazz.hu refused the order number as a duplicate (71/152) and the
    /// external id re-query found nothing.
    DuplicateOrderNumber {
        /// The szamlazz.hu code (`71` or `152`).
        code: String,
        /// The szamlazz.hu message.
        message: String,
    },
    /// szamlazz.hu refused the document; nothing was created.
    Rejected {
        /// The szamlazz.hu code.
        code: String,
        /// The szamlazz.hu message.
        message: String,
    },
    /// The attempt may or may not have issued a document (codes 1, 55, 56
    /// without a number, or `szlahu_down`); re-query before retrying.
    Unknown {
        /// The szamlazz.hu code, when one was reported.
        code: Option<String>,
        /// What was reported.
        message: String,
    },
    /// The HTTP exchange or the response parse failed; the outcome of any
    /// create is unknown.
    Transport(String),
}

/// A document szamlazz.hu issued in response to a create.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct IssuedDocument {
    /// The document number.
    pub number: String,
    /// Net total.
    pub net: Option<Decimal>,
    /// Gross total.
    pub gross: Option<Decimal>,
    /// Outstanding amount.
    pub outstanding: Option<Decimal>,
    /// Buyer-facing account URL.
    pub customer_account_url: Option<String>,
    /// szamlazz.hu's document id (`szlahu_id`).
    pub document_id: Option<u64>,
    /// The document was issued but its notification could not be delivered
    /// (code 56 with a number).
    pub notification_delivery_failed: bool,
}

/// The ledger-relevant projection of a queried document. Carries no buyer
/// data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct FoundDocument {
    /// The document number.
    pub number: String,
    /// The `tipus` code: `SZ`, `D`, `ES`, `VS`, `SS`, `HS`, `SL`, ….
    pub document_type: String,
    /// `<sztornozott>`; `None` means live (or a storno document).
    pub reversed: Option<bool>,
    /// The order number.
    pub order_number: Option<String>,
    /// The proforma this document converted (`hivdijbekszam`).
    pub referenced_proforma: Option<String>,
    /// The invoice this document references (`hivszamlaszam`): the reversed
    /// original of a storno, the corrected one of a corrective, the settled
    /// prepayment of a final invoice.
    pub referenced_invoice: Option<String>,
    /// Gross total.
    pub gross: Option<Decimal>,
    /// Net total.
    pub net: Option<Decimal>,
    /// Issued from a test account.
    pub test: bool,
    /// Whether it is an e-invoice; `None` for non-invoices (proformas) and
    /// unknown `eszamla` codes.
    pub e_invoice: Option<bool>,
    /// The issuing account's supplier id (`szallito/id`).
    pub supplier_id: Option<u64>,
    /// szamlazz.hu's document id (`alap/id`).
    pub document_id: Option<u64>,
    /// Registered credit entry amounts, in the order the server lists them.
    pub payments: Vec<Decimal>,
    /// The document was found under the order number referencing our proforma
    /// rather than under our external id.
    pub adopted: bool,
}

impl FoundDocument {
    /// Whether the document is live: `reversed != Some(true)`.
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.reversed != Some(true)
    }
}

impl From<&InvoiceDocument> for FoundDocument {
    fn from(document: &InvoiceDocument) -> Self {
        let info = &document.info;
        Self {
            number: info.invoice_number.as_str().to_owned(),
            document_type: info.document_type.clone(),
            reversed: info.reversed,
            order_number: info.order_number.clone(),
            referenced_proforma: info
                .referenced_proforma_number
                .as_ref()
                .map(|number| number.as_str().to_owned()),
            referenced_invoice: info
                .referenced_invoice_number
                .as_ref()
                .map(|number| number.as_str().to_owned()),
            gross: Some(document.totals.total.gross),
            net: Some(document.totals.total.net),
            test: info.test,
            e_invoice: match info.e_invoice {
                InvoiceAppearance::Paper => Some(false),
                InvoiceAppearance::Electronic(_) => Some(true),
                _ => None,
            },
            supplier_id: document.supplier.id,
            document_id: Some(info.id),
            payments: document
                .payments
                .iter()
                .map(|payment| payment.amount)
                .collect(),
            adopted: false,
        }
    }
}

/// The result of a query that only needs to know whether a document exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryOutcome {
    /// The document.
    Found(FoundDocument),
    /// szamlazz.hu does not know the selector (code 7): unknown number, order
    /// number or external id — or a deleted / consumed proforma.
    NotFound,
    /// The check itself failed (transport, parse, unavailability or another
    /// szamlazz.hu error); nothing may be concluded.
    Transport(String),
}

/// Why [`SzamlaAgent::query_document`] returned no document.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum QueryError {
    /// szamlazz.hu does not know the selector (code 7).
    #[error("szamlazz.hu does not know the document (code 7)")]
    NotFound,
    /// szamlazz.hu reported another error.
    #[error("szamlazz.hu error {code}: {message}")]
    Api {
        /// The szamlazz.hu code.
        code: String,
        /// The szamlazz.hu message.
        message: String,
    },
    /// szamlazz.hu reported unavailability (`szlahu_down`).
    #[error("szamlazz.hu is unavailable: {0}")]
    Unavailable(String),
    /// The HTTP exchange or the response parse failed.
    #[error("transport failure: {0}")]
    Transport(String),
}

/// One storno attempt (design §7 step 3).
#[derive(Debug, Clone, Copy)]
pub struct StornoAttempt<'a> {
    /// The invoice to reverse.
    pub invoice_number: &'a str,
    /// The external id attached to the storno invoice and pre-queried first
    /// (`{original external id}:storno`).
    pub external_id: &'a ExternalId,
    /// Comment placed on the storno invoice.
    pub comment: Option<&'a str>,
    /// Issue the storno as an e-invoice.
    pub e_invoice: bool,
}

/// The result of one storno attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StornoOutcome {
    /// The invoice is reversed by `storno_number` (now, or echoed by an
    /// idempotent repeat).
    Reversed {
        /// The storno invoice number.
        storno_number: String,
        /// The storno invoice's (negative) gross total.
        gross: Option<Decimal>,
        /// szamlazz.hu's document id of the storno invoice.
        document_id: Option<u64>,
    },
    /// The pre-query found the storno invoice under the storno external id;
    /// nothing was sent.
    AlreadyReversed {
        /// The storno invoice number.
        storno_number: String,
    },
    /// szamlazz.hu answered success but echoed the requested document with
    /// positive totals: a proforma or delivery note, which cannot be reversed.
    NotStornoable,
    /// szamlazz.hu refused (14: the document is itself a storno; 221: it has a
    /// corrective; …).
    Rejected {
        /// The szamlazz.hu code.
        code: String,
        /// The szamlazz.hu message.
        message: String,
    },
    /// The storno may or may not have been issued; re-query before retrying.
    Unknown {
        /// The szamlazz.hu code, when one was reported.
        code: Option<String>,
        /// What was reported.
        message: String,
    },
    /// The HTTP exchange or the response parse failed.
    Transport(String),
}

/// The result of a proforma deletion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeleteOutcome {
    /// Deleted now.
    Deleted,
    /// szamlazz.hu no longer knows the proforma (335): already deleted or
    /// consumed.
    AlreadyGone,
    /// szamlazz.hu refused.
    Rejected {
        /// The szamlazz.hu code.
        code: String,
        /// The szamlazz.hu message.
        message: String,
    },
    /// The HTTP exchange, the response parse or the service failed.
    Transport(String),
}

/// The result of registering credit entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SetPaymentsOutcome {
    /// The entries are registered.
    Done {
        /// Outstanding amount after the update.
        outstanding: Option<Decimal>,
        /// Gross total of the invoice.
        gross: Option<Decimal>,
    },
    /// szamlazz.hu (or the wire contract: more than five entries) refused.
    Rejected {
        /// The szamlazz.hu code, or `request` for a wire-contract violation.
        code: String,
        /// The message.
        message: String,
    },
    /// The HTTP exchange, the response parse or the service failed.
    Transport(String),
}

impl SzamlaAgent {
    /// Builds the layer for `config`: agent-key credentials and the configured
    /// endpoint override.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP client cannot be constructed.
    pub fn new(config: Arc<Config>) -> Result<Self, BuildError> {
        let mut builder = Client::builder()
            .credentials(Credentials::agent_key(config.account.agent_key.expose()));
        if let Some(endpoint) = &config.account.endpoint {
            builder = builder.endpoint(endpoint.clone());
        }
        Ok(Self {
            client: builder.build()?,
            config,
        })
    }

    /// The deployment configuration.
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// One issuing attempt, query-first (design §6 step 4).
    ///
    /// 1. Query by external id: a validated hit is [`IssueOutcome::Found`], an
    ///    invalid one [`IssueOutcome::Collision`]; code 7 continues; any other
    ///    failure is [`IssueOutcome::Transport`] — never create when the check
    ///    itself failed.
    /// 2. With `check_hint`, query by order number: a live `SZ | ES | VS` not
    ///    among `our_numbers` is [`IssueOutcome::Foreign`], unless it is of
    ///    the issued kind and references `our_proforma` — then
    ///    [`IssueOutcome::Found`] with `adopted`. Anything else continues.
    /// 3. Create: success with a number is [`IssueOutcome::Issued`]; 71/152
    ///    re-queries the external id ([`IssueOutcome::Found`] /
    ///    [`IssueOutcome::Collision`] / [`IssueOutcome::DuplicateOrderNumber`]);
    ///    1, 55, 56 and `szlahu_down` are [`IssueOutcome::Unknown`]; other
    ///    codes [`IssueOutcome::Rejected`].
    pub async fn issue(&self, request: IssueRequest<'_>) -> IssueOutcome {
        let span = tracing::info_span!(
            "szamla_agent.issue",
            external_id = %request.external_id,
            kind = %request.kind,
        );
        self.issue_inner(&request).instrument(span).await
    }

    async fn issue_inner(&self, request: &IssueRequest<'_>) -> IssueOutcome {
        match self.query_by_external_id(request).await {
            Ok(Some(outcome)) => return outcome,
            Ok(None) => {}
            Err(message) => return IssueOutcome::Transport(message),
        }

        if request.check_hint {
            match self
                .query_raw(InvoiceSelector::OrderNumber(
                    request.order.as_str().to_owned(),
                ))
                .await
            {
                Ok(document) => {
                    if let Some(outcome) = classify_hint(&document, request) {
                        return outcome;
                    }
                }
                Err(QueryError::NotFound | QueryError::Api { .. }) => {}
                Err(error) => return IssueOutcome::Transport(error.to_string()),
            }
        }

        match self.client.send(request.create).await {
            Ok(result) => match result.invoice_number {
                Some(number) => {
                    tracing::info!(number = %number, "document issued");
                    IssueOutcome::Issued(IssuedDocument {
                        number: number.as_str().to_owned(),
                        net: result.net_total,
                        gross: result.gross_total,
                        outstanding: result.outstanding,
                        customer_account_url: result.customer_account_url,
                        document_id: result.document_id,
                        notification_delivery_failed: result.notification_delivery_failed,
                    })
                }
                None => IssueOutcome::Unknown {
                    code: None,
                    message: "create succeeded without a document number".to_owned(),
                },
            },
            Err(error) => match classify_failure(error) {
                Failure::Duplicate { code, message } => {
                    tracing::info!(code = %code, "duplicate order number; re-querying");
                    match self.query_by_external_id(request).await {
                        Ok(Some(outcome)) => outcome,
                        Ok(None) => IssueOutcome::DuplicateOrderNumber { code, message },
                        Err(message) => IssueOutcome::Transport(message),
                    }
                }
                Failure::Rejected { code, message } => {
                    tracing::info!(code = %code, "document rejected");
                    IssueOutcome::Rejected { code, message }
                }
                Failure::Unknown { code, message } => {
                    tracing::warn!(code = ?code, "outcome unknown");
                    IssueOutcome::Unknown { code, message }
                }
                Failure::Transport(message) => {
                    tracing::warn!("transport failure");
                    IssueOutcome::Transport(message)
                }
            },
        }
    }

    /// Queries the document `number` to verify a recorded document.
    pub async fn verify(&self, number: &str) -> QueryOutcome {
        outcome(
            self.query_raw(InvoiceSelector::InvoiceNumber(InvoiceNumber::new(number)))
                .await,
        )
    }

    /// Queries by any selector.
    pub async fn query(&self, selector: &Selector) -> QueryOutcome {
        outcome(self.query_raw(invoice_selector(selector)).await)
    }

    /// The order-number hint: the most recently issued document of any kind
    /// carrying the order number.
    pub async fn hint(&self, order: &OrderKey) -> QueryOutcome {
        outcome(
            self.query_raw(InvoiceSelector::OrderNumber(order.as_str().to_owned()))
                .await,
        )
    }

    /// Queries by any selector and returns the full document (for the public
    /// `SzamlaAgent.query` handler).
    ///
    /// # Errors
    ///
    /// See [`QueryError`].
    pub async fn query_document(&self, selector: &Selector) -> Result<InvoiceDocument, QueryError> {
        self.query_raw(invoice_selector(selector)).await
    }

    /// One storno attempt, query-first (design §7 step 3): the storno external
    /// id is queried and an `SS` referencing the invoice is
    /// [`StornoOutcome::AlreadyReversed`]; otherwise `xmlszamlast` is sent
    /// with the external id, comment and e-invoice flag and **no issue date**
    /// (352 otherwise), and the response is validated with
    /// [`szamlazz_agent::ops::invoice::CreatedInvoice::reverses`].
    pub async fn storno(&self, attempt: StornoAttempt<'_>) -> StornoOutcome {
        let span = tracing::info_span!(
            "szamla_agent.storno",
            number = %attempt.invoice_number,
            external_id = %attempt.external_id,
        );
        self.storno_inner(attempt).instrument(span).await
    }

    async fn storno_inner(&self, attempt: StornoAttempt<'_>) -> StornoOutcome {
        match self
            .query_raw(InvoiceSelector::ExternalId(
                attempt.external_id.as_str().to_owned(),
            ))
            .await
        {
            Ok(document)
                if document.info.document_type == "SS"
                    && document
                        .info
                        .referenced_invoice_number
                        .as_ref()
                        .map(InvoiceNumber::as_str)
                        == Some(attempt.invoice_number) =>
            {
                let storno_number = document.info.invoice_number.as_str().to_owned();
                tracing::info!(storno_number = %storno_number, "storno already issued");
                return StornoOutcome::AlreadyReversed { storno_number };
            }
            Ok(_) | Err(QueryError::NotFound) => {}
            Err(error) => return StornoOutcome::Transport(error.to_string()),
        }

        let mut request = StornoInvoice::new(attempt.invoice_number);
        request.e_invoice = attempt.e_invoice;
        request.external_id = Some(attempt.external_id.as_str().to_owned());
        request.comment = attempt.comment.map(str::to_owned);
        request
            .aggregator
            .clone_from(&self.config.defaults.aggregator);
        request.guardian = self.config.defaults.guardian;
        request.issue_date = None;

        match self.client.send(&request).await {
            Ok(created) if created.reverses(&request.invoice_number) => {
                tracing::info!(storno_number = %created.invoice_number, "invoice reversed");
                StornoOutcome::Reversed {
                    storno_number: created.invoice_number.as_str().to_owned(),
                    gross: created.gross_total,
                    document_id: created.document_id,
                }
            }
            Ok(created) => {
                tracing::info!(echoed = %created.invoice_number, "storno was a no-op");
                StornoOutcome::NotStornoable
            }
            Err(error) => match classify_failure(error) {
                Failure::Rejected { code, message } | Failure::Duplicate { code, message } => {
                    tracing::info!(code = %code, "storno rejected");
                    StornoOutcome::Rejected { code, message }
                }
                Failure::Unknown { code, message } => StornoOutcome::Unknown { code, message },
                Failure::Transport(message) => StornoOutcome::Transport(message),
            },
        }
    }

    /// Deletes the proforma `number`; 335 is [`DeleteOutcome::AlreadyGone`].
    pub async fn delete_proforma(&self, number: &str) -> DeleteOutcome {
        let request =
            DeleteProforma::new(ProformaSelector::InvoiceNumber(InvoiceNumber::new(number)));
        match self.client.send(&request).await {
            Ok(()) => {
                tracing::info!(number = %number, "proforma deleted");
                DeleteOutcome::Deleted
            }
            Err(ClientError::Api(api)) if api.code == ErrorCode::ProformaNotFound => {
                tracing::info!(number = %number, "proforma already gone");
                DeleteOutcome::AlreadyGone
            }
            Err(ClientError::Api(api)) => DeleteOutcome::Rejected {
                code: api.code.code().to_owned(),
                message: api.message,
            },
            Err(error) => DeleteOutcome::Transport(error.to_string()),
        }
    }

    /// Registers `entries` on invoice `number`, replacing the existing entries
    /// unless `additive`.
    pub async fn set_payments(
        &self,
        number: &str,
        entries: &[PaymentEntry],
        additive: bool,
    ) -> SetPaymentsOutcome {
        let credit_entries = entries
            .iter()
            .map(|entry| {
                let mut credit =
                    CreditEntry::new(entry.date, entry.method.clone().into(), entry.amount);
                credit.description.clone_from(&entry.description);
                credit
            })
            .collect::<Vec<_>>();
        let credit_entries = match CreditEntries::try_from(credit_entries) {
            Ok(entries) => entries,
            Err(error) => {
                return SetPaymentsOutcome::Rejected {
                    code: "request".to_owned(),
                    message: error.to_string(),
                };
            }
        };
        let mut request = RegisterCreditEntry::new(number);
        request.additive = additive;
        request.entries = credit_entries;
        request
            .aggregator
            .clone_from(&self.config.defaults.aggregator);

        match self.client.send(&request).await {
            Ok(result) => {
                tracing::info!(number = %number, additive, "credit entries registered");
                SetPaymentsOutcome::Done {
                    outstanding: result.outstanding,
                    gross: result.gross_total,
                }
            }
            Err(ClientError::Api(api)) => SetPaymentsOutcome::Rejected {
                code: api.code.code().to_owned(),
                message: api.message,
            },
            Err(ClientError::Request(error)) => SetPaymentsOutcome::Rejected {
                code: "request".to_owned(),
                message: error.to_string(),
            },
            Err(error) => SetPaymentsOutcome::Transport(error.to_string()),
        }
    }

    /// Step 1 (and the 71/152 re-query) of [`Self::issue`]: `Ok(Some)` on a
    /// hit (validated or not), `Ok(None)` on code 7, `Err` when the check
    /// itself failed.
    async fn query_by_external_id(
        &self,
        request: &IssueRequest<'_>,
    ) -> Result<Option<IssueOutcome>, String> {
        match self
            .query_raw(InvoiceSelector::ExternalId(
                request.external_id.as_str().to_owned(),
            ))
            .await
        {
            Ok(document) => {
                let found = FoundDocument::from(&document);
                if is_ours(&document, request) {
                    tracing::info!(number = %found.number, "found under external id");
                    Ok(Some(IssueOutcome::Found(found)))
                } else {
                    tracing::warn!(number = %found.number, "external id collision");
                    Ok(Some(IssueOutcome::Collision(found)))
                }
            }
            Err(QueryError::NotFound) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    async fn query_raw(&self, selector: InvoiceSelector) -> Result<InvoiceDocument, QueryError> {
        match self.client.send(&QueryInvoiceXml::new(selector)).await {
            Ok(document) => Ok(document),
            Err(ClientError::Api(api)) if api.code == ErrorCode::MissingData => {
                Err(QueryError::NotFound)
            }
            Err(ClientError::Api(api)) => Err(QueryError::Api {
                code: api.code.code().to_owned(),
                message: api.message,
            }),
            Err(ClientError::ServiceUnavailable(message)) => Err(QueryError::Unavailable(message)),
            Err(error) => Err(QueryError::Transport(error.to_string())),
        }
    }
}

/// The `tipus` code the documents of `kind` carry.
#[must_use]
pub const fn document_type_of(kind: IssuedKind) -> &'static str {
    match kind {
        IssuedKind::Proforma => "D",
        IssuedKind::Invoice => "SZ",
        IssuedKind::Prepayment => "ES",
        IssuedKind::Final => "VS",
        IssuedKind::Corrective => "HS",
    }
}

/// Whether `tipus` is the document type a `kind` slot holds (`D`, `SZ`,
/// `ES`, `VS`).
#[must_use]
pub fn is_live_kind(kind: DocumentKind, tipus: &str) -> bool {
    tipus == document_type_of(kind.into())
}

/// Whether `tipus` is a legal invoice of the kinds the ledger slots hold:
/// `SZ`, `ES` or `VS`. Stornos, correctives, proformas and delivery notes are
/// not.
#[must_use]
pub fn is_invoice_family(tipus: &str) -> bool {
    matches!(tipus, "SZ" | "ES" | "VS")
}

/// A failed create-like call, classified for the outcome enums.
enum Failure {
    Rejected {
        code: String,
        message: String,
    },
    Duplicate {
        code: String,
        message: String,
    },
    Unknown {
        code: Option<String>,
        message: String,
    },
    Transport(String),
}

fn classify_failure(error: ClientError) -> Failure {
    match error {
        ClientError::Api(api) => {
            let code = api.code.code().to_owned();
            match api.code {
                ErrorCode::DuplicateOrderNumber | ErrorCode::DuplicateOrderNumberNamed => {
                    Failure::Duplicate {
                        code,
                        message: api.message,
                    }
                }
                ErrorCode::Maintenance
                | ErrorCode::EInvoiceSigningFailed
                | ErrorCode::InvoiceNotificationDeliveryFailed => Failure::Unknown {
                    code: Some(code),
                    message: api.message,
                },
                _ => Failure::Rejected {
                    code,
                    message: api.message,
                },
            }
        }
        ClientError::ServiceUnavailable(message) => Failure::Unknown {
            code: None,
            message,
        },
        ClientError::Request(error) => Failure::Rejected {
            code: "request".to_owned(),
            message: error.to_string(),
        },
        other => Failure::Transport(other.to_string()),
    }
}

/// Whether a document found under our external id is ours: same order, the
/// issued kind's `tipus`, the account's `teszt` and (when both are known) the
/// account's supplier id.
fn is_ours(document: &InvoiceDocument, request: &IssueRequest<'_>) -> bool {
    document.info.order_number.as_deref().map(str::trim) == Some(request.order.as_str())
        && document.info.document_type == document_type_of(request.kind)
        && account_matches(document, request)
}

fn account_matches(document: &InvoiceDocument, request: &IssueRequest<'_>) -> bool {
    document.info.test == request.expect_test
        && match (request.expect_supplier_id, document.supplier.id) {
            (Some(expected), Some(seen)) => expected == seen,
            _ => true,
        }
}

/// Step 2 of [`SzamlaAgent::issue`]: what the order-number hint means.
fn classify_hint(document: &InvoiceDocument, request: &IssueRequest<'_>) -> Option<IssueOutcome> {
    let info = &document.info;
    let tipus = info.document_type.as_str();
    let number = info.invoice_number.as_str();
    if !is_invoice_family(tipus)
        || info.reversed == Some(true)
        || request.our_numbers.iter().any(|known| known == number)
    {
        return None;
    }
    let converts_our_proforma = request.our_proforma.is_some()
        && info
            .referenced_proforma_number
            .as_ref()
            .map(InvoiceNumber::as_str)
            == request.our_proforma;
    let mut found = FoundDocument::from(document);
    if matches!(tipus, "SZ" | "ES")
        && tipus == document_type_of(request.kind)
        && converts_our_proforma
        && account_matches(document, request)
    {
        tracing::info!(number = %number, "adopting the conversion of our proforma");
        found.adopted = true;
        Some(IssueOutcome::Found(found))
    } else {
        tracing::warn!(number = %number, tipus = %tipus, "foreign document under the order");
        Some(IssueOutcome::Foreign(found))
    }
}

fn outcome(result: Result<InvoiceDocument, QueryError>) -> QueryOutcome {
    match result {
        Ok(document) => QueryOutcome::Found(FoundDocument::from(&document)),
        Err(QueryError::NotFound) => QueryOutcome::NotFound,
        Err(error) => QueryOutcome::Transport(error.to_string()),
    }
}

fn invoice_selector(selector: &Selector) -> InvoiceSelector {
    match selector {
        Selector::InvoiceNumber(number) => {
            InvoiceSelector::InvoiceNumber(InvoiceNumber::new(number.clone()))
        }
        Selector::OrderNumber(number) => InvoiceSelector::OrderNumber(number.clone()),
        Selector::ExternalId(id) => InvoiceSelector::ExternalId(id.clone()),
    }
}
