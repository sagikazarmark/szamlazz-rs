//! The module that speaks to szamlazz.hu on behalf of one account: one plain
//! async fn per `ctx.run`, over the [`szamlazz_agent::Client`], returning
//! every expected szamlazz.hu outcome **as data** — a rejection, a duplicate
//! order number, a not-found or a no-op storno is a value, never an `Err`.
//! The one exception is deliberate: the create and storno steps return
//! [`Unconfirmed`] when szamlazz.hu's answer is *not* known, so that the
//! type says what the run retry policy may re-execute.
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

use crate::account::Account;
use crate::contract::{DocumentKind, IssuedKind, PaymentEntry, Selector};
use crate::identity::{ExternalId, OrderKey};

pub mod build;

pub use build::{DocumentRefs, InputError, gross_total};

/// The module that speaks to szamlazz.hu for one account: the Számla Agent
/// client plus the [`Account`] it is opened for.
///
/// Opened with [`Gateway::open`] for one handler execution from a resolved
/// account and freshly fetched credentials.
#[derive(Debug, Clone)]
pub struct Gateway {
    client: Client,
    account: Account,
}

/// The lookup step (design §5 step 3): what identifies the document whose
/// external id is queried and, for every kind but correctives, the order
/// whose hint is taken.
///
/// A found document is validated against the gateway's own [`Account`]; the
/// request carries only what identifies the document.
#[derive(Debug, Clone)]
pub struct LookupRequest<'a> {
    /// The external id the document carries and is looked up by.
    pub external_id: &'a ExternalId,
    /// The kind being issued; a found document must have the matching `tipus`.
    /// Correctives take no order-number hint.
    pub kind: IssuedKind,
    /// The order the document belongs to.
    pub order: &'a OrderKey,
    /// Numbers of documents known to be ours (seen in the exclusivity and
    /// proforma checks); hint results among them are ignored.
    pub our_numbers: &'a [String],
}

/// What the lookup step found. Every case that needs no create is settled
/// here; [`LookupOutcome::Absent`] and [`LookupOutcome::Reversed`] proceed to
/// the create step.
///
/// Documents are boxed: a queried [`InvoiceDocument`] is large next to the
/// unit variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum LookupOutcome {
    /// Nothing under the external id (code 7), and the hint saw nothing
    /// foreign.
    Absent,
    /// A live document of ours under the external id. The hint is not taken:
    /// nothing will be created.
    Live(Box<InvoiceDocument>),
    /// A reversed document of ours under the external id, and the hint saw
    /// nothing foreign.
    Reversed {
        /// The reversed document.
        document: Box<InvoiceDocument>,
        /// Its storno's number, when the newest document under the order is
        /// the `SS` referencing it; absent otherwise and for correctives.
        storno_number: Option<String>,
    },
    /// The external id resolves to a document that fails validation (another
    /// order, kind, account mode or supplier).
    Collision(Box<InvoiceDocument>),
    /// A live invoice-kind document that is not ours exists under the order
    /// number. Reported even when our own document under the id is reversed:
    /// no create — reissue or not — may proceed past it.
    Foreign(Box<InvoiceDocument>),
    /// szamlazz.hu rejected the agent credentials (3, 135, 136, 164) on the
    /// external-id query or the hint; nothing may be concluded and nothing
    /// will be created. See [`is_credentials_rejected`].
    CredentialsRejected {
        /// The szamlazz.hu code.
        code: String,
        /// The szamlazz.hu message.
        message: String,
    },
    /// A query failed (transport, parse, unavailability or another
    /// szamlazz.hu error); nothing may be concluded.
    Transport(String),
}

/// The create step (design §5 step 4): query the external id, then send the
/// create unless a live document of ours is already there.
///
/// Carries what identifies the document and the create to send. A found
/// document is validated against the gateway's own [`Account`].
#[derive(Debug, Clone)]
pub struct CreateStepRequest<'a> {
    /// The external id the document carries and is looked up by.
    pub external_id: &'a ExternalId,
    /// The kind being issued; a found document must have the matching `tipus`.
    pub kind: IssuedKind,
    /// The order the document belongs to.
    pub order: &'a OrderKey,
    /// The create request built by [`Gateway::build_create`].
    pub create: &'a CreateInvoice,
    /// The number of the reversed document the lookup step saw under the
    /// external id (a reissue). A live document under the id that is not this
    /// one was issued by an earlier execution of the step.
    pub reversed: Option<&'a str>,
}

