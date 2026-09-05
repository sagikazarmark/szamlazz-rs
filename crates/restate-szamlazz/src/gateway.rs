//! The module that speaks to szamlazz.hu on behalf of one account: one plain
//! async fn per `ctx.run`, over the [`szamlazz_agent::Client`], returning
//! every expected szamlazz.hu outcome **as data** — a rejection, a duplicate
//! order number, a not-found or a no-op storno is a value, never an `Err`.
//!
//! [`Gateway`] owns the client and the [`Account`] it speaks for; it is not a
//! second client — the Számla Agent `Client` is the transport it wraps.
//! `Szamlazz.Order` calls these inside `ctx.run`; the `Szamlazz.Agent` Restate
//! service is a thin facade over the same functions. Neither Restate service
//! calls the other. Everything the services need to know about the account
//! (its ownership-validation pins, its document defaults) is read through
//! [`Gateway::account`].
//!
//! Every query result is validated before it is called ours (design §3):
//! external ids are not unique server-side and the order-number hint returns
//! the most recently issued document of any kind.
//!
//! Tracing events carry external ids, kinds, numbers and codes — never buyer
//! data.
//!
//! The outcome types derive `serde` so that the Restate services can journal
//! them as the result of a `ctx.run`. They carry the agent crate's response
//! types as they are — [`InvoiceDocument`], [`InvoiceCreationResult`],
//! [`CreatedInvoice`] — which round-trip through JSON; a journaled document
//! therefore includes the buyer block szamlazz.hu returned with it.
//! [`InvoiceDocumentExt`] adds the checks the services make on a queried
//! document before trusting or acting on it.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use szamlazz_agent::client::BuildError;
use szamlazz_agent::ops::credit_entry::{
    CreditEntries, CreditEntry, CreditEntryResult, RegisterCreditEntry,
};
use szamlazz_agent::ops::invoice::{CreateInvoice, CreatedInvoice, InvoiceCreationResult};
use szamlazz_agent::ops::proforma::{DeleteProforma, ProformaSelector};
use szamlazz_agent::ops::query_pdf::InvoiceSelector;
use szamlazz_agent::ops::query_xml::{InvoiceAppearance, InvoiceDocument, QueryInvoiceXml};
use szamlazz_agent::ops::storno::StornoInvoice;
use szamlazz_agent::{ApiError, Client, ClientError, Credentials, ErrorCode, InvoiceNumber};
use tracing::Instrument as _;

use crate::config::{AccountMode, Config, Defaults, SellerConfig};
use crate::contract::{DocumentKind, IssuedKind, PaymentEntry, Selector};
use crate::identity::{ExternalId, OrderKey};

pub mod build;

pub use build::{DocumentRefs, InputError, gross_total};

/// The module that speaks to szamlazz.hu for one account: the Számla Agent
/// client plus the [`Account`] it is opened for.
#[derive(Debug, Clone)]
pub struct Gateway {
    client: Client,
    account: Account,
}

/// One szamlazz.hu account as the services know it — never the agent key.
///
/// Everything account-shaped the service layer reads: the ownership-validation
/// pins (`mode`, `supplier_id`), the document defaults and the seller block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// Whether the account is live or a test account; validated against
    /// `teszt` on every document found under our external ids.
    pub mode: AccountMode,
    /// The account's supplier id (`szállító/id`). Optional pin; when set it is
    /// validated against every document found under our external ids.
    pub supplier_id: Option<u64>,
    /// Document defaults that per-call overrides may change.
    pub defaults: Defaults,
    /// The seller block; account data is used where absent.
    pub seller: SellerConfig,
}

impl From<&Config> for Account {
    fn from(config: &Config) -> Self {
        Self {
            mode: config.account.mode,
            supplier_id: config.account.supplier_id,
            defaults: config.defaults.clone(),
            seller: config.seller.clone(),
        }
    }
}