/// The settled result of the create step: szamlazz.hu's answer is known.
/// What is *not* settled is an [`Unconfirmed`] error, which the run retry
/// policy re-executes.
///
/// Documents are boxed: a queried [`InvoiceDocument`] is large next to the
/// code-and-message variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CreateOutcome {
    /// szamlazz.hu issued the document (or replayed a byte-identical earlier
    /// create — indistinguishable and reported either way). The result
    /// carries a number.
    Issued(InvoiceCreationResult),
    /// A live document of ours is under the external id — found by the
    /// leading query (an earlier execution of this step created it) or by the
    /// re-query after a lost reply. Nothing was sent, or what was sent landed.
    Found(Box<InvoiceDocument>),
    /// szamlazz.hu refused the order number as a duplicate (71/152) and the
    /// external-id re-query found a live document of ours: an earlier send
    /// had landed.
    Reconciled(Box<InvoiceDocument>),
    /// The external id resolves to a document that fails validation (another
    /// order, kind, account mode or supplier). Nothing was created.
    Collision(Box<InvoiceDocument>),
    /// szamlazz.hu refused the order number as a duplicate (71/152) and the
    /// external-id re-query found no live document of ours: the duplicate is
    /// not ours. Never reported for correctives, which are exempt from the
    /// order-number check — their unresolved 71/152 is
    /// [`CreateOutcome::Rejected`].
    DuplicateOrderNumber {
        /// The szamlazz.hu code (`71` or `152`).
        code: String,
        /// The szamlazz.hu message.
        message: String,
        /// The newest document under the order, when it is a live document of
        /// the kind being issued; absent when a document of another kind (or a
        /// reversed one) is newest, and when the naming query itself failed.
        existing_number: Option<String>,
    },
    /// szamlazz.hu refused the document; nothing was created.
    Rejected {
        /// The szamlazz.hu code.
        code: String,
        /// The szamlazz.hu message.
        message: String,
    },
    /// szamlazz.hu rejected the agent credentials (3, 135, 136, 164) on the
    /// leading query, the create or a re-query; this execution issued
    /// nothing. Settled data, not [`Unconfirmed`]: re-executing with the same
    /// key would only repeat the answer. See [`is_credentials_rejected`].
    CredentialsRejected {
        /// The szamlazz.hu code.
        code: String,
        /// The szamlazz.hu message.
        message: String,
    },
}

/// The create or storno step ended without a settled outcome: the run retry
/// policy re-executes the step, whose leading query then finds whatever
/// landed.
///
/// Every variant follows an immediate external-id re-query that found no live
/// document of ours (read-your-writes lag ≈ 0, so "nothing" is not lag).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Unconfirmed {
    /// The HTTP exchange or the response parse failed — on the leading query
    /// (nothing was sent), on the create or storno, or on the re-query itself.
    #[error("transport failure: {0}")]
    Transport(String),
    /// szamlazz.hu reported an open code, one that leaves the outcome open:
    /// 1, 55, 56 without a number, or `szlahu_down`.
    #[error("open code {}: {message}", code.as_deref().unwrap_or("szlahu_down"))]
    Open {
        /// The szamlazz.hu code, when one was reported.
        code: Option<String>,
        /// What was reported.
        message: String,
    },
    /// szamlazz.hu refused the order number as a duplicate (71/152), yet the
    /// order-number query knows nothing under the order. Create only.
    #[error("duplicate order number {code} reported but nothing is under the order: {message}")]
    Contradiction {
        /// The szamlazz.hu code (`71` or `152`).
        code: String,
        /// The szamlazz.hu message.
        message: String,
    },
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

    /// Whether the document belongs to the resolved account: it carries the
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
    /// szamlazz.hu rejected the agent credentials (3, 135, 136, 164); the
    /// check was not made. See [`is_credentials_rejected`].
    CredentialsRejected {
        /// The szamlazz.hu code.
        code: String,
        /// The szamlazz.hu message.
        message: String,
    },
    /// The check itself failed (transport, parse, unavailability or another
    /// szamlazz.hu error); nothing may be concluded.
    Transport(String),
}

/// What the account probe of `Szamlazz.Agent.check_account` learned from one
/// query of the sentinel external id ([`ExternalId::for_probe`]).
///
/// Credential acceptance is the only fact it establishes: szamlazz.hu answers
/// the credential codes before it looks at the request, so any other answer
/// — code 7 above all, since nothing the service issues carries the sentinel
/// id — means the key works. The supplier id appears only in found-document
/// bodies, so a not-found probe cannot cross-check it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ProbeOutcome {
    /// szamlazz.hu accepted the credentials and answered the query (with code
    /// 7, a document, or any other non-credential code).
    Accepted,
    /// szamlazz.hu rejected the agent credentials (3, 135, 136, 164). See
    /// [`is_credentials_rejected`].
    CredentialsRejected {
        /// The szamlazz.hu code.
        code: String,
        /// The szamlazz.hu message.
        message: String,
    },
    /// The exchange itself failed (transport, parse or `szlahu_down`) and
    /// szamlazz.hu's verdict on the credentials is not known.
    Transport(String),
}

/// Why [`Gateway::query_document`] returned no document.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum QueryError {
    /// szamlazz.hu does not know the selector (code 7).
    #[error("szamlazz.hu does not know the document (code 7)")]
    NotFound,
    /// szamlazz.hu rejected the agent credentials (3, 135, 136, 164).
    #[error("szamlazz.hu rejected the agent credentials ({code}): {message}")]
    CredentialsRejected {
        /// The szamlazz.hu code.
        code: String,
        /// The szamlazz.hu message.
        message: String,
    },
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