/// One issuing attempt (design §5 step 3): pre-query by external id, optional
/// order-number hint, create, and the duplicate-order-number re-query.
///
/// A found document is validated against the gateway's own [`Account`]; the
/// request carries only what identifies the document.
#[derive(Debug, Clone)]
pub struct IssueRequest<'a> {
    /// The external id the document carries and is looked up by.
    pub external_id: &'a ExternalId,
    /// The kind being issued; a found document must have the matching `tipus`.
    pub kind: IssuedKind,
    /// The order the document belongs to.
    pub order: &'a OrderKey,
    /// The create request built by [`Gateway::build_create`].
    pub create: &'a CreateInvoice,
    /// Proceed past a reversed document under the external id instead of
    /// answering [`IssueOutcome::FoundReversed`].
    pub reissue: bool,
    /// Query the order number before creating to detect foreign documents.
    pub check_hint: bool,
    /// Numbers of documents known to be ours (seen in the exclusivity and
    /// proforma checks); hint results among them are ignored.
    pub our_numbers: &'a [String],
}

/// The result of one issuing attempt.
///
/// Documents are boxed: a queried [`InvoiceDocument`] is large next to the
/// code-and-message variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IssueOutcome {
    /// szamlazz.hu issued the document (or replayed a byte-identical earlier
    /// create — indistinguishable and reported either way). The result
    /// carries a number: a success without one is [`IssueOutcome::Unknown`].
    Issued(InvoiceCreationResult),
    /// The pre-query found a live document of ours under the external id.
    /// Nothing was created.
    Found(Box<InvoiceDocument>),
    /// The pre-query found a reversed document of ours under the external id
    /// and `reissue` was not set. Nothing was created.
    FoundReversed(Box<InvoiceDocument>),
    /// szamlazz.hu refused the order number as a duplicate (71/152) and the
    /// external-id re-query found a live document of ours: an earlier attempt
    /// had landed.
    Reconciled(Box<InvoiceDocument>),
    /// The external id resolves to a document that fails validation (another
    /// order, kind, account mode or supplier). Nothing was created.
    Collision(Box<InvoiceDocument>),
    /// A live invoice-kind document that is not ours exists under the order
    /// number. Nothing was created.
    Foreign(Box<InvoiceDocument>),
    /// szamlazz.hu refused the order number as a duplicate (71/152) and the
    /// external id re-query found no live document of ours.
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

/// A successful create: [`IssueOutcome::Issued`] with the number, or
/// [`IssueOutcome::Unknown`] when szamlazz.hu answered success without one.
impl From<InvoiceCreationResult> for IssueOutcome {
    fn from(result: InvoiceCreationResult) -> Self {
        if result.invoice_number.is_some() {
            Self::Issued(result)
        } else {
            Self::Unknown {
                code: None,
                message: "create succeeded without a document number".to_owned(),
            }
        }
    }
}

/// The checks the services make on a queried document before trusting or
/// acting on it (design §3).
pub trait InvoiceDocumentExt {
    /// The document number.
    fn number(&self) -> &str;

    /// Whether the document is live: `reversed != Some(true)`.
    fn is_live(&self) -> bool;

    /// Whether the document is the storno invoice (`SS`) reversing `number`.
    fn is_storno_of(&self, number: &str) -> bool;

    /// Whether it is an e-invoice; `None` for non-invoices (proformas) and
    /// unknown `eszamla` codes, where the account default applies.
    fn e_invoice(&self) -> Option<bool>;

    /// Registered credit entry amounts, in the order szamlazz.hu lists them.
    fn payment_amounts(&self) -> Vec<Decimal>;

    /// Whether the document belongs to the configured account: it carries the
    /// account's `teszt` flag and — when both are known — the account's
    /// supplier id.
    fn account_matches(&self, expect_test: bool, expect_supplier_id: Option<u64>) -> bool;

    /// Whether the document is ours (design §3): it carries `order`, the
    /// `tipus` of `kind` and [belongs to the account](Self::account_matches).
    fn is_ours(
        &self,
        order: &OrderKey,
        kind: IssuedKind,
        expect_test: bool,
        expect_supplier_id: Option<u64>,
    ) -> bool;
}

impl InvoiceDocumentExt for InvoiceDocument {
    fn number(&self) -> &str {
        self.info.invoice_number.as_str()
    }

    fn is_live(&self) -> bool {
        self.info.reversed != Some(true)
    }

    fn is_storno_of(&self, number: &str) -> bool {
        self.info.document_type == "SS"
            && self
                .info
                .referenced_invoice_number
                .as_ref()
                .is_some_and(|referenced| referenced.as_str() == number)
    }

    fn e_invoice(&self) -> Option<bool> {
        match self.info.e_invoice {
            InvoiceAppearance::Paper => Some(false),
            InvoiceAppearance::Electronic(_) => Some(true),
            _ => None,
        }
    }

    fn payment_amounts(&self) -> Vec<Decimal> {
        self.payments.iter().map(|payment| payment.amount).collect()
    }

    fn account_matches(&self, expect_test: bool, expect_supplier_id: Option<u64>) -> bool {
        self.info.test == expect_test
            && match (expect_supplier_id, self.supplier.id) {
                (Some(expected), Some(seen)) => expected == seen,
                _ => true,
            }
    }

    fn is_ours(
        &self,
        order: &OrderKey,
        kind: IssuedKind,
        expect_test: bool,
        expect_supplier_id: Option<u64>,
    ) -> bool {
        self.info.order_number.as_deref().map(str::trim) == Some(order.as_str())
            && self.info.document_type == document_type_of(kind)
            && self.account_matches(expect_test, expect_supplier_id)
    }
}

/// The result of a query that only needs to know whether a document exists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum QueryOutcome {
    /// The document.
    Found(Box<InvoiceDocument>),
    /// szamlazz.hu does not know the selector (code 7): unknown number, order
    /// number or external id — or a deleted / consumed proforma.
    NotFound,
    /// The check itself failed (transport, parse, unavailability or another
    /// szamlazz.hu error); nothing may be concluded.
    Transport(String),
}

/// Why [`Gateway::query_document`] returned no document.
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

/// One storno attempt (design §6 step 2).
#[derive(Debug, Clone, Copy)]
pub struct StornoAttempt<'a> {
    /// The invoice to reverse.
    pub invoice_number: &'a str,
    /// The external id attached to the storno invoice and pre-queried first
    /// (`{namespace}:{order}:storno:{number}` or `{namespace}:by-number:{number}:storno`).
    pub external_id: &'a ExternalId,
    /// Comment placed on the storno invoice.
    pub comment: Option<&'a str>,
    /// Issue the storno as an e-invoice.
    pub e_invoice: bool,
}

/// The result of one storno attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StornoOutcome {
    /// The invoice is reversed by the storno invoice szamlazz.hu issued (now,
    /// or echoed by an idempotent repeat), validated with
    /// [`CreatedInvoice::reverses`] by [`Gateway::storno`].
    Reversed(CreatedInvoice),
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

/// A szamlazz.hu error on a deletion: 335 is [`DeleteOutcome::AlreadyGone`],
/// anything else [`DeleteOutcome::Rejected`].
impl From<ApiError> for DeleteOutcome {
    fn from(api: ApiError) -> Self {
        if api.code == ErrorCode::ProformaNotFound {
            Self::AlreadyGone
        } else {
            Self::Rejected {
                code: api.code.code().to_owned(),
                message: api.message,
            }
        }
    }
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

/// A successful registration: [`SetPaymentsOutcome::Done`] with the reported
/// totals.
impl From<CreditEntryResult> for SetPaymentsOutcome {
    fn from(result: CreditEntryResult) -> Self {
        Self::Done {
            outstanding: result.outstanding,
            gross: result.gross_total,
        }
    }
}

/// A szamlazz.hu error on a registration is a rejection.
impl From<ApiError> for SetPaymentsOutcome {
    fn from(api: ApiError) -> Self {
        Self::Rejected {
            code: api.code.code().to_owned(),
            message: api.message,
        }
    }
}

impl Gateway {
    /// Opens the gateway for the account in `config`: agent-key credentials
    /// and the configured endpoint override.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP client cannot be constructed.
    pub fn new(config: &Config) -> Result<Self, BuildError> {
        let mut builder = Client::builder()
            .credentials(Credentials::agent_key(config.account.agent_key.expose()));
        if let Some(endpoint) = &config.account.endpoint {
            builder = builder.endpoint(endpoint.clone());
        }
        Ok(Self {
            client: builder.build()?,
            account: Account::from(config),
        })
    }