/// What the storno lookup step found (design §6 step 2), read-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum StornoLookupOutcome {
    /// Nothing under the storno external id (code 7), or a holder that is not
    /// the storno of the invoice — a storno is idempotent server-side, so the
    /// storno step proceeds past a stray holder.
    Absent,
    /// The `SS` reversing the invoice holds the storno external id: a storno
    /// of ours was issued. Nothing will be sent.
    AlreadyReversed {
        /// The storno invoice number.
        storno_number: String,
    },
    /// szamlazz.hu rejected the agent credentials (3, 135, 136, 164); nothing
    /// may be concluded and nothing will be sent. See
    /// [`is_credentials_rejected`].
    CredentialsRejected {
        /// The szamlazz.hu code.
        code: String,
        /// The szamlazz.hu message.
        message: String,
    },
    /// The query failed (transport, parse, unavailability or another
    /// szamlazz.hu error); nothing may be concluded.
    Transport(String),
}

/// The storno step (design §6 step 3): what identifies the storno to send.
#[derive(Debug, Clone, Copy)]
pub struct StornoStepRequest<'a> {
    /// The invoice to reverse.
    pub invoice_number: &'a str,
    /// The external id attached to the storno invoice and queried first
    /// (`{namespace}:{order}:storno:{number}` or `{namespace}:by-number:{number}:storno`).
    pub external_id: &'a ExternalId,
    /// Comment placed on the storno invoice.
    pub comment: Option<&'a str>,
    /// Issue the storno as an e-invoice.
    pub e_invoice: bool,
}

/// The settled result of the storno step: szamlazz.hu's answer is known.
/// What is *not* settled is an [`Unconfirmed`] error, which the run retry
/// policy re-executes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum StornoOutcome {
    /// The invoice is reversed by the storno invoice szamlazz.hu issued (now,
    /// or echoed by an idempotent repeat), validated with
    /// [`CreatedInvoice::reverses`] by [`Gateway::storno`].
    Reversed(CreatedInvoice),
    /// The storno invoice is under the storno external id — found by the
    /// leading query (an earlier execution of this step, or the lookup step's
    /// race, sent it) or by the re-query after a lost reply. Nothing was
    /// sent, or what was sent landed.
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
    /// szamlazz.hu rejected the agent credentials (3, 135, 136, 164) on the
    /// leading query, the storno or a re-query; this execution issued
    /// nothing. Settled data, not [`Unconfirmed`]: re-executing with the same
    /// key would only repeat the answer. See [`is_credentials_rejected`].
    CredentialsRejected {
        /// The szamlazz.hu code.
        code: String,
        /// The szamlazz.hu message.
        message: String,
    },
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
    /// szamlazz.hu rejected the agent credentials (3, 135, 136, 164); nothing
    /// was deleted. See [`is_credentials_rejected`].
    CredentialsRejected {
        /// The szamlazz.hu code.
        code: String,
        /// The szamlazz.hu message.
        message: String,
    },
    /// The HTTP exchange, the response parse or the service failed.
    Transport(String),
}

/// A szamlazz.hu error on a deletion: 335 is [`DeleteOutcome::AlreadyGone`],
/// a credential code [`DeleteOutcome::CredentialsRejected`], anything else
/// [`DeleteOutcome::Rejected`].
impl From<ApiError> for DeleteOutcome {
    fn from(api: ApiError) -> Self {
        if api.code == ErrorCode::ProformaNotFound {
            Self::AlreadyGone
        } else if is_credentials_rejected(&api.code) {
            Self::CredentialsRejected {
                code: api.code.code().to_owned(),
                message: api.message,
            }
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
    /// szamlazz.hu rejected the agent credentials (3, 135, 136, 164); nothing
    /// was registered. See [`is_credentials_rejected`].
    CredentialsRejected {
        /// The szamlazz.hu code.
        code: String,
        /// The szamlazz.hu message.
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

/// A szamlazz.hu error on a registration is a rejection, unless it is a
/// credential code.
impl From<ApiError> for SetPaymentsOutcome {
    fn from(api: ApiError) -> Self {
        if is_credentials_rejected(&api.code) {
            Self::CredentialsRejected {
                code: api.code.code().to_owned(),
                message: api.message,
            }
        } else {
            Self::Rejected {
                code: api.code.code().to_owned(),
                message: api.message,
            }
        }
    }
}

impl Gateway {
    /// Opens the gateway for one handler execution: `account` as resolved
    /// and `credentials` as just fetched, over a **fresh** Számla Agent
    /// client.
    ///
    /// A fresh client every time is a boundary, not a performance choice: the
    /// default `reqwest::Client` keeps szamlazz.hu's `JSESSIONID` cookie, so
    /// a client shared between accounts would carry one account's session
    /// into another account's request.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP client cannot be constructed.
    pub fn open(account: Account, credentials: Credentials) -> Result<Self, BuildError> {
        let client = Client::builder()
            .credentials(credentials)
            .endpoint(account.endpoint.as_str())
            .build()?;
        Ok(Self { client, account })
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

    /// The lookup step (design §5 step 3), read-only.
    ///
    /// 1. Query by external id: a validated live hit is
    ///    [`LookupOutcome::Live`] (the hint is not taken); an invalid hit is
    ///    [`LookupOutcome::Collision`]; code 7 and a validated reversed hit
    ///    continue; rejected credentials are
    ///    [`LookupOutcome::CredentialsRejected`]; any other failure is
    ///    [`LookupOutcome::Transport`].
    /// 2. The order-number hint, for every kind but correctives: a live
    ///    `SZ | ES | VS` that is neither among `our_numbers` nor the document
    ///    seen in step 1 is [`LookupOutcome::Foreign`]. Rejected credentials
    ///    are [`LookupOutcome::CredentialsRejected`]; the hint's own failure
    ///    is otherwise not conclusive and continues.
    /// 3. Otherwise [`LookupOutcome::Absent`], or [`LookupOutcome::Reversed`]
    ///    with the storno number when the hint is the `SS` reversing it.
    pub async fn lookup(&self, request: LookupRequest<'_>) -> LookupOutcome {
        let span = tracing::info_span!(
            "gateway.lookup",
            external_id = %request.external_id,
            kind = %request.kind,
        );
        self.lookup_inner(&request).instrument(span).await
    }

    async fn lookup_inner(&self, request: &LookupRequest<'_>) -> LookupOutcome {
        // Step 1: the external id.
        let reversed = match self
            .seen(request.external_id, request.order, request.kind)
            .await
        {
            Ok(Seen::Absent) => None,
            Ok(Seen::Collision(found)) => return LookupOutcome::Collision(found),
            Ok(Seen::Live(found)) => return LookupOutcome::Live(found),
            Ok(Seen::Reversed(found)) => Some(found),
            Err(QueryError::CredentialsRejected { code, message }) => {
                return LookupOutcome::CredentialsRejected { code, message };
            }
            Err(error) => return LookupOutcome::Transport(error.to_string()),
        };

        // Step 2: the order-number hint; correctives are exempt.
        let mut storno_number = None;
        if request.kind != IssuedKind::Corrective {
            match self.hint_raw(request.order).await {
                Ok(hint) => {
                    let seen = reversed.as_deref().map(InvoiceDocumentExt::number);
                    if is_foreign(&hint, request.our_numbers, seen) {
                        tracing::warn!(
                            number = %hint.number(),
                            tipus = %hint.info.document_type,
                            "foreign document under the order"
                        );
                        return LookupOutcome::Foreign(Box::new(hint));
                    }
                    if let Some(reversed) = &reversed
                        && hint.is_storno_of(reversed.number())
                    {
                        storno_number = Some(hint.number().to_owned());
                    }
                }
                Err(QueryError::CredentialsRejected { code, message }) => {
                    return LookupOutcome::CredentialsRejected { code, message };
                }
                // A miss or an API error says nothing about foreign documents.
                Err(QueryError::NotFound | QueryError::Api { .. }) => {}
                Err(error) => return LookupOutcome::Transport(error.to_string()),
            }
        }

        match reversed {
            Some(document) => LookupOutcome::Reversed {
                document,
                storno_number,
            },
            None => LookupOutcome::Absent,
        }
    }

    /// The create step (design §5 step 4), query-first on every execution.
    ///
    /// 1. Query by external id: a validated live hit that is not
    ///    `request.reversed` is [`CreateOutcome::Found`] — an earlier
    ///    execution created it; an invalid hit is [`CreateOutcome::Collision`];
    ///    code 7 and a reversed hit continue; rejected credentials are
    ///    [`CreateOutcome::CredentialsRejected`]; a failed query is
    ///    [`Unconfirmed::Transport`] — never create when the check itself
    ///    failed.
    /// 2. Send the create: success with a number is [`CreateOutcome::Issued`],
    ///    a refusal [`CreateOutcome::Rejected`], rejected credentials
    ///    [`CreateOutcome::CredentialsRejected`]. A lost reply or an open code
    ///    is re-queried once, immediately: what landed settles the step,
    ///    nothing is [`Unconfirmed`]. 71/152 is re-queried the same way and
    ///    then named through the order-number query.
    ///
    /// # Errors
    ///
    /// [`Unconfirmed`] when the outcome is not settled; the caller's run retry
    /// policy re-executes the step.
    pub async fn create(
        &self,
        request: CreateStepRequest<'_>,
    ) -> Result<CreateOutcome, Unconfirmed> {
        let span = tracing::info_span!(
            "gateway.create",
            external_id = %request.external_id,
            kind = %request.kind,
            reversed = request.reversed,
        );
        self.create_inner(&request).instrument(span).await
    }

    async fn create_inner(
        &self,
        request: &CreateStepRequest<'_>,
    ) -> Result<CreateOutcome, Unconfirmed> {
        // Step 1: the leading query.
        if let Some(settled) = self.settled_by_query(request).await? {
            return Ok(settled);
        }

        // Step 2: create.
        match self.client.send(request.create).await {
            Ok(result) => {
                let Some(number) = &result.invoice_number else {
                    let open = Unconfirmed::Open {
                        code: None,
                        message: "create succeeded without a document number".to_owned(),
                    };
                    return self.settle_or(request, open).await;
                };
                tracing::info!(number = %number, "document issued");
                Ok(CreateOutcome::Issued(result))
            }
            Err(error) => match classify_failure(error) {
                Failure::Rejected { code, message } => {
                    tracing::info!(code = %code, "document rejected");
                    Ok(CreateOutcome::Rejected { code, message })
                }
                Failure::CredentialsRejected { code, message } => {
                    Ok(CreateOutcome::CredentialsRejected { code, message })
                }
                Failure::Unknown { code, message } => {
                    tracing::warn!(code = ?code, "open code; re-querying");
                    self.settle_or(request, Unconfirmed::Open { code, message })
                        .await
                }
                Failure::Transport(message) => {
                    tracing::warn!("transport failure; re-querying");
                    self.settle_or(request, Unconfirmed::Transport(message))
                        .await
                }
                Failure::Duplicate { code, message } => {
                    tracing::info!(code = %code, "duplicate order number; re-querying");
                    self.after_duplicate(request, code, message).await
                }
            },
        }
    }

    /// The immediate re-query after a create whose reply was lost or open:
    /// what landed settles the step; nothing is `unconfirmed`.
    async fn settle_or(
        &self,
        request: &CreateStepRequest<'_>,
        unconfirmed: Unconfirmed,
    ) -> Result<CreateOutcome, Unconfirmed> {
        match self.settled_by_query(request).await? {
            Some(settled) => Ok(settled),
            None => Err(unconfirmed),
        }
    }

    /// The re-query after 71/152: a live document of ours under the id is
    /// [`CreateOutcome::Reconciled`]; a collision is reported as such;
    /// otherwise the duplicate is not ours — the order-number query names it
    /// when the newest document under the order is a live document of the
    /// kind being issued, and its miss is a contradiction.
    ///
    /// Correctives are exempt from the order-number check, so their
    /// unresolved 71/152 is an ordinary [`CreateOutcome::Rejected`], without
    /// an order-number query.
    async fn after_duplicate(
        &self,
        request: &CreateStepRequest<'_>,
        code: String,
        message: String,
    ) -> Result<CreateOutcome, Unconfirmed> {
        match self.settled_by_query(request).await? {
            Some(CreateOutcome::Found(found)) => {
                tracing::info!(number = %found.number(), "reconciled after duplicate");
                return Ok(CreateOutcome::Reconciled(found));
            }
            Some(settled) => return Ok(settled),
            None => {}
        }

        if request.kind == IssuedKind::Corrective {
            tracing::info!(code = %code, "duplicate order number on a corrective: rejected");
            return Ok(CreateOutcome::Rejected { code, message });
        }

        let existing_number = match self.hint_raw(request.order).await {
            Ok(newest)
                if newest.is_live()
                    && newest.info.document_type == document_type_of(request.kind) =>
            {
                Some(newest.number().to_owned())
            }
            Ok(_) => None,
            Err(QueryError::NotFound) => {
                tracing::warn!(code = %code, "duplicate order number but nothing under the order");
                return Err(Unconfirmed::Contradiction { code, message });
            }
            Err(QueryError::CredentialsRejected { code, message }) => {
                return Ok(CreateOutcome::CredentialsRejected { code, message });
            }
            Err(error) => {
                tracing::warn!(error = %error, "could not name the duplicate");
                None
            }
        };
        Ok(CreateOutcome::DuplicateOrderNumber {
            code,
            message,
            existing_number,
        })
    }

    /// The external-id query of the create step: `Some` when it settles the
    /// step — a live document of ours that is not `request.reversed`
    /// ([`CreateOutcome::Found`]) or an invalid holder
    /// ([`CreateOutcome::Collision`]) — `None` when nothing live of ours is
    /// there.
    ///
    /// Rejected credentials settle the step as
    /// [`CreateOutcome::CredentialsRejected`].
    ///
    /// # Errors
    ///
    /// [`Unconfirmed::Transport`] when the query itself failed.
    async fn settled_by_query(
        &self,
        request: &CreateStepRequest<'_>,
    ) -> Result<Option<CreateOutcome>, Unconfirmed> {
        match self
            .seen(request.external_id, request.order, request.kind)
            .await
        {
            Ok(Seen::Collision(found)) => Ok(Some(CreateOutcome::Collision(found))),
            Ok(Seen::Live(found)) if Some(found.number()) != request.reversed => {
                Ok(Some(CreateOutcome::Found(found)))
            }
            // Nothing (code 7), a reversed document, or the document the
            // lookup saw reversed and the server still reports live.
            Ok(Seen::Live(_) | Seen::Reversed(_) | Seen::Absent) => Ok(None),
            Err(QueryError::CredentialsRejected { code, message }) => {
                Ok(Some(CreateOutcome::CredentialsRejected { code, message }))
            }
            Err(error) => Err(Unconfirmed::Transport(error.to_string())),
        }
    }

    /// The external-id query of both steps, validated against this gateway's
    /// account (design §3).
    ///
    /// # Errors
    ///
    /// The failed query (transport, parse, unavailability, rejected
    /// credentials or another szamlazz.hu error); code 7 is [`Seen::Absent`].
    async fn seen(
        &self,
        external_id: &ExternalId,
        order: &OrderKey,
        kind: IssuedKind,
    ) -> Result<Seen, QueryError> {
        let selector = InvoiceSelector::ExternalId(external_id.as_str().to_owned());
        match self.query_raw(selector).await {
            Ok(found) if !self.is_ours(&found, order, kind) => {
                tracing::warn!(number = %found.number(), "external id collision");
                Ok(Seen::Collision(Box::new(found)))
            }
            Ok(found) if found.is_live() => {
                tracing::info!(number = %found.number(), "found live under external id");
                Ok(Seen::Live(Box::new(found)))
            }
            Ok(found) => {
                tracing::info!(number = %found.number(), "found reversed under external id");
                Ok(Seen::Reversed(Box::new(found)))
            }
            Err(QueryError::NotFound) => Ok(Seen::Absent),
            Err(error) => Err(error),
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
        outcome(self.hint_raw(order).await)
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

    /// The account probe of `Szamlazz.Agent.check_account`: one query of the
    /// sentinel `external_id` ([`ExternalId::for_probe`]), whose expected
    /// answer is code 7. Every szamlazz.hu answer but a credential code is
    /// [`ProbeOutcome::Accepted`] — the credential codes come before anything
    /// else, so any other code means the key was accepted; a document under
    /// the sentinel id, which nothing the service issues carries, is logged
    /// and accepted as well. Only a failed exchange (transport, parse,
    /// `szlahu_down`) settles nothing. Issues nothing.
    pub async fn probe(&self, external_id: &ExternalId) -> ProbeOutcome {
        let span = tracing::info_span!("gateway.probe", external_id = %external_id);
        match self
            .query_raw(InvoiceSelector::ExternalId(external_id.as_str().to_owned()))
            .instrument(span)
            .await
        {
            Ok(found) => {
                tracing::warn!(
                    external_id = %external_id,
                    number = %found.number(),
                    "a document carries the probe's sentinel external id; it was not issued by this service"
                );
                ProbeOutcome::Accepted
            }
            Err(QueryError::NotFound) => ProbeOutcome::Accepted,
            Err(QueryError::Api { code, .. }) => {
                tracing::debug!(code, "the probe was answered with a non-credential code");
                ProbeOutcome::Accepted
            }
            Err(QueryError::CredentialsRejected { code, message }) => {
                ProbeOutcome::CredentialsRejected { code, message }
            }
            Err(error @ QueryError::Unavailable(_)) => ProbeOutcome::Transport(error.to_string()),
            Err(QueryError::Transport(message)) => ProbeOutcome::Transport(message),
        }
    }

    /// The storno lookup step (design §6 step 2), read-only: the storno
    /// external id is queried and the `SS` reversing `invoice_number` is
    /// [`StornoLookupOutcome::AlreadyReversed`]; code 7 or another holder is
    /// [`StornoLookupOutcome::Absent`]; rejected credentials are
    /// [`StornoLookupOutcome::CredentialsRejected`]; any other failure is
    /// [`StornoLookupOutcome::Transport`].
    pub async fn lookup_storno(
        &self,
        external_id: &ExternalId,
        invoice_number: &str,
    ) -> StornoLookupOutcome {
        let span = tracing::info_span!(
            "gateway.lookup_storno",
            number = %invoice_number,
            external_id = %external_id,
        );
        match self
            .storno_seen(external_id, invoice_number)
            .instrument(span)
            .await
        {
            Ok(Some(storno_number)) => StornoLookupOutcome::AlreadyReversed { storno_number },
            Ok(None) => StornoLookupOutcome::Absent,
            Err(QueryError::CredentialsRejected { code, message }) => {
                StornoLookupOutcome::CredentialsRejected { code, message }
            }
            Err(error) => StornoLookupOutcome::Transport(error.to_string()),
        }
    }

    /// The storno step (design §6 step 3), query-first on every execution.
    ///
    /// 1. Query the storno external id: an `SS` referencing the invoice is
    ///    [`StornoOutcome::AlreadyReversed`] — an earlier execution sent it;
    ///    code 7 (or another holder) continues; rejected credentials are
    ///    [`StornoOutcome::CredentialsRejected`]; a failed query is
    ///    [`Unconfirmed::Transport`] — never send when the check itself
    ///    failed.
    /// 2. Send `xmlszamlast` with the external id, comment and e-invoice flag
    ///    and **no issue date** (352 otherwise): a response validated with
    ///    [`CreatedInvoice::reverses`] is [`StornoOutcome::Reversed`], an
    ///    echo of the requested number [`StornoOutcome::NotStornoable`], a
    ///    refusal [`StornoOutcome::Rejected`], rejected credentials
    ///    [`StornoOutcome::CredentialsRejected`]. A lost reply or an open
    ///    code is re-queried once, immediately: a landed storno settles the
    ///    step as [`StornoOutcome::AlreadyReversed`], nothing is
    ///    [`Unconfirmed`].
    ///
    /// # Errors
    ///
    /// [`Unconfirmed`] when the outcome is not settled; the caller's run retry
    /// policy re-executes the step.
    pub async fn storno(
        &self,
        request: StornoStepRequest<'_>,
    ) -> Result<StornoOutcome, Unconfirmed> {
        let span = tracing::info_span!(
            "gateway.storno",
            number = %request.invoice_number,
            external_id = %request.external_id,
        );
        self.storno_inner(request).instrument(span).await
    }

    async fn storno_inner(
        &self,
        request: StornoStepRequest<'_>,
    ) -> Result<StornoOutcome, Unconfirmed> {
        // Step 1: the leading query.
        if let Some(settled) = self.storno_settled_by_query(&request).await? {
            return Ok(settled);
        }

        // Step 2: send.
        let mut storno = StornoInvoice::new(request.invoice_number);
        storno.e_invoice = request.e_invoice;
        storno.external_id = Some(request.external_id.as_str().to_owned());
        storno.comment = request.comment.map(str::to_owned);
        storno
            .aggregator
            .clone_from(&self.account.defaults.aggregator);
        storno.guardian = self.account.defaults.guardian;
        storno.issue_date = None;

        match self.client.send(&storno).await {
            Ok(created) if created.reverses(&storno.invoice_number) => {
                tracing::info!(storno_number = %created.invoice_number, "invoice reversed");
                Ok(StornoOutcome::Reversed(created))
            }
            Ok(created) => {
                tracing::info!(echoed = %created.invoice_number, "storno was a no-op");
                Ok(StornoOutcome::NotStornoable)
            }
            Err(error) => match classify_failure(error) {
                Failure::Rejected { code, message } | Failure::Duplicate { code, message } => {
                    tracing::info!(code = %code, "storno rejected");
                    Ok(StornoOutcome::Rejected { code, message })
                }
                Failure::CredentialsRejected { code, message } => {
                    Ok(StornoOutcome::CredentialsRejected { code, message })
                }
                Failure::Unknown { code, message } => {
                    tracing::warn!(code = ?code, "open code; re-querying");
                    self.storno_settle_or(&request, Unconfirmed::Open { code, message })
                        .await
                }
                Failure::Transport(message) => {
                    tracing::warn!("transport failure; re-querying");
                    self.storno_settle_or(&request, Unconfirmed::Transport(message))
                        .await
                }
            },
        }
    }

    /// The immediate re-query after a storno whose reply was lost or open:
    /// a landed storno settles the step; nothing is `unconfirmed`.
    async fn storno_settle_or(
        &self,
        request: &StornoStepRequest<'_>,
        unconfirmed: Unconfirmed,
    ) -> Result<StornoOutcome, Unconfirmed> {
        match self.storno_settled_by_query(request).await? {
            Some(settled) => Ok(settled),
            None => Err(unconfirmed),
        }
    }

    /// The storno-external-id query of the storno step: `Some` when it
    /// settles the step — the `SS` reversing the invoice
    /// ([`StornoOutcome::AlreadyReversed`]) or rejected credentials — `None`
    /// when no storno of ours is there.
    ///
    /// # Errors
    ///
    /// [`Unconfirmed::Transport`] when the query itself failed.
    async fn storno_settled_by_query(
        &self,
        request: &StornoStepRequest<'_>,
    ) -> Result<Option<StornoOutcome>, Unconfirmed> {
        match self
            .storno_seen(request.external_id, request.invoice_number)
            .await
        {
            Ok(Some(storno_number)) => Ok(Some(StornoOutcome::AlreadyReversed { storno_number })),
            Ok(None) => Ok(None),
            Err(QueryError::CredentialsRejected { code, message }) => {
                Ok(Some(StornoOutcome::CredentialsRejected { code, message }))
            }
            Err(error) => Err(Unconfirmed::Transport(error.to_string())),
        }
    }

    /// The storno-external-id query of the storno lookup and storno steps:
    /// the number of the `SS` reversing `invoice_number` when it holds the
    /// id, `None` on code 7 or when the holder is something else (a storno is
    /// idempotent server-side, so proceeding past a stray holder is safe).
    ///
    /// # Errors
    ///
    /// The failed query (transport, parse, unavailability, rejected
    /// credentials or another szamlazz.hu error).
    async fn storno_seen(
        &self,
        external_id: &ExternalId,
        invoice_number: &str,
    ) -> Result<Option<String>, QueryError> {
        let selector = InvoiceSelector::ExternalId(external_id.as_str().to_owned());
        match self.query_raw(selector).await {
            Ok(document) if document.is_storno_of(invoice_number) => {
                let storno_number = document.number().to_owned();
                tracing::info!(storno_number = %storno_number, "storno already issued");
                Ok(Some(storno_number))
            }
            Ok(document) => {
                tracing::warn!(
                    number = %document.number(),
                    tipus = %document.info.document_type,
                    "the storno external id holds another document"
                );
                Ok(None)
            }
            Err(QueryError::NotFound) => Ok(None),
            Err(error) => Err(error),
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

    /// The order-number hint as a raw query result.
    async fn hint_raw(&self, order: &OrderKey) -> Result<InvoiceDocument, QueryError> {
        self.query_raw(InvoiceSelector::OrderNumber(order.as_str().to_owned()))
            .await
    }

    async fn query_raw(&self, selector: InvoiceSelector) -> Result<InvoiceDocument, QueryError> {
        match self.client.send(&QueryInvoiceXml::new(selector)).await {
            Ok(document) => Ok(document),
            Err(ClientError::Api(api)) if api.code == ErrorCode::MissingData => {
                Err(QueryError::NotFound)
            }
            Err(ClientError::Api(api)) if is_credentials_rejected(&api.code) => {
                Err(QueryError::CredentialsRejected {
                    code: api.code.code().to_owned(),
                    message: api.message,
                })
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

/// Whether `code` means szamlazz.hu rejected the agent credentials: 3 invalid
/// credentials, 135 browser session active, 136 login blocked, 164 multiple
/// accounts. szamlazz.hu answers these before it acts on the request (its
/// documentation; unverified on the probe account), so the attempt that sees
/// one has issued nothing — the worker's configuration is wrong, not the request.
#[must_use]
pub fn is_credentials_rejected(code: &ErrorCode) -> bool {
    matches!(
        code,
        ErrorCode::InvalidCredentials
            | ErrorCode::BrowserSessionActive
            | ErrorCode::LoginBlocked
            | ErrorCode::MultipleAccounts
    )
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

/// What the external-id query of the lookup and create steps saw, validated
/// (design §3).
enum Seen {
    /// Code 7.
    Absent,
    /// A live document of ours.
    Live(Box<InvoiceDocument>),
    /// A reversed document of ours.
    Reversed(Box<InvoiceDocument>),
    /// A document that fails validation. Never trusted.
    Collision(Box<InvoiceDocument>),
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
    /// See [`is_credentials_rejected`].
    CredentialsRejected {
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
        ClientError::Api(api) if is_credentials_rejected(&api.code) => {
            Failure::CredentialsRejected {
                code: api.code.code().to_owned(),
                message: api.message,
            }
        }
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

/// Step 2 of [`Gateway::lookup`]: whether the order-number hint is a live
/// invoice-kind document that is neither known to be ours nor the document
/// seen under our external id.
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
        Err(QueryError::CredentialsRejected { code, message }) => {
            QueryOutcome::CredentialsRejected { code, message }
        }
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
    use rust_decimal::dec;
    use szamlazz_agent::wire::{AgentRequest as _, RawResponse};

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

        let malformed = ApiError {
            code: ErrorCode::MalformedXml,
            message: "xml".to_owned(),
        };
        assert_eq!(
            DeleteOutcome::from(malformed.clone()),
            DeleteOutcome::Rejected {
                code: "57".to_owned(),
                message: "xml".to_owned(),
            }
        );
        assert_eq!(
            SetPaymentsOutcome::from(malformed),
            SetPaymentsOutcome::Rejected {
                code: "57".to_owned(),
                message: "xml".to_owned(),
            }
        );

        for code in [
            ErrorCode::InvalidCredentials,
            ErrorCode::BrowserSessionActive,
            ErrorCode::LoginBlocked,
            ErrorCode::MultipleAccounts,
        ] {
            assert!(is_credentials_rejected(&code), "{code:?}");
            let login = ApiError {
                code: code.clone(),
                message: "login".to_owned(),
            };
            assert_eq!(
                DeleteOutcome::from(login.clone()),
                DeleteOutcome::CredentialsRejected {
                    code: code.code().to_owned(),
                    message: "login".to_owned(),
                }
            );
            assert_eq!(
                SetPaymentsOutcome::from(login),
                SetPaymentsOutcome::CredentialsRejected {
                    code: code.code().to_owned(),
                    message: "login".to_owned(),
                }
            );
        }
        for code in [
            ErrorCode::MissingData,
            ErrorCode::Maintenance,
            ErrorCode::ProformaNotFound,
            ErrorCode::Unknown("999".to_owned()),
        ] {
            assert!(!is_credentials_rejected(&code), "{code:?}");
        }
    }
}