    /// The account the gateway speaks for: the only way the services read
    /// account configuration.
    #[must_use]
    pub fn account(&self) -> &Account {
        &self.account
    }

    /// Whether `found` is ours (design §3) for `order` and `kind`, validated
    /// against this gateway's account.
    fn is_ours(&self, found: &InvoiceDocument, order: &OrderKey, kind: IssuedKind) -> bool {
        found.is_ours(
            order,
            kind,
            self.account.mode.is_test(),
            self.account.supplier_id,
        )
    }

    /// One issuing attempt, query-first (design §5 step 3).
    ///
    /// 1. Query by external id: a validated live hit is
    ///    [`IssueOutcome::Found`]; a validated reversed hit is
    ///    [`IssueOutcome::FoundReversed`] unless `reissue`, in which case the
    ///    attempt continues; an invalid hit is [`IssueOutcome::Collision`];
    ///    code 7 continues; any other failure is [`IssueOutcome::Transport`]
    ///    — never create when the check itself failed.
    /// 2. With `check_hint`, query by order number: a live `SZ | ES | VS`
    ///    that is neither among `our_numbers` nor the document seen in
    ///    step 1 is [`IssueOutcome::Foreign`]. Anything else continues.
    /// 3. Create: success with a number is [`IssueOutcome::Issued`]; 71/152
    ///    re-queries the external id — a live document of ours is
    ///    [`IssueOutcome::Reconciled`], a reversed one or code 7 is
    ///    [`IssueOutcome::DuplicateOrderNumber`] (the duplicate is foreign),
    ///    an invalid one [`IssueOutcome::Collision`]; 1, 55, 56 and
    ///    `szlahu_down` are [`IssueOutcome::Unknown`]; other codes
    ///    [`IssueOutcome::Rejected`].
    pub async fn issue(&self, request: IssueRequest<'_>) -> IssueOutcome {
        let span = tracing::info_span!(
            "gateway.issue",
            external_id = %request.external_id,
            kind = %request.kind,
            reissue = request.reissue,
        );
        self.issue_inner(&request).instrument(span).await
    }

    async fn issue_inner(&self, request: &IssueRequest<'_>) -> IssueOutcome {
        // Step 1: the external-id pre-query.
        let seen = match self.query_by_external_id(request).await {
            Ok(Some(found)) if !self.is_ours(&found, request.order, request.kind) => {
                tracing::warn!(number = %found.number(), "external id collision");
                return IssueOutcome::Collision(found);
            }
            Ok(Some(found)) if found.is_live() => {
                tracing::info!(number = %found.number(), "found live under external id");
                return IssueOutcome::Found(found);
            }
            Ok(Some(found)) if request.reissue => {
                tracing::info!(number = %found.number(), "reversed under external id; reissuing");
                Some(found.number().to_owned())
            }
            Ok(Some(found)) => {
                tracing::info!(number = %found.number(), "found reversed under external id");
                return IssueOutcome::FoundReversed(found);
            }
            Ok(None) => None,
            Err(message) => return IssueOutcome::Transport(message),
        };

        // Step 2: the order-number hint.
        if request.check_hint {
            match self
                .query_raw(InvoiceSelector::OrderNumber(
                    request.order.as_str().to_owned(),
                ))
                .await
            {
                Ok(document) => {
                    if is_foreign(&document, request.our_numbers, seen.as_deref()) {
                        tracing::warn!(
                            number = %document.number(),
                            tipus = %document.info.document_type,
                            "foreign document under the order"
                        );
                        return IssueOutcome::Foreign(Box::new(document));
                    }
                }
                Err(QueryError::NotFound | QueryError::Api { .. }) => {}
                Err(error) => return IssueOutcome::Transport(error.to_string()),
            }
        }

        // Step 3: create.
        match self.client.send(request.create).await {
            Ok(result) => {
                if let Some(number) = &result.invoice_number {
                    tracing::info!(number = %number, "document issued");
                }
                IssueOutcome::from(result)
            }
            Err(error) => match classify_failure(error) {
                Failure::Duplicate { code, message } => {
                    tracing::info!(code = %code, "duplicate order number; re-querying");
                    match self.query_by_external_id(request).await {
                        Ok(Some(found)) if !self.is_ours(&found, request.order, request.kind) => {
                            IssueOutcome::Collision(found)
                        }
                        Ok(Some(found)) if found.is_live() => {
                            tracing::info!(number = %found.number(), "reconciled after duplicate");
                            IssueOutcome::Reconciled(found)
                        }
                        // Only a reversed document of ours holds the id: the
                        // live duplicate is not ours.
                        Ok(Some(_) | None) => IssueOutcome::DuplicateOrderNumber { code, message },
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
    /// `Szamlazz.Agent.query` handler).
    ///
    /// # Errors
    ///
    /// See [`QueryError`].
    pub async fn query_document(&self, selector: &Selector) -> Result<InvoiceDocument, QueryError> {
        self.query_raw(invoice_selector(selector)).await
    }

    /// One storno attempt, query-first (design §6 step 2): the storno external
    /// id is queried and an `SS` referencing the invoice is
    /// [`StornoOutcome::AlreadyReversed`]; otherwise `xmlszamlast` is sent
    /// with the external id, comment and e-invoice flag and **no issue date**
    /// (352 otherwise), and the response is validated with
    /// [`szamlazz_agent::ops::invoice::CreatedInvoice::reverses`].
    pub async fn storno(&self, attempt: StornoAttempt<'_>) -> StornoOutcome {
        let span = tracing::info_span!(
            "gateway.storno",
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
            Ok(document) if document.is_storno_of(attempt.invoice_number) => {
                let storno_number = document.number().to_owned();
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
            .clone_from(&self.account.defaults.aggregator);
        request.guardian = self.account.defaults.guardian;
        request.issue_date = None;

        match self.client.send(&request).await {
            Ok(created) if created.reverses(&request.invoice_number) => {
                tracing::info!(storno_number = %created.invoice_number, "invoice reversed");
                StornoOutcome::Reversed(created)
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
            Err(ClientError::Api(api)) => {
                let outcome = DeleteOutcome::from(api);
                if outcome == DeleteOutcome::AlreadyGone {
                    tracing::info!(number = %number, "proforma already gone");
                }
                outcome
            }
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
        let credit_entries = entries.iter().map(CreditEntry::from).collect::<Vec<_>>();
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
            .clone_from(&self.account.defaults.aggregator);

        match self.client.send(&request).await {
            Ok(result) => {
                tracing::info!(number = %number, additive, "credit entries registered");
                SetPaymentsOutcome::from(result)
            }
            Err(ClientError::Api(api)) => SetPaymentsOutcome::from(api),
            Err(ClientError::Request(error)) => SetPaymentsOutcome::Rejected {
                code: "request".to_owned(),
                message: error.to_string(),
            },
            Err(error) => SetPaymentsOutcome::Transport(error.to_string()),
        }
    }

    /// Step 1 (and the 71/152 re-query) of [`Self::issue`]: `Ok(Some)` on a
    /// hit (validated by the caller), `Ok(None)` on code 7, `Err` when the
    /// check itself failed.
    async fn query_by_external_id(
        &self,
        request: &IssueRequest<'_>,
    ) -> Result<Option<Box<InvoiceDocument>>, String> {
        match self
            .query_raw(InvoiceSelector::ExternalId(
                request.external_id.as_str().to_owned(),
            ))
            .await
        {
            Ok(document) => Ok(Some(Box::new(document))),
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

/// The kind whose documents carry `tipus`, or `None` for stornos, delivery
/// notes and unknown codes.
#[must_use]
pub fn issued_kind_of(tipus: &str) -> Option<IssuedKind> {
    match tipus {
        "D" => Some(IssuedKind::Proforma),
        "SZ" => Some(IssuedKind::Invoice),
        "ES" => Some(IssuedKind::Prepayment),
        "VS" => Some(IssuedKind::Final),
        "HS" => Some(IssuedKind::Corrective),
        _ => None,
    }
}

/// Whether `tipus` is the document type of `kind` (`D`, `SZ`, `ES`, `VS`).
#[must_use]
pub fn is_live_kind(kind: DocumentKind, tipus: &str) -> bool {
    tipus == document_type_of(kind.into())
}

/// Whether `tipus` is a legal invoice of the kinds an order carries: `SZ`,
/// `ES` or `VS`. Stornos, correctives, proformas and delivery notes are
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

/// Step 2 of [`Gateway::issue`]: whether the order-number hint is a live
/// invoice-kind document that is neither known to be ours nor the document
/// the pre-query saw under our external id.
fn is_foreign(found: &InvoiceDocument, our_numbers: &[String], seen: Option<&str>) -> bool {
    is_invoice_family(&found.info.document_type)
        && found.is_live()
        && Some(found.number()) != seen
        && !our_numbers.iter().any(|known| known == found.number())
}

fn outcome(result: Result<InvoiceDocument, QueryError>) -> QueryOutcome {
    match result {
        Ok(document) => QueryOutcome::Found(Box::new(document)),
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

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use rust_decimal::dec;
    use szamlazz_agent::ops::invoice::{Buyer, InvoiceHeader, InvoiceKind};
    use szamlazz_agent::wire::{AgentRequest as _, RawResponse};
    use szamlazz_agent::{Currency, Language, PaymentMethod};

    use super::*;

    /// A successful `xmlszamlavalasz` body, as create, storno and credit-entry
    /// responses share it.
    fn created(number: &str, net: &str, gross: &str, outstanding: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><szamlaszam>{number}</szamlaszam><szamlanetto>{net}</szamlanetto><szamlabrutto>{gross}</szamlabrutto><kintlevoseg>{outstanding}</kintlevoseg><vevoifiokurl>https://example.test/acct</vevoifiokurl></xmlszamlavalasz>"#
        )
    }

    fn response(body: &str) -> RawResponse {
        RawResponse::new([("szlahu_id", "924307747")], body.as_bytes().to_vec())
    }

    /// The smallest create request whose response parses.
    fn create_request() -> CreateInvoice {
        CreateInvoice::new(
            InvoiceKind::Proforma,
            InvoiceHeader::new(
                date(2026, 7, 4),
                date(2026, 7, 12),
                PaymentMethod::Transfer,
                Currency::HUF,
                Language::Hungarian,
            ),
            Buyer::new("A", "1", "B", "C"),
            Vec::new(),
        )
    }

    #[test]
    fn creation_result_with_a_number_is_issued() {
        let result = create_request()
            .parse(&response(&created("SZ-1", "1000", "1270", "270")))
            .expect("parse");
        let outcome = IssueOutcome::from(result.clone());
        assert_eq!(outcome, IssueOutcome::Issued(result));

        let IssueOutcome::Issued(issued) = outcome else {
            unreachable!()
        };
        assert_eq!(
            issued.invoice_number.as_ref().map(InvoiceNumber::as_str),
            Some("SZ-1")
        );
        assert_eq!(issued.net_total, Some(dec!(1000)));
        assert_eq!(issued.gross_total, Some(dec!(1270)));
        assert_eq!(issued.outstanding, Some(dec!(270)));
        assert_eq!(
            issued.customer_account_url.as_deref(),
            Some("https://example.test/acct")
        );
        assert_eq!(issued.document_id, Some(924_307_747));
        assert!(!issued.notification_delivery_failed);
    }

    #[test]
    fn creation_result_without_a_number_is_unknown() {
        // Only a PDF preview parses without a number.
        let mut request = create_request();
        request.header.preview_pdf = Some(true);
        let body = r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres></xmlszamlavalasz>"#;
        let result = request
            .parse(&RawResponse::new::<&str, &str>(
                [],
                body.as_bytes().to_vec(),
            ))
            .expect("parse");
        assert_eq!(result.invoice_number, None);
        assert_eq!(
            IssueOutcome::from(result),
            IssueOutcome::Unknown {
                code: None,
                message: "create succeeded without a document number".to_owned(),
            }
        );
    }

    /// A queried document, as the `szamla` response XML: a test-account
    /// document of `ORD-1` with two payments; proformas carry `eszamla` 0.
    fn queried(number: &str, tipus: &str, extra: &str) -> InvoiceDocument {
        let eszamla = if tipus == "D" { 0 } else { 2 };
        let body = format!(
            r#"<szamla xmlns="http://www.szamlazz.hu/szamla">
              <szallito><id>972720</id><nev>Seller</nev><cim><irsz>1111</irsz><telepules>Budapest</telepules><cim>Fő u. 1.</cim></cim></szallito>
              <alap><id>1</id><szamlaszam>{number}</szamlaszam><tipus>{tipus}</tipus><eszamla>{eszamla}</eszamla><rendelesszam>ORD-1</rendelesszam><teszt>true</teszt>{extra}</alap>
              <vevo><nev>Buyer</nev></vevo><tetelek></tetelek>
              <osszegek><totalossz><netto>1000</netto><afa>270</afa><brutto>1270</brutto></totalossz></osszegek>
              <kifizetesek><kifizetes><datum>2026-07-04</datum><jogcim>transfer</jogcim><osszeg>500</osszeg></kifizetes>
              <kifizetes><datum>2026-07-05</datum><jogcim>transfer</jogcim><osszeg>770</osszeg></kifizetes></kifizetesek>
              </szamla>"#
        );
        QueryInvoiceXml::new(InvoiceSelector::InvoiceNumber(InvoiceNumber::new(number)))
            .parse(&RawResponse::new::<&str, &str>([], body.into_bytes()))
            .expect("parse")
    }

    #[test]
    fn document_ext_reads_the_checks_off_a_queried_document() {
        let order = OrderKey::parse("ORD-1").expect("order");
        let live = queried("SZ-1", "SZ", "");
        assert_eq!(live.number(), "SZ-1");
        assert!(live.is_live());
        assert_eq!(live.e_invoice(), Some(true));
        assert_eq!(live.payment_amounts(), [dec!(500), dec!(770)]);
        assert!(live.account_matches(true, Some(972_720)));
        assert!(live.account_matches(true, None));
        assert!(!live.account_matches(false, Some(972_720)));
        assert!(!live.account_matches(true, Some(1)));
        assert!(live.is_ours(&order, IssuedKind::Invoice, true, Some(972_720)));
        assert!(!live.is_ours(&order, IssuedKind::Proforma, true, Some(972_720)));
        assert!(!live.is_ours(&order, IssuedKind::Invoice, false, None));
        assert!(!live.is_storno_of("SZ-0"));

        let reversed = queried("SZ-1", "SZ", "<sztornozott>true</sztornozott>");
        assert!(!reversed.is_live());
        assert!(reversed.is_ours(&order, IssuedKind::Invoice, true, None));

        let storno = queried("SS-1", "SS", "<hivszamlaszam>SZ-1</hivszamlaszam>");
        assert!(storno.is_live(), "the storno invoice carries no marker");
        assert!(storno.is_storno_of("SZ-1"));
        assert!(!storno.is_storno_of("SZ-2"));

        let proforma = queried("D-1", "D", "");
        assert_eq!(proforma.e_invoice(), None, "eszamla 0 is not an invoice");
        assert!(proforma.is_ours(&order, IssuedKind::Proforma, true, None));
    }

    #[test]
    fn set_payments_outcome_from_credit_entry_result() {
        let result = RegisterCreditEntry::new("SZ-1")
            .parse(&response(&created("SZ-1", "1000", "1270", "270")))
            .expect("parse");
        assert_eq!(
            SetPaymentsOutcome::from(result),
            SetPaymentsOutcome::Done {
                outstanding: Some(dec!(270)),
                gross: Some(dec!(1270)),
            }
        );
    }

    #[test]
    fn api_errors_map_to_delete_and_set_payments_outcomes() {
        let gone = ApiError {
            code: ErrorCode::ProformaNotFound,
            message: "Nincs ilyen díjbekérő".to_owned(),
        };
        assert_eq!(DeleteOutcome::from(gone), DeleteOutcome::AlreadyGone);

        let login = ApiError {
            code: ErrorCode::InvalidCredentials,
            message: "login".to_owned(),
        };
        assert_eq!(
            DeleteOutcome::from(login.clone()),
            DeleteOutcome::Rejected {
                code: "3".to_owned(),
                message: "login".to_owned(),
            }
        );
        assert_eq!(
            SetPaymentsOutcome::from(login),
            SetPaymentsOutcome::Rejected {
                code: "3".to_owned(),
                message: "login".to_owned(),
            }
        );
    }
}
