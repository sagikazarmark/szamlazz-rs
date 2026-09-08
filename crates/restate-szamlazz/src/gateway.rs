//! The module that speaks to szamlazz.hu on behalf of one account: one plain
//! async fn per `ctx.run`, over the [`szamlazz_agent::Client`], returning
//! every expected szamlazz.hu outcome **as data**; a rejection, a duplicate
//! order number, a not-found or a no-op storno is a value, never an `Err`.
//! The `Err`s are deliberate and say what a run retry policy may re-execute:
//! the read-only steps return [`Unanswered`] when szamlazz.hu did not answer
//! (a transport or parse failure, `szlahu_down`), and the create and storno
//! steps return [`Unconfirmed`] when szamlazz.hu's answer is *not* known; an
//! answer to a write step's leading query is data too ([`CreateOutcome::Api`],
//! [`CreateOutcome::Unavailable`] and the storno twins): nothing was sent, so
//! nothing is unconfirmed.
//!
//! [`Gateway`] owns the client and the [`Account`] it speaks for; it is not a
//! second client: the Számla Agent `Client` is the transport it wraps.
//! `Szamlazz.Order` calls these inside `ctx.run`; the `Szamlazz.Agent` Restate
//! service is a thin facade over the same functions. Neither Restate service
//! calls the other. Everything the services need to know about the account
//! (its ownership-validation pins, its document defaults) is read through
//! [`Gateway::account`].
//!
//! Every query result is validated before it is called ours:
//! external ids are not unique server-side and the order-number hint returns
//! the most recently issued document of any kind.
//!
//! Tracing events carry external ids, kinds, numbers and codes, never buyer
//! data.
//!
//! # Journaled types are additive-only
//!
//! The outcome types derive `serde` so that the Restate services can journal
//! them as the result of a `ctx.run`. An in-flight invocation replays the
//! entries the *previous* deployment wrote, and an entry the new code cannot
//! decode is a retryable SDK error: the invocation replays into the same
//! failure until its attempts are spent (holding the order key the whole time),
//! and is killed. So every type the services journal is **additive-only**: a
//! new field carries a serde default, a new variant may be added, and no field
//! or variant is renamed, removed or retyped, with one admitted widening: a
//! field `T` may become `Option<T>` when every value the old type wrote decodes
//! to `Some` and re-encodes byte for byte, which the compatibility test proves
//! on the committed fixtures (`InvoiceInfo::test`).
//! This holds for the outcomes here
//! ([`LookupOutcome`], [`CreateOutcome`], [`QueryOutcome`],
//! [`StornoLookupOutcome`], [`StornoOutcome`], [`DeleteOutcome`],
//! [`SetPaymentsOutcome`], [`ProbeOutcome`], [`TaxpayerOutcome`]), for the
//! prologue's journaled [`Account`] and pinned namespace, and for the agent
//! crate's response types the document outcomes carry **as they are**
//! ([`InvoiceDocument`], [`InvoiceCreationResult`], [`CreatedInvoice`] and
//! everything they nest), whose JSON layout is thereby part of this crate's
//! journal contract. [`TaxpayerOutcome`] carries the crate-owned
//! [`QueryTaxpayerResponse`] instead: a projection that is additive-only by
//! the same rule, so a change to the agent crate's `TaxpayerInfo` cannot reach
//! a journaled taxpayer answer. The rule is checked in CI: `service::journal`
//! pins one JSON fixture per variant of every journaled type under
//! `tests/journal/` and replays every fixture ever committed through the
//! current types; the `Journaled` marker trait the run helpers require is the
//! link from the `ctx.run` sites to that directory. A journaled document
//! therefore includes the buyer block szamlazz.hu returned with it.
//!
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
use szamlazz_agent::ops::taxpayer::{QueryTaxpayer, TaxpayerPrefix};
use szamlazz_agent::{
    ApiError, Client, ClientError, Credentials, Date, ErrorCode, InvoiceNumber, OutcomeClass,
    reqwest,
};
use tracing::Instrument as _;

use crate::account::Account;
use crate::contract::{IssuedKind, PaymentEntry, QueryTaxpayerResponse, Selector};
use crate::identity::{ExternalId, OrderKey};

pub mod build;

pub use build::{DocumentRefs, InputError, gross_total};

/// The pseudo-code of a rejection that never reached szamlazz.hu: the request
/// violates the Számla Agent wire contract (a sixth credit entry; a replacing
/// credit-entry request with no entries, which would clear the invoice's
/// payments; a document without line items). Stands beside szamlazz.hu's
/// numeric codes in the `Rejected { code }` outcomes. On a create or storno
/// it is the `rejected` outcome like any other code;
/// `Szamlazz.Agent.set_payments` tells it apart and answers the caller's
/// request as `invalid_input`, since szamlazz.hu answered nothing to pass
/// through.
pub const REQUEST_CODE: &str = "request";

/// The module that speaks to szamlazz.hu for one account: the Számla Agent
/// client plus the [`Account`] it is opened for.
///
/// Opened with [`Gateway::open`] for one handler execution from a resolved
/// account and freshly fetched credentials, or with
/// [`Gateway::open_with_http`] over a caller-built HTTP client.
#[derive(Debug, Clone)]
pub struct Gateway {
    client: Client,
    account: Account,
}

/// The lookup step: what identifies the document whose
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
/// the create step. A lookup szamlazz.hu did not answer is [`Unanswered`],
/// never an outcome.
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
    /// order or kind).
    Collision(Box<InvoiceDocument>),
    /// A live invoice-kind document under the order number that is neither
    /// in `our_numbers` nor the document seen under the external id: another
    /// channel's. Reported even when our own document under the id is
    /// reversed: no create (reissue or not) may proceed past it.
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
    /// szamlazz.hu answered the external-id query with another code: an
    /// answer the step cannot conclude from, and nothing will be created. (On
    /// the hint the same answer says nothing about foreign documents and the
    /// lookup continues.)
    Api {
        /// The szamlazz.hu code.
        code: String,
        /// The szamlazz.hu message.
        message: String,
    },
}

/// The create step: query the external id, then send the
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
    /// external id (a reissue). It is the one holder the step may send past;
    /// a live document that is not this one was issued by an earlier
    /// execution of the step, and a reversed document that is not this one
    /// was reversed since the lookup.
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
    /// create, indistinguishable and reported either way). The result
    /// carries a number.
    Issued(InvoiceCreationResult),
    /// A live document of ours is under the external id, found by the
    /// leading query (an earlier execution of this step created it) or by the
    /// re-query after a lost reply. Nothing was sent, or what was sent landed.
    Found(Box<InvoiceDocument>),
    /// A **reversed** document of ours that the lookup step did not see is
    /// under the external id: an earlier execution of this step (or anyone)
    /// issued it and it was reversed since. Nothing was sent: a reversal
    /// the lookup did not see must be answered as `reversed`, never issued
    /// past (a new document needs an explicit `reissue`).
    Reversed(Box<InvoiceDocument>),
    /// The document the lookup step saw **reversed** is reported **live**
    /// by the leading query or the re-query: the server contradicts itself.
    /// Nothing was sent: sending is the least safe answer to an
    /// inconsistency; the caller sees `conflict{live}` as the lookup would
    /// have reported.
    LiveAgain(Box<InvoiceDocument>),
    /// szamlazz.hu refused the order number as a duplicate (71/152) and the
    /// external-id re-query found a live document of ours: an earlier send
    /// had landed.
    Reconciled(Box<InvoiceDocument>),
    /// The external id resolves to a document that fails validation (another
    /// order or kind). Nothing was created.
    Collision(Box<InvoiceDocument>),
    /// szamlazz.hu refused the order number as a duplicate (71/152) and the
    /// external-id re-query found no live document of ours: the duplicate is
    /// not ours. Never reported for correctives, which are exempt from the
    /// order-number check: their unresolved 71/152 is
    /// [`CreateOutcome::Rejected`].
    DuplicateOrderNumber {
        /// The szamlazz.hu code (`71` or `152`).
        code: String,
        /// The szamlazz.hu message.
        message: String,
        /// The newest document under the order, when it is a live document of
        /// the kind being issued; absent when a document of another kind (or a
        /// reversed one) is newest, when the order-number query knows nothing
        /// under the order (a contradiction, logged at `warn` and settled all
        /// the same), and when the naming query itself failed.
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
    /// szamlazz.hu answered the **leading** query with another code (neither
    /// 7 nor a credential code): an answer the step cannot conclude from, so
    /// nothing was sent. Settled data, as [`LookupOutcome::Api`] is for the
    /// same code one step earlier, not [`Unconfirmed`], which would spend the
    /// issue policy, sized for the post-send window, on a read and report an
    /// answer as silence. The same code on a post-send re-query is
    /// [`Unconfirmed::ReQueryFailed`]: there a send happened.
    Api {
        /// The szamlazz.hu code.
        code: String,
        /// The szamlazz.hu message.
        message: String,
    },
    /// szamlazz.hu reported unavailability (`szlahu_down`) to the **leading**
    /// query: nothing was sent. Settled data for the same reason as
    /// [`CreateOutcome::Api`]. (The lookup step, under the read policy sized
    /// for reads, re-executes on the same answer, [`Unanswered::Unavailable`].)
    Unavailable {
        /// szamlazz.hu's message.
        message: String,
    },
}

/// The create or storno step ended without a settled outcome: the run retry
/// policy re-executes the step, whose leading query then finds whatever
/// landed.
///
/// Reserved for exchanges whose answer is not known. An *answer* to the
/// leading query (another code, `szlahu_down`) is settled data
/// ([`CreateOutcome::Api`], [`CreateOutcome::Unavailable`] and the storno
/// twins), never this. Every variant but [`Unconfirmed::Transport`] on the
/// leading query follows an immediate external-id re-query: one that found no
/// live document of ours (read-your-writes lag ≈ 0, so "nothing" is not lag),
/// or one that failed itself ([`Unconfirmed::ReQueryFailed`], which names
/// both causes).
///
/// The display is what the run journals as its last failure and what the
/// `outcome_unknown` fault repeats on exhaustion: each variant names the
/// cause it stands for.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Unconfirmed {
    /// The HTTP exchange or the response parse failed, on the leading query
    /// (nothing was sent) or on the create or storno.
    #[error("transport failure: {0}")]
    Transport(String),
    /// szamlazz.hu reported an open code to the create or storno, one that
    /// leaves the outcome open: 1, 55, 56 without a number, or a code the
    /// agent crate does not know ([`OutcomeClass::Unknown`]). `code` is
    /// `None` for the one open answer that names no code (success without
    /// a document number), which is as open as a code would be.
    #[error("{}", open_display(code.as_deref(), message))]
    Open {
        /// The szamlazz.hu code, when one was reported.
        code: Option<String>,
        /// What was reported.
        message: String,
    },
    /// szamlazz.hu reported unavailability (`szlahu_down`) to the create or
    /// storno: whether it acted first is not known.
    #[error("szamlazz.hu is unavailable (szlahu_down): {0}")]
    Unavailable(String),
    /// The send ended without a settled answer (`sent` says how), and the
    /// immediate re-query that would have settled it failed itself, so
    /// neither is known. Both are named: the re-query's failure never hides
    /// that a send happened.
    #[error("{sent}; the re-query that would have settled it failed: {re_query}")]
    ReQueryFailed {
        /// How the send ended: the display of the [`Unconfirmed`] it would
        /// have been had the re-query found nothing, or the duplicate-order-
        /// number answer (71/152) the re-query was to resolve.
        sent: String,
        /// The re-query's failure: a transport failure, another code, or
        /// `szlahu_down`.
        re_query: String,
    },
}

/// The display of [`Unconfirmed::Open`]: the code when one was reported,
/// otherwise the one open answer without a code.
fn open_display(code: Option<&str>, message: &str) -> String {
    match code {
        Some(code) => format!("open code {code}: {message}"),
        None => format!("open answer without a code or a document number: {message}"),
    }
}

impl Unconfirmed {
    /// This send-side cause, composed with the failure of the re-query that
    /// would have settled it: [`Unconfirmed::ReQueryFailed`] naming both.
    fn re_query_failed(self, re_query: &QueryError) -> Self {
        Self::ReQueryFailed {
            sent: self.to_string(),
            re_query: re_query.to_string(),
        }
    }
}

/// A read-only step got no answer from szamlazz.hu: the read policy
/// re-executes it. The error of every read fn of the gateway ([`lookup`],
/// [`verify`], [`query`], [`hint`], [`lookup_storno`], [`query_taxpayer`],
/// [`probe`]), and never of a write.
///
/// Every szamlazz.hu *answer* (a document, code 7, rejected credentials,
/// another API code) is the read's data; this is only the exchange that
/// produced none. A read writes nothing, so re-executing it is safe and a
/// re-executed closure's answer is exactly as fresh as a first one; its
/// exhaustion is the handler's `unavailable` fault.
///
/// [`lookup`]: Gateway::lookup
/// [`verify`]: Gateway::verify
/// [`query`]: Gateway::query
/// [`hint`]: Gateway::hint
/// [`lookup_storno`]: Gateway::lookup_storno
/// [`query_taxpayer`]: Gateway::query_taxpayer
/// [`probe`]: Gateway::probe
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Unanswered {
    /// The HTTP exchange or the response parse failed.
    #[error("transport failure: {0}")]
    Transport(String),
    /// szamlazz.hu reported unavailability (`szlahu_down`).
    #[error("szamlazz.hu is unavailable: {0}")]
    Unavailable(String),
}

/// The checks the services make on a queried document before trusting or
/// acting on it.
pub trait InvoiceDocumentExt {
    /// The document number.
    fn number(&self) -> &str;

    /// Whether the document is live: `reversed != Some(true)`.
    fn is_live(&self) -> bool;

    /// Whether the document is the storno invoice (`SS`) reversing `number`.
    fn is_storno_of(&self, number: &str) -> bool;

    /// Whether it is an e-invoice; `None` for non-invoices (proformas) and
    /// unknown `eszamla` codes, where the account default applies.
    ///
    /// What the storno handlers send as the storno's `eszamla`. szamlazz.hu
    /// does not require a storno's form to match its original's: a mismatch
    /// is accepted silently and the storno document takes the request's flag
    /// (P73), so this derivation, not the server, is what keeps a reversal
    /// in its original's form. `1` is paper and `2`/`3` are e-invoice codes,
    /// as the vendor annotation says and the test account confirmed (`3` for
    /// an invoice created with `eszamla=true`).
    fn e_invoice(&self) -> Option<bool>;

    /// Registered credit entry amounts, in the order szamlazz.hu lists them.
    fn payment_amounts(&self) -> Vec<Decimal>;

    /// The order number the document carries (`rendelesszam`), trimmed as
    /// szamlazz.hu matches it; `None` when the element is absent, empty or
    /// whitespace only: a document issued outside any order. The one reading
    /// of the element: what `Szamlazz.Agent.storno` answers as
    /// `managed_by_order`'s `order_key`, and what [`Self::carries_order`]
    /// compares with the key.
    fn order_number(&self) -> Option<&str>;

    /// Whether the document carries `order` as its [order
    /// number](Self::order_number). What makes a document found by number
    /// this order's to act on or link.
    fn carries_order(&self, order: &OrderKey) -> bool;

    /// Whether the document is ours: it [carries
    /// `order`](Self::carries_order) and the `tipus` of `kind`. Nothing about
    /// the account: the worker holds no account pin (`teszt` and `szallito/id`
    /// are parsed, never compared).
    fn is_ours(&self, order: &OrderKey, kind: IssuedKind) -> bool;
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

    fn order_number(&self) -> Option<&str> {
        self.info
            .order_number
            .as_deref()
            .map(str::trim)
            .filter(|order| !order.is_empty())
    }

    fn carries_order(&self, order: &OrderKey) -> bool {
        self.order_number() == Some(order.as_str())
    }

    fn is_ours(&self, order: &OrderKey, kind: IssuedKind) -> bool {
        self.carries_order(order) && self.info.document_type == document_type_of(kind)
    }
}

/// The answered result of a query by number, external id or order number
/// ([`Gateway::verify`], [`Gateway::query`], [`Gateway::hint`]). A query
/// szamlazz.hu did not answer is [`Unanswered`], never an outcome.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum QueryOutcome {
    /// The document.
    Found(Box<InvoiceDocument>),
    /// szamlazz.hu does not know the selector (code 7): unknown number, order
    /// number or external id, or a deleted / consumed proforma.
    NotFound,
    /// szamlazz.hu rejected the agent credentials (3, 135, 136, 164); the
    /// check was not made. See [`is_credentials_rejected`].
    CredentialsRejected {
        /// The szamlazz.hu code.
        code: String,
        /// The szamlazz.hu message.
        message: String,
    },
    /// szamlazz.hu answered with another code: an answer the caller cannot
    /// conclude a document from.
    Api {
        /// The szamlazz.hu code.
        code: String,
        /// The szamlazz.hu message.
        message: String,
    },
}

/// What the account probe of `Szamlazz.Agent.check_account` learned from one
/// query of the sentinel external id ([`ExternalId::for_probe`]).
///
/// Credential acceptance is the only fact it establishes: szamlazz.hu answers
/// the credential codes before it looks at the request, so any other answer
/// (code 7 above all, since nothing the service issues carries the sentinel
/// id) means the key works. *Which* account the key opens it cannot tell: a
/// not-found probe has no document to read, and no operation answers "which
/// account am I?"; that is the operator's go-live check. An exchange that
/// produced no answer is [`Unanswered`], never an outcome.
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
}

/// What the taxpayer query of `Szamlazz.Agent.query_taxpayer` learned from
/// one `xmltaxpayer` exchange ([`Gateway::query_taxpayer`]).
///
/// Every szamlazz.hu answer is data: NAV's verdict on the prefix (valid or
/// not) is [`TaxpayerOutcome::Found`], a credential code is
/// [`TaxpayerOutcome::CredentialsRejected`], any other `funcCode ≠ OK` (a
/// NAV-side failure szamlazz.hu relays, a szamlazz.hu code of its own) is
/// [`TaxpayerOutcome::Api`]. An exchange that produced no answer is
/// [`Unanswered`], never an outcome. Journaled as the read step's result, so
/// additive-only; it carries the crate-owned [`QueryTaxpayerResponse`], never
/// the agent crate's `TaxpayerInfo`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TaxpayerOutcome {
    /// NAV answered: the taxpayer as registered, or `valid: false`.
    Found(QueryTaxpayerResponse),
    /// szamlazz.hu rejected the agent credentials (3, 135, 136, 164). See
    /// [`is_credentials_rejected`].
    CredentialsRejected {
        /// The szamlazz.hu code.
        code: String,
        /// The szamlazz.hu message.
        message: String,
    },
    /// szamlazz.hu answered with another code: its own, or NAV's
    /// `errorCode` relayed under `funcCode ERROR`.
    Api {
        /// The code.
        code: String,
        /// The message.
        message: String,
    },
}

/// Why a raw query returned no document: szamlazz.hu's answers as the
/// gateway classifies them internally, before each read fn splits them into
/// its outcome (the answers: 7, a credential code, another code) and
/// [`Unanswered`] (the rest).
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

/// What szamlazz.hu answered a query with, when it answered without a
/// document: the data side of [`QueryError::answered`].
#[derive(Debug, Clone, PartialEq, Eq)]
enum Answer {
    /// Code 7.
    NotFound,
    /// A credential code (3, 135, 136, 164).
    CredentialsRejected { code: String, message: String },
    /// Any other code.
    Api { code: String, message: String },
}

impl QueryError {
    /// Splits the error into what szamlazz.hu answered and what it did not:
    /// `Ok` is an [`Answer`] for the read fn to turn into its outcome, `Err`
    /// the [`Unanswered`] exchange the read policy re-executes.
    fn answered(self) -> Result<Answer, Unanswered> {
        match self {
            Self::NotFound => Ok(Answer::NotFound),
            Self::CredentialsRejected { code, message } => {
                Ok(Answer::CredentialsRejected { code, message })
            }
            Self::Api { code, message } => Ok(Answer::Api { code, message }),
            Self::Unavailable(message) => Err(Unanswered::Unavailable(message)),
            Self::Transport(message) => Err(Unanswered::Transport(message)),
        }
    }
}

/// What the storno lookup step found, read-only. A query
/// szamlazz.hu did not answer is [`Unanswered`], never an outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum StornoLookupOutcome {
    /// Nothing under the storno external id (code 7), or a holder that is not
    /// the storno of the invoice: a storno is idempotent server-side, so the
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
    /// szamlazz.hu answered with another code: an answer the step cannot
    /// conclude from, and nothing will be sent.
    Api {
        /// The szamlazz.hu code.
        code: String,
        /// The szamlazz.hu message.
        message: String,
    },
}

/// The storno step: what identifies the storno to send.
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
    /// The storno's `teljesitesDatum`: the verified original's `telj`, which
    /// NAV requires the storno to repeat. A pure function of the
    /// journaled verify result, so every execution of the step sends the
    /// same date.
    pub fulfillment_date: Date,
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
    /// The storno invoice is under the storno external id, found by the
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
    /// szamlazz.hu answered the **leading** query with another code (neither
    /// 7 nor a credential code): nothing was sent. Settled data, as
    /// [`CreateOutcome::Api`] is for the create step.
    Api {
        /// The szamlazz.hu code.
        code: String,
        /// The szamlazz.hu message.
        message: String,
    },
    /// szamlazz.hu reported unavailability (`szlahu_down`) to the **leading**
    /// query: nothing was sent. Settled data, as [`CreateOutcome::Unavailable`]
    /// is for the create step.
    Unavailable {
        /// szamlazz.hu's message.
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
        /// The szamlazz.hu code, or [`REQUEST_CODE`] for a wire-contract
        /// violation that never reached szamlazz.hu.
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

    /// Opens the gateway as [`Gateway::open`] does, over the caller's own
    /// [`reqwest::Client`] instead of a default one: the embedder's hook for a
    /// proxy or a custom TLS setup. The Számla Agent crate's
    /// [`ClientBuilder::http_client`](szamlazz_agent::client::ClientBuilder::http_client)
    /// is the same hook one level down, and what the default client sets is
    /// then the caller's to set: `.cookie_store(true)` so the `JSESSIONID`
    /// session is reused, a timeout (the default client's
    /// [`REQUEST_TIMEOUT`](szamlazz_agent::client::REQUEST_TIMEOUT) is not
    /// applied to a supplied client), and `redirect(Policy::none())`, since
    /// following a redirect would turn the multipart POST into a body-less
    /// GET.
    ///
    /// The fresh-client-per-execution boundary of [`Gateway::open`] becomes
    /// the caller's to keep: two gateways of two accounts opened over one
    /// `http` share its cookie jar, and with it one account's session.
    ///
    /// This crate's own unit and wiremock tests open every gateway through it,
    /// over a client with no root certificates, so that none of them parses
    /// the system CA store for a plain-`http://` mock; the e2e suite's
    /// deployment opens its gateways through the prologue's [`Gateway::open`].
    ///
    /// # Errors
    ///
    /// Returns an error when the Számla Agent client cannot be built; nothing
    /// on this path constructs an HTTP client, so the error is the builder's
    /// contract rather than an outcome this crate has seen.
    pub fn open_with_http(
        account: Account,
        credentials: Credentials,
        http: reqwest::Client,
    ) -> Result<Self, BuildError> {
        let client = Client::builder()
            .credentials(credentials)
            .endpoint(account.endpoint.as_str())
            .http_client(http)
            .build()?;
        Ok(Self { client, account })
    }

    /// The account the gateway speaks for: the only way the services read
    /// account configuration.
    #[must_use]
    pub fn account(&self) -> &Account {
        &self.account
    }

    /// Whether `found` is ours for `order` and `kind`.
    fn is_ours(found: &InvoiceDocument, order: &OrderKey, kind: IssuedKind) -> bool {
        found.is_ours(order, kind)
    }

    /// The lookup step, read-only.
    ///
    /// 1. Query by external id: a validated live hit is
    ///    [`LookupOutcome::Live`] (the hint is not taken); an invalid hit is
    ///    [`LookupOutcome::Collision`]; code 7 and a validated reversed hit
    ///    continue; rejected credentials are
    ///    [`LookupOutcome::CredentialsRejected`]; another szamlazz.hu code is
    ///    [`LookupOutcome::Api`].
    /// 2. The order-number hint, for every kind but correctives: a live
    ///    `SZ | ES | VS` that is neither among `our_numbers` nor the document
    ///    seen in step 1 is [`LookupOutcome::Foreign`]. Rejected credentials
    ///    are [`LookupOutcome::CredentialsRejected`]; a miss or another code
    ///    is not conclusive and continues.
    /// 3. Otherwise [`LookupOutcome::Absent`], or [`LookupOutcome::Reversed`]
    ///    with the storno number when the hint is the `SS` reversing it.
    ///
    /// # Errors
    ///
    /// [`Unanswered`] when either query got no answer (transport, parse,
    /// `szlahu_down`); the caller's read policy re-executes the step.
    pub async fn lookup(&self, request: LookupRequest<'_>) -> Result<LookupOutcome, Unanswered> {
        let span = tracing::info_span!(
            "gateway.lookup",
            external_id = %request.external_id,
            kind = %request.kind,
        );
        self.lookup_inner(&request).instrument(span).await
    }

    async fn lookup_inner(&self, request: &LookupRequest<'_>) -> Result<LookupOutcome, Unanswered> {
        // Step 1: the external id.
        let reversed = match self
            .seen(request.external_id, request.order, request.kind)
            .await
        {
            Ok(Seen::Collision(found)) => return Ok(LookupOutcome::Collision(found)),
            Ok(Seen::Live(found)) => return Ok(LookupOutcome::Live(found)),
            Ok(Seen::Reversed(found)) => Some(found),
            Ok(Seen::Absent) => None,
            Err(error) => match error.answered()? {
                // `seen` maps code 7 to `Seen::Absent`; the arm keeps the
                // match exhaustive.
                Answer::NotFound => None,
                Answer::CredentialsRejected { code, message } => {
                    return Ok(LookupOutcome::CredentialsRejected { code, message });
                }
                Answer::Api { code, message } => {
                    tracing::warn!(code = %code, "the external-id query was answered with another code");
                    return Ok(LookupOutcome::Api { code, message });
                }
            },
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
                        return Ok(LookupOutcome::Foreign(Box::new(hint)));
                    }
                    if let Some(reversed) = &reversed
                        && hint.is_storno_of(reversed.number())
                    {
                        storno_number = Some(hint.number().to_owned());
                    }
                }
                Err(error) => match error.answered()? {
                    Answer::CredentialsRejected { code, message } => {
                        return Ok(LookupOutcome::CredentialsRejected { code, message });
                    }
                    // A miss or another code says nothing about foreign
                    // documents.
                    Answer::NotFound | Answer::Api { .. } => {}
                },
            }
        }

        Ok(match reversed {
            Some(document) => LookupOutcome::Reversed {
                document,
                storno_number,
            },
            None => LookupOutcome::Absent,
        })
    }

    /// The create step, query-first on every execution.
    ///
    /// 1. Query by external id: a validated live hit that is not
    ///    `request.reversed` is [`CreateOutcome::Found`] (an earlier
    ///    execution created it); a validated **reversed** hit that is not
    ///    `request.reversed` is [`CreateOutcome::Reversed`] (issued and
    ///    reversed since the lookup); `request.reversed` reported live is
    ///    [`CreateOutcome::LiveAgain`]; an invalid hit is
    ///    [`CreateOutcome::Collision`]; code 7 and `request.reversed` still
    ///    reversed continue; rejected credentials are
    ///    [`CreateOutcome::CredentialsRejected`]; another code is
    ///    [`CreateOutcome::Api`] and `szlahu_down` [`CreateOutcome::Unavailable`]
    ///    (answers, settled with nothing sent); only a transport failure
    ///    is [`Unconfirmed::Transport`]: never create when the check itself
    ///    failed. The rule: the step sends only when the external id holds
    ///    nothing, or exactly the document the lookup step saw reversed.
    /// 2. Send the create: success with a number is [`CreateOutcome::Issued`],
    ///    a refusal [`CreateOutcome::Rejected`], rejected credentials
    ///    [`CreateOutcome::CredentialsRejected`]. A lost reply, an open code
    ///    or `szlahu_down` is re-queried once, immediately: what landed
    ///    settles the step, nothing is [`Unconfirmed`], and a re-query that
    ///    fails itself is [`Unconfirmed::ReQueryFailed`] naming both. 71/152
    ///    is re-queried the same way and then named through the order-number
    ///    query.
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
        // Step 1: the leading query. An answer settles the step: nothing
        // has been sent; only an exchange without one is unconfirmed.
        match self.settled_by_query(request).await {
            Ok(Some(settled)) => return Ok(settled),
            // Nothing under the id, or the lookup's reversed document still
            // reversed: send. (`seen` settles 7 as `Absent`; the `NotFound`
            // arm keeps the match exhaustive and is right if reached.)
            Ok(None) | Err(QueryError::NotFound) => {}
            Err(QueryError::Api { code, message }) => {
                tracing::warn!(code = %code, "the leading query was answered with another code");
                return Ok(CreateOutcome::Api { code, message });
            }
            Err(QueryError::Unavailable(message)) => {
                tracing::warn!("the leading query was answered with szlahu_down");
                return Ok(CreateOutcome::Unavailable { message });
            }
            // `settled_by_query` settles the credential codes; likewise.
            Err(QueryError::CredentialsRejected { code, message }) => {
                return Ok(CreateOutcome::CredentialsRejected { code, message });
            }
            Err(QueryError::Transport(message)) => return Err(Unconfirmed::Transport(message)),
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
                    tracing::warn!(code = %code, "open code; re-querying");
                    let open = Unconfirmed::Open {
                        code: Some(code),
                        message,
                    };
                    self.settle_or(request, open).await
                }
                Failure::Unavailable(message) => {
                    tracing::warn!("szlahu_down on the create; re-querying");
                    self.settle_or(request, Unconfirmed::Unavailable(message))
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
    /// what landed settles the step; nothing is `unconfirmed`, and a re-query
    /// that fails itself is unconfirmed naming both causes.
    async fn settle_or(
        &self,
        request: &CreateStepRequest<'_>,
        unconfirmed: Unconfirmed,
    ) -> Result<CreateOutcome, Unconfirmed> {
        match self.settled_by_query(request).await {
            Ok(Some(settled)) => Ok(settled),
            Ok(None) => Err(unconfirmed),
            Err(error) => Err(unconfirmed.re_query_failed(&error)),
        }
    }

    /// The re-query after 71/152: a live document of ours under the id is
    /// [`CreateOutcome::Reconciled`]; every other settled answer of the query
    /// (a collision, a document reversed since the lookup, the lookup's
    /// reversed document live again) is reported as such; otherwise the
    /// duplicate is not ours: [`CreateOutcome::DuplicateOrderNumber`], named
    /// through the order-number query when the newest document under the
    /// order is a live document of the kind being issued. The query's miss is
    /// a contradiction (szamlazz.hu refused the order number yet knows nothing
    /// under it), logged at `warn` and settled all the same: the refusal is
    /// an answer szamlazz.hu already gave, and re-sending would only repeat
    /// it.
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
        match self.settled_by_query(request).await {
            Ok(Some(CreateOutcome::Found(found))) => {
                tracing::info!(number = %found.number(), "reconciled after duplicate");
                return Ok(CreateOutcome::Reconciled(found));
            }
            Ok(Some(settled)) => return Ok(settled),
            Ok(None) => {}
            // Whether the duplicate is ours is what the re-query was to
            // settle; unconfirmed, naming the refusal it was resolving.
            Err(error) => {
                return Err(Unconfirmed::ReQueryFailed {
                    sent: format!("duplicate order number {code}: {message}"),
                    re_query: error.to_string(),
                });
            }
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
                // A contradiction (szamlazz.hu refused the order number yet
                // knows nothing under it), but still a refusal it has already
                // given: settled, not re-sent.
                tracing::warn!(
                    code = %code,
                    order = %request.order,
                    "duplicate order number reported but nothing is under the order"
                );
                None
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

    /// The external-id query of the create step, decided by
    /// [`settle_create`] against `request.reversed`: `Some` when it settles
    /// the step, `None` when the send may proceed.
    ///
    /// # Errors
    ///
    /// The query's own failure, for the caller to place: on the leading query
    /// an answer (another code, `szlahu_down`) is settled data and only a
    /// transport failure is [`Unconfirmed`]; after a send every failure
    /// leaves the step unconfirmed.
    async fn settled_by_query(
        &self,
        request: &CreateStepRequest<'_>,
    ) -> Result<Option<CreateOutcome>, QueryError> {
        settle_create(
            self.seen(request.external_id, request.order, request.kind)
                .await,
            request.reversed,
        )
    }

    /// The external-id query of both steps, validated against this gateway's
    /// account.
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
            Ok(found) if !Self::is_ours(&found, order, kind) => {
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
    ///
    /// # Errors
    ///
    /// [`Unanswered`] when the query got no answer; the caller's read policy
    /// re-executes the step.
    pub async fn verify(&self, number: &str) -> Result<QueryOutcome, Unanswered> {
        outcome(
            self.query_raw(InvoiceSelector::InvoiceNumber(InvoiceNumber::new(number)))
                .await,
        )
    }

    /// Queries by any selector (also the `Szamlazz.Agent.query` handler's
    /// one step).
    ///
    /// # Errors
    ///
    /// [`Unanswered`] when the query got no answer; the caller's read policy
    /// re-executes the step.
    pub async fn query(&self, selector: &Selector) -> Result<QueryOutcome, Unanswered> {
        outcome(self.query_raw(invoice_selector(selector)).await)
    }

    /// The order-number hint: the most recently issued document of any kind
    /// carrying the order number.
    ///
    /// # Errors
    ///
    /// [`Unanswered`] when the query got no answer; the caller's read policy
    /// re-executes the step.
    pub async fn hint(&self, order: &OrderKey) -> Result<QueryOutcome, Unanswered> {
        outcome(self.hint_raw(order).await)
    }

    /// The account probe of `Szamlazz.Agent.check_account`: one query of the
    /// sentinel `external_id` ([`ExternalId::for_probe`]), whose expected
    /// answer is code 7. Every szamlazz.hu answer but a credential code is
    /// [`ProbeOutcome::Accepted`]: the credential codes come before anything
    /// else, so any other code means the key was accepted; a document under
    /// the sentinel id, which nothing the service issues carries, is logged
    /// and accepted as well. Issues nothing.
    ///
    /// # Errors
    ///
    /// [`Unanswered`] when the exchange produced no answer (transport, parse,
    /// `szlahu_down`): szamlazz.hu's verdict on the credentials is not known,
    /// and the caller's read policy re-executes the step.
    pub async fn probe(&self, external_id: &ExternalId) -> Result<ProbeOutcome, Unanswered> {
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
                Ok(ProbeOutcome::Accepted)
            }
            Err(error) => match error.answered()? {
                Answer::NotFound => Ok(ProbeOutcome::Accepted),
                Answer::Api { code, .. } => {
                    tracing::debug!(code, "the probe was answered with a non-credential code");
                    Ok(ProbeOutcome::Accepted)
                }
                Answer::CredentialsRejected { code, message } => {
                    Ok(ProbeOutcome::CredentialsRejected { code, message })
                }
            },
        }
    }

    /// The taxpayer lookup of `Szamlazz.Agent.query_taxpayer`: one
    /// `xmltaxpayer` query of the eight-digit `prefix`, read-only. NAV's
    /// verdict (the registered taxpayer, or `valid: false`) is
    /// [`TaxpayerOutcome::Found`]; rejected credentials are
    /// [`TaxpayerOutcome::CredentialsRejected`]; any other code, szamlazz.hu's
    /// or NAV's relayed one, is [`TaxpayerOutcome::Api`]. Finds no document,
    /// so there are no account pins to check. Issues nothing.
    ///
    /// # Errors
    ///
    /// [`Unanswered`] when the exchange produced no answer (transport, parse,
    /// `szlahu_down`); the caller's read policy re-executes the step.
    pub async fn query_taxpayer(
        &self,
        prefix: &TaxpayerPrefix,
    ) -> Result<TaxpayerOutcome, Unanswered> {
        let span = tracing::info_span!("gateway.query_taxpayer", prefix = %prefix.as_str());
        let request = QueryTaxpayer::from(prefix.clone());
        match self.client.send(&request).instrument(span).await {
            Ok(info) => {
                tracing::debug!(prefix = %prefix.as_str(), valid = info.valid, "taxpayer answered");
                Ok(TaxpayerOutcome::Found(QueryTaxpayerResponse::from(info)))
            }
            Err(ClientError::Api(api)) if is_credentials_rejected(&api.code) => {
                Ok(TaxpayerOutcome::CredentialsRejected {
                    code: api.code.code().to_owned(),
                    message: api.message,
                })
            }
            Err(ClientError::Api(api)) => Ok(TaxpayerOutcome::Api {
                code: api.code.code().to_owned(),
                message: api.message,
            }),
            Err(ClientError::ServiceUnavailable(message)) => Err(Unanswered::Unavailable(message)),
            Err(error) => Err(Unanswered::Transport(error.to_string())),
        }
    }

    /// The storno lookup step, read-only: the storno
    /// external id is queried and the `SS` reversing `invoice_number` is
    /// [`StornoLookupOutcome::AlreadyReversed`]; code 7 or another holder is
    /// [`StornoLookupOutcome::Absent`]; rejected credentials are
    /// [`StornoLookupOutcome::CredentialsRejected`]; another szamlazz.hu code
    /// is [`StornoLookupOutcome::Api`].
    ///
    /// # Errors
    ///
    /// [`Unanswered`] when the query got no answer; the caller's read policy
    /// re-executes the step.
    pub async fn lookup_storno(
        &self,
        external_id: &ExternalId,
        invoice_number: &str,
    ) -> Result<StornoLookupOutcome, Unanswered> {
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
            Ok(Some(storno_number)) => Ok(StornoLookupOutcome::AlreadyReversed { storno_number }),
            Ok(None) => Ok(StornoLookupOutcome::Absent),
            Err(error) => match error.answered()? {
                // `storno_seen` maps code 7 to `None`; the arm keeps the
                // match exhaustive.
                Answer::NotFound => Ok(StornoLookupOutcome::Absent),
                Answer::CredentialsRejected { code, message } => {
                    Ok(StornoLookupOutcome::CredentialsRejected { code, message })
                }
                Answer::Api { code, message } => {
                    tracing::warn!(code = %code, "the storno lookup was answered with another code");
                    Ok(StornoLookupOutcome::Api { code, message })
                }
            },
        }
    }

    /// The storno step, query-first on every execution.
    ///
    /// 1. Query the storno external id: an `SS` referencing the invoice is
    ///    [`StornoOutcome::AlreadyReversed`] (an earlier execution sent it);
    ///    code 7 (or another holder) continues; rejected credentials are
    ///    [`StornoOutcome::CredentialsRejected`]; another code is
    ///    [`StornoOutcome::Api`] and `szlahu_down` [`StornoOutcome::Unavailable`]
    ///    (answers, settled with nothing sent); only a transport failure
    ///    is [`Unconfirmed::Transport`]: never send when the check itself
    ///    failed.
    /// 2. Send `xmlszamlast` with the external id, comment, e-invoice flag and
    ///    `teljesitesDatum` (the verified original's `telj`, which NAV
    ///    requires the storno to repeat), and **no issue date**
    ///    (352 otherwise): a response validated with
    ///    [`CreatedInvoice::reverses`] is [`StornoOutcome::Reversed`], an
    ///    echo of the requested number [`StornoOutcome::NotStornoable`], a
    ///    refusal [`StornoOutcome::Rejected`], rejected credentials
    ///    [`StornoOutcome::CredentialsRejected`]. A lost reply, an open code
    ///    or `szlahu_down` is re-queried once, immediately: a landed storno
    ///    settles the step as [`StornoOutcome::AlreadyReversed`], nothing is
    ///    [`Unconfirmed`], and a re-query that fails itself is
    ///    [`Unconfirmed::ReQueryFailed`] naming both.
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
        // Step 1: the leading query. An answer settles the step: nothing
        // has been sent; only an exchange without one is unconfirmed.
        match self.storno_settled_by_query(&request).await {
            Ok(Some(settled)) => return Ok(settled),
            // No storno of ours under the id: send. (`storno_seen` settles 7
            // as `None`; the `NotFound` arm keeps the match exhaustive and is
            // right if reached.)
            Ok(None) | Err(QueryError::NotFound) => {}
            Err(QueryError::Api { code, message }) => {
                tracing::warn!(code = %code, "the leading query was answered with another code");
                return Ok(StornoOutcome::Api { code, message });
            }
            Err(QueryError::Unavailable(message)) => {
                tracing::warn!("the leading query was answered with szlahu_down");
                return Ok(StornoOutcome::Unavailable { message });
            }
            // `storno_settled_by_query` settles the credential codes; likewise.
            Err(QueryError::CredentialsRejected { code, message }) => {
                return Ok(StornoOutcome::CredentialsRejected { code, message });
            }
            Err(QueryError::Transport(message)) => return Err(Unconfirmed::Transport(message)),
        }

        // Step 2: send.
        let storno = StornoInvoice {
            e_invoice: request.e_invoice,
            external_id: Some(request.external_id.as_str().to_owned()),
            comment: request.comment.map(str::to_owned),
            aggregator: self.account.defaults.aggregator.clone(),
            guardian: self.account.defaults.guardian,
            issue_date: None,
            fulfillment_date: Some(request.fulfillment_date),
            ..StornoInvoice::new(request.invoice_number)
        };

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
                    tracing::warn!(code = %code, "open code; re-querying");
                    let open = Unconfirmed::Open {
                        code: Some(code),
                        message,
                    };
                    self.storno_settle_or(&request, open).await
                }
                Failure::Unavailable(message) => {
                    tracing::warn!("szlahu_down on the storno; re-querying");
                    self.storno_settle_or(&request, Unconfirmed::Unavailable(message))
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
    /// a landed storno settles the step; nothing is `unconfirmed`, and a
    /// re-query that fails itself is unconfirmed naming both causes.
    async fn storno_settle_or(
        &self,
        request: &StornoStepRequest<'_>,
        unconfirmed: Unconfirmed,
    ) -> Result<StornoOutcome, Unconfirmed> {
        match self.storno_settled_by_query(request).await {
            Ok(Some(settled)) => Ok(settled),
            Ok(None) => Err(unconfirmed),
            Err(error) => Err(unconfirmed.re_query_failed(&error)),
        }
    }

    /// The storno-external-id query of the storno step, decided by
    /// [`settle_storno`]: `Some` when it settles the step, `None` when no
    /// storno of ours is there.
    ///
    /// # Errors
    ///
    /// The query's own failure, for the caller to place: on the leading query
    /// an answer (another code, `szlahu_down`) is settled data and only a
    /// transport failure is [`Unconfirmed`]; after a send every failure
    /// leaves the step unconfirmed.
    async fn storno_settled_by_query(
        &self,
        request: &StornoStepRequest<'_>,
    ) -> Result<Option<StornoOutcome>, QueryError> {
        settle_storno(
            self.storno_seen(request.external_id, request.invoice_number)
                .await,
        )
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
                    code: REQUEST_CODE.to_owned(),
                    message: error.to_string(),
                };
            }
        };
        let request = RegisterCreditEntry {
            additive,
            entries: credit_entries,
            aggregator: self.account.defaults.aggregator.clone(),
            ..RegisterCreditEntry::new(number)
        };

        match self.client.send(&request).await {
            Ok(result) => {
                tracing::info!(number = %number, additive, "credit entries registered");
                SetPaymentsOutcome::from(result)
            }
            Err(ClientError::Api(api)) => SetPaymentsOutcome::from(api),
            Err(ClientError::Request(error)) => SetPaymentsOutcome::Rejected {
                code: REQUEST_CODE.to_owned(),
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
/// documentation; unverified on the probe account), so the request that
/// draws one was not acted on: the worker's configuration is wrong, not the
/// request.
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
pub(crate) const fn document_type_of(kind: IssuedKind) -> &'static str {
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
pub(crate) fn issued_kind_of(tipus: &str) -> Option<IssuedKind> {
    match tipus {
        "D" => Some(IssuedKind::Proforma),
        "SZ" => Some(IssuedKind::Invoice),
        "ES" => Some(IssuedKind::Prepayment),
        "VS" => Some(IssuedKind::Final),
        "HS" => Some(IssuedKind::Corrective),
        _ => None,
    }
}

/// Whether `tipus` is a legal invoice of the kinds an order carries: `SZ`,
/// `ES` or `VS`. Stornos, correctives, proformas and delivery notes are
/// not.
#[must_use]
pub(crate) fn is_invoice_family(tipus: &str) -> bool {
    matches!(tipus, "SZ" | "ES" | "VS")
}

/// What the external-id query of the lookup and create steps saw, validated.
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
///
/// The API-code arms come from [`ErrorCode::outcome_class`]; the worker adds
/// [`Failure::CredentialsRejected`] on top (a fault of its configuration, not
/// of the request) and keeps the transport/parse failures apart from
/// `szlahu_down` because [`Unconfirmed`] reports them differently.
#[derive(Debug, PartialEq, Eq)]
enum Failure {
    /// [`OutcomeClass::Rejected`] or [`OutcomeClass::NotFound`]: szamlazz.hu
    /// refused before acting; on a write, 7 is a missing field.
    Rejected {
        code: String,
        message: String,
    },
    /// [`OutcomeClass::DuplicateOrderNumber`] (71/152).
    Duplicate {
        code: String,
        message: String,
    },
    /// See [`is_credentials_rejected`].
    CredentialsRejected {
        code: String,
        message: String,
    },
    /// [`OutcomeClass::Unknown`]: 1, 55, 56 without a number, a code the
    /// agent crate does not know, or any class added to the crate later: the
    /// outcome is open, re-query.
    Unknown {
        code: String,
        message: String,
    },
    /// `szlahu_down`: whether szamlazz.hu acted before answering is not
    /// known, re-query.
    Unavailable(String),
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
            let message = api.message;
            match api.code.outcome_class() {
                OutcomeClass::DuplicateOrderNumber => Failure::Duplicate { code, message },
                OutcomeClass::Rejected | OutcomeClass::NotFound => {
                    Failure::Rejected { code, message }
                }
                // `Unknown`, and any class the agent crate adds later: a
                // document may exist, so the step re-queries rather than
                // claims `rejected`.
                _ => Failure::Unknown { code, message },
            }
        }
        ClientError::ServiceUnavailable(message) => Failure::Unavailable(message),
        ClientError::Request(error) => Failure::Rejected {
            code: REQUEST_CODE.to_owned(),
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

/// A raw query result as the read's outcome: every answer is data, no answer
/// is [`Unanswered`].
fn outcome(result: Result<InvoiceDocument, QueryError>) -> Result<QueryOutcome, Unanswered> {
    match result {
        Ok(document) => Ok(QueryOutcome::Found(Box::new(document))),
        Err(error) => Ok(match error.answered()? {
            Answer::NotFound => QueryOutcome::NotFound,
            Answer::CredentialsRejected { code, message } => {
                QueryOutcome::CredentialsRejected { code, message }
            }
            Answer::Api { code, message } => QueryOutcome::Api { code, message },
        }),
    }
}

/// The create step's rule on what its external-id query saw (`seen`),
/// against the number of the document the lookup step saw reversed
/// (`reversed`): the step sends only when the id holds **nothing**, or
/// **exactly** that document, still reversed (`Ok(None)`). `Some` settles the
/// step without a send: a live document of ours that is not `reversed`
/// ([`CreateOutcome::Found`], an earlier execution created it), a reversed
/// document of ours that is not `reversed` ([`CreateOutcome::Reversed`],
/// issued and reversed since the lookup), `reversed` reported live
/// ([`CreateOutcome::LiveAgain`], the server contradicting itself), an
/// invalid holder ([`CreateOutcome::Collision`]) and rejected credentials
/// ([`CreateOutcome::CredentialsRejected`]).
///
/// # Errors
///
/// Every other failure of the query, unchanged, for the caller to place: on
/// the leading query an answer (another code, `szlahu_down`) is settled data
/// and only a transport failure is [`Unconfirmed`]; after a send every
/// failure leaves the step unconfirmed.
fn settle_create(
    seen: Result<Seen, QueryError>,
    reversed: Option<&str>,
) -> Result<Option<CreateOutcome>, QueryError> {
    match seen {
        Ok(Seen::Collision(found)) => Ok(Some(CreateOutcome::Collision(found))),
        Ok(Seen::Live(found)) if Some(found.number()) != reversed => {
            Ok(Some(CreateOutcome::Found(found)))
        }
        // The document the lookup saw reversed, reported live: a server
        // inconsistency. Never send past it.
        Ok(Seen::Live(found)) => {
            tracing::warn!(
                number = %found.number(),
                "the document the lookup saw reversed is reported live"
            );
            Ok(Some(CreateOutcome::LiveAgain(found)))
        }
        // A reversed document the lookup did not see: issued and reversed
        // since. Never send past a reversal the caller has not acknowledged.
        Ok(Seen::Reversed(found)) if Some(found.number()) != reversed => {
            tracing::warn!(
                number = %found.number(),
                "a document reversed since the lookup holds the external id"
            );
            Ok(Some(CreateOutcome::Reversed(found)))
        }
        // Nothing (code 7), or the document the lookup saw reversed, still
        // reversed.
        Ok(Seen::Reversed(_) | Seen::Absent) => Ok(None),
        Err(QueryError::CredentialsRejected { code, message }) => {
            Ok(Some(CreateOutcome::CredentialsRejected { code, message }))
        }
        Err(error) => Err(error),
    }
}

/// The storno step's rule on what its storno-external-id query saw
/// (`seen`, the number of the `SS` reversing the invoice when one holds the
/// id): the `SS` settles the step ([`StornoOutcome::AlreadyReversed`], an
/// earlier execution sent it), nothing lets the send proceed (`Ok(None)`),
/// rejected credentials settle it ([`StornoOutcome::CredentialsRejected`]).
///
/// # Errors
///
/// Every other failure of the query, unchanged, for the caller to place, as
/// [`settle_create`] hands them back.
fn settle_storno(
    seen: Result<Option<String>, QueryError>,
) -> Result<Option<StornoOutcome>, QueryError> {
    match seen {
        Ok(Some(storno_number)) => Ok(Some(StornoOutcome::AlreadyReversed { storno_number })),
        Ok(None) => Ok(None),
        Err(QueryError::CredentialsRejected { code, message }) => {
            Ok(Some(StornoOutcome::CredentialsRejected { code, message }))
        }
        Err(error) => Err(error),
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
    use szamlazz_agent::wire::{AgentRequest as _, RawResponse};
    use szamlazz_agent::{ParseError, RequestError};

    use super::*;
    use crate::test_support::{CreditRecord, Doc};

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

    #[test]
    fn document_ext_reads_the_checks_off_a_queried_document() {
        let order = OrderKey::parse("ORD-1").expect("order");
        let live = Doc {
            payments: &[
                CreditRecord::new(date(2026, 7, 4), "transfer", "500"),
                CreditRecord::new(date(2026, 7, 5), "transfer", "770"),
            ],
            ..Doc::new("SZ-1", "SZ")
        }
        .parse();
        assert_eq!(live.number(), "SZ-1");
        assert!(live.is_live());
        assert_eq!(live.e_invoice(), Some(true));
        assert_eq!(live.payment_amounts(), [dec!(500), dec!(770)]);
        assert!(live.carries_order(&order));
        assert!(
            !live.carries_order(&OrderKey::parse("ORD-2").expect("order")),
            "another order's number"
        );
        assert!(
            !live.carries_order(&OrderKey::parse("ord-1").expect("order")),
            "case is significant, as on the server"
        );
        assert!(live.is_ours(&order, IssuedKind::Invoice));
        assert!(!live.is_ours(&order, IssuedKind::Proforma));
        assert!(
            !live.is_ours(
                &OrderKey::parse("ORD-2").expect("order"),
                IssuedKind::Invoice
            ),
            "another order's"
        );
        assert!(!live.is_storno_of("SZ-0"));

        let reversed = Doc {
            reversed: true,
            ..Doc::new("SZ-1", "SZ")
        }
        .parse();
        assert!(!reversed.is_live());
        assert!(reversed.is_ours(&order, IssuedKind::Invoice));

        let storno = Doc {
            referenced_invoice: Some("SZ-1"),
            ..Doc::new("SS-1", "SS")
        }
        .parse();
        assert!(storno.is_live(), "the storno invoice carries no marker");
        assert!(storno.is_storno_of("SZ-1"));
        assert!(!storno.is_storno_of("SZ-2"));

        let proforma = Doc::new("D-1", "D").parse();
        assert_eq!(proforma.e_invoice(), None, "eszamla 0 is not an invoice");
        assert!(proforma.is_ours(&order, IssuedKind::Proforma));

        // No account pin: neither `teszt` nor the seller record's id is read:
        // not a live marker, and not a missing one either (the agent crate
        // reports an absent `<teszt>` as `None` since #70; the worker has
        // nothing to compare it with).
        let other_account = Doc {
            test: Some(false),
            ..Doc::new("SZ-1", "SZ")
        }
        .parse();
        assert!(other_account.is_ours(&order, IssuedKind::Invoice));
        let unknown_mode = Doc {
            test: None,
            ..Doc::new("SZ-1", "SZ")
        }
        .parse();
        assert_eq!(unknown_mode.info.test, None);
        assert!(unknown_mode.is_ours(&order, IssuedKind::Invoice));
    }

    /// The order number a document carries is `rendelesszam` trimmed, as
    /// szamlazz.hu matches it, and nothing when the element is absent,
    /// empty or whitespace only: a document issued outside any order.
    /// `carries_order` is that reading compared with the key, so a padded
    /// `rendelesszam` carries the order and an empty one carries none. The
    /// agent crate's parser trims the element and reads an empty one as
    /// `None` already, so the rendered cases prove the pair end to end and
    /// the assigned ones prove the worker's own reading, which does not lean
    /// on the parser's.
    #[test]
    fn the_order_number_is_the_trimmed_rendelesszam_or_none() {
        let order = OrderKey::parse("ORD-1").expect("order");

        let plain = Doc::default().parse();
        assert_eq!(plain.order_number(), Some("ORD-1"));
        assert!(plain.carries_order(&order));

        let padded = Doc {
            order: Some("  ORD-1 "),
            ..Doc::default()
        }
        .parse();
        assert_eq!(padded.order_number(), Some("ORD-1"), "trimmed");
        assert!(padded.carries_order(&order));

        for outside_any_order in [None, Some(""), Some("   "), Some("\t\n")] {
            let document = Doc {
                order: outside_any_order,
                ..Doc::default()
            }
            .parse();
            assert_eq!(
                document.order_number(),
                None,
                "rendelesszam {outside_any_order:?}"
            );
            assert!(
                !document.carries_order(&order),
                "rendelesszam {outside_any_order:?}"
            );
            assert!(
                !document.is_ours(&order, IssuedKind::Invoice),
                "rendelesszam {outside_any_order:?}"
            );
        }

        // The worker's own reading of the parsed value, with the parser's
        // normalisation out of the way.
        let mut assigned = Doc::default().parse();
        for (raw, read) in [
            ("ORD-1", Some("ORD-1")),
            ("  ORD-1 ", Some("ORD-1")),
            ("", None),
            ("   ", None),
            ("\t\n", None),
        ] {
            assigned.info.order_number = Some(raw.to_owned());
            assert_eq!(assigned.order_number(), read, "order_number {raw:?}");
            assert_eq!(
                assigned.carries_order(&order),
                read.is_some(),
                "order_number {raw:?}"
            );
        }
        assigned.info.order_number = None;
        assert_eq!(assigned.order_number(), None);
        assert!(!assigned.carries_order(&order));
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

    // The pure classifiers behind the async steps, each on its own table.
    // The wiremock suite (`tests/gateway.rs`) reaches them through HTTP and
    // keeps one exchange per operation for the header-vs-body parse path;
    // the branches are pinned here.

    /// Step 2 of the lookup: the order-number hint is foreign when it is a
    /// **live** document of the invoice family (`SZ`, `ES`, `VS`) that is
    /// neither the document seen under our external id nor a number the
    /// exclusivity and proforma checks already saw as ours. A reversed one
    /// is not (the order is free again), and neither is a proforma, a
    /// corrective, a storno or a delivery note under the order, whatever it
    /// references: the order number is what the hint queried by, so it says
    /// nothing here.
    #[test]
    fn a_foreign_document_is_a_live_invoice_kind_that_is_neither_seen_nor_known() {
        let live = |number: &'static str, tipus: &'static str| Doc::new(number, tipus).parse();
        let none: &[String] = &[];

        for tipus in ["SZ", "ES", "VS"] {
            let found = live("X-9", tipus);
            assert!(is_foreign(&found, none, None), "{tipus}: nothing known");
            assert!(
                !is_foreign(&found, none, Some("X-9")),
                "{tipus}: the document seen under our external id"
            );
            assert!(
                is_foreign(&found, none, Some("X-1")),
                "{tipus}: another document seen under our external id"
            );
            assert!(
                !is_foreign(&found, &["X-9".to_owned()], None),
                "{tipus}: a number known to be ours"
            );
            assert!(
                is_foreign(&found, &["X-1".to_owned(), "X-2".to_owned()], None),
                "{tipus}: other numbers known to be ours"
            );
            assert!(
                !is_foreign(
                    &Doc {
                        reversed: true,
                        ..Doc::new("X-9", tipus)
                    }
                    .parse(),
                    none,
                    None
                ),
                "{tipus}: reversed is not foreign"
            );
        }

        for (tipus, referenced) in [
            ("D", None),
            ("HS", Some("X-1")),
            ("SS", Some("X-1")),
            ("SL", None),
        ] {
            let found = Doc {
                referenced_invoice: referenced,
                ..Doc::new("X-9", tipus)
            }
            .parse();
            assert!(
                !is_foreign(&found, none, None),
                "{tipus} is not of the invoice family"
            );
        }
    }

    /// A szamlazz.hu code on a create-like send, classified for the step by
    /// its outcome class: the credential codes first, before their class
    /// (`Rejected`, as asserted); 71 and 152 as the duplicate; every code
    /// szamlazz.hu refuses before acting as `Rejected`, and 7 among them (on
    /// a write it is a missing field, not a missing document; the `NotFound`
    /// class); the open codes 1, 55, 56 and a code the agent crate does not
    /// know as `Unknown`, through the wildcard arm that a class the crate
    /// adds later falls into too.
    #[test]
    fn a_failed_send_is_classified_by_its_code_class_with_the_credential_codes_first() {
        let api = |code: ErrorCode| {
            ClientError::Api(ApiError {
                code,
                message: "üzenet".to_owned(),
            })
        };
        let classified = |code: ErrorCode| classify_failure(api(code));

        for code in [
            ErrorCode::InvalidCredentials,
            ErrorCode::BrowserSessionActive,
            ErrorCode::LoginBlocked,
            ErrorCode::MultipleAccounts,
        ] {
            assert_eq!(
                code.outcome_class(),
                OutcomeClass::Rejected,
                "{code:?}: the class the credential check pre-empts"
            );
            assert_eq!(
                classified(code.clone()),
                Failure::CredentialsRejected {
                    code: code.code().to_owned(),
                    message: "üzenet".to_owned(),
                },
                "{code:?}"
            );
        }

        for code in [
            ErrorCode::DuplicateOrderNumber,
            ErrorCode::DuplicateOrderNumberNamed,
        ] {
            assert_eq!(
                code.outcome_class(),
                OutcomeClass::DuplicateOrderNumber,
                "{code:?}"
            );
            assert_eq!(
                classified(code.clone()),
                Failure::Duplicate {
                    code: code.code().to_owned(),
                    message: "üzenet".to_owned(),
                },
                "{code:?}"
            );
        }

        assert_eq!(
            ErrorCode::MissingData.outcome_class(),
            OutcomeClass::NotFound
        );
        for code in [
            ErrorCode::MissingData,
            ErrorCode::StornoOfReversalInvoice,
            ErrorCode::MalformedXml,
            ErrorCode::PrepaymentInvoiceNotIdentifiable,
            ErrorCode::HasCorrectiveInvoice,
            ErrorCode::NetValueMismatch,
            ErrorCode::ProformaNotFound,
            ErrorCode::IssueDateMustBeToday,
        ] {
            assert!(
                matches!(
                    code.outcome_class(),
                    OutcomeClass::Rejected | OutcomeClass::NotFound
                ),
                "{code:?}"
            );
            assert_eq!(
                classified(code.clone()),
                Failure::Rejected {
                    code: code.code().to_owned(),
                    message: "üzenet".to_owned(),
                },
                "{code:?}"
            );
        }

        for code in [
            ErrorCode::Maintenance,
            ErrorCode::EInvoiceSigningFailed,
            ErrorCode::InvoiceNotificationDeliveryFailed,
            ErrorCode::Unknown("999".to_owned()),
        ] {
            assert_eq!(code.outcome_class(), OutcomeClass::Unknown, "{code:?}");
            assert_eq!(
                classified(code.clone()),
                Failure::Unknown {
                    code: code.code().to_owned(),
                    message: "üzenet".to_owned(),
                },
                "{code:?}"
            );
        }
    }

    /// The failures of a create-like send that carry no szamlazz.hu code:
    /// `szlahu_down` as `Unavailable`; a request the wire contract refused
    /// as `Rejected` under the `request` pseudo-code with the contract's
    /// message; and the two remaining `ClientError`s, a parse failure and a
    /// `reqwest` error (from a request that cannot be built, the one such
    /// error a test can make without a wire), as `Transport` with their
    /// display.
    #[test]
    fn a_failed_send_without_a_code_is_unavailable_the_request_or_transport() {
        assert_eq!(
            classify_failure(ClientError::ServiceUnavailable("karbantartás".to_owned())),
            Failure::Unavailable("karbantartás".to_owned())
        );

        let request = ClientError::Request(RequestError::MissingLineItems);
        let message = request.to_string();
        assert_eq!(
            classify_failure(request),
            Failure::Rejected {
                code: REQUEST_CODE.to_owned(),
                message,
            }
        );

        let parse = ClientError::Parse(ParseError::Missing("szamlaszam"));
        let message = parse.to_string();
        assert_eq!(classify_failure(parse), Failure::Transport(message));

        let unbuildable = crate::test_support::http_client()
            .get("http://")
            .build()
            .expect_err("an empty host does not build");
        let reqwest_error = ClientError::Transport(unbuildable);
        let message = reqwest_error.to_string();
        assert_eq!(classify_failure(reqwest_error), Failure::Transport(message));
    }

    /// The one place a query's failure is split into what szamlazz.hu
    /// answered and what it did not: 7, a credential code and another code
    /// are answers for the read fn to turn into its outcome; `szlahu_down`
    /// and a transport failure are the `Unanswered` the read policy
    /// re-executes, each carrying its message. And the fold every read fn
    /// (`verify`, `query`, `hint`) applies: a document is `Found`, the
    /// answers are the three outcome variants, the rest is `Err`.
    #[test]
    fn a_query_error_is_an_answer_or_unanswered_and_the_outcome_folds_it() {
        let rejected = || QueryError::CredentialsRejected {
            code: "3".to_owned(),
            message: "Sikertelen bejelentkezés.".to_owned(),
        };
        let other = || QueryError::Api {
            code: "57".to_owned(),
            message: "Hibás XML.".to_owned(),
        };

        assert_eq!(QueryError::NotFound.answered(), Ok(Answer::NotFound));
        assert_eq!(
            rejected().answered(),
            Ok(Answer::CredentialsRejected {
                code: "3".to_owned(),
                message: "Sikertelen bejelentkezés.".to_owned(),
            })
        );
        assert_eq!(
            other().answered(),
            Ok(Answer::Api {
                code: "57".to_owned(),
                message: "Hibás XML.".to_owned(),
            })
        );
        assert_eq!(
            QueryError::Unavailable("szlahu_down".to_owned()).answered(),
            Err(Unanswered::Unavailable("szlahu_down".to_owned()))
        );
        assert_eq!(
            QueryError::Transport("connection reset".to_owned()).answered(),
            Err(Unanswered::Transport("connection reset".to_owned()))
        );

        let document = Doc::default().parse();
        assert_eq!(
            outcome(Ok(document.clone())),
            Ok(QueryOutcome::Found(Box::new(document)))
        );
        assert_eq!(
            outcome(Err(QueryError::NotFound)),
            Ok(QueryOutcome::NotFound)
        );
        assert_eq!(
            outcome(Err(rejected())),
            Ok(QueryOutcome::CredentialsRejected {
                code: "3".to_owned(),
                message: "Sikertelen bejelentkezés.".to_owned(),
            })
        );
        assert_eq!(
            outcome(Err(other())),
            Ok(QueryOutcome::Api {
                code: "57".to_owned(),
                message: "Hibás XML.".to_owned(),
            })
        );
        assert_eq!(
            outcome(Err(QueryError::Unavailable("szlahu_down".to_owned()))),
            Err(Unanswered::Unavailable("szlahu_down".to_owned()))
        );
        assert_eq!(
            outcome(Err(QueryError::Transport("connection reset".to_owned()))),
            Err(Unanswered::Transport("connection reset".to_owned()))
        );
    }

    /// The query failures [`settle_create`] and [`settle_storno`] hand back
    /// unchanged for the step to place. `NotFound` is among them for the
    /// match to be exhaustive only: `seen` and `storno_seen` fold code 7 into
    /// their "nothing" answer, so the step never passes it; right if reached.
    fn unplaced_query_errors() -> [QueryError; 4] {
        [
            QueryError::NotFound,
            QueryError::Api {
                code: "57".to_owned(),
                message: "xml".to_owned(),
            },
            QueryError::Unavailable("szlahu_down".to_owned()),
            QueryError::Transport("reset".to_owned()),
        ]
    }

    /// The create step's rule, as a function of what its external-id query
    /// saw and the number the lookup step saw reversed: the step sends only
    /// when the id holds **nothing**, or **exactly** the lookup's reversed
    /// document, still reversed (`Ok(None)`). A live document that is not
    /// that one was issued by an earlier execution (`Found`); the lookup's
    /// reversed document reported live is the server contradicting itself
    /// (`LiveAgain`); a reversed document that is not the lookup's was
    /// issued and reversed since (`Reversed`); an invalid holder is a
    /// `Collision`; rejected credentials settle the step. Every other failure
    /// of the query is handed back for the caller to place: settled data on
    /// the leading query, unconfirmed after a send.
    #[test]
    fn the_create_step_sends_only_past_nothing_or_the_lookups_reversed_document() {
        let sz_1 = || Doc::default().boxed();
        let sz_2 = || Doc::new("SZ-2", "SZ").boxed();
        let reversed = |number: &'static str| {
            Doc {
                reversed: true,
                ..Doc::new(number, "SZ")
            }
            .boxed()
        };
        let other = || {
            Doc {
                order: Some("ORD-2"),
                ..Doc::new("SZ-OTHER", "SZ")
            }
            .boxed()
        };

        for lookup_saw in [None, Some("SZ-1")] {
            assert_eq!(
                settle_create(Ok(Seen::Absent), lookup_saw),
                Ok(None),
                "nothing under the id, lookup saw {lookup_saw:?}"
            );
            assert_eq!(
                settle_create(Ok(Seen::Collision(other())), lookup_saw),
                Ok(Some(CreateOutcome::Collision(other()))),
                "a collision, lookup saw {lookup_saw:?}"
            );
            assert_eq!(
                settle_create(Ok(Seen::Live(sz_2())), lookup_saw),
                Ok(Some(CreateOutcome::Found(sz_2()))),
                "a live document that is not the lookup's, lookup saw {lookup_saw:?}"
            );
            assert_eq!(
                settle_create(Ok(Seen::Reversed(reversed("SZ-2"))), lookup_saw),
                Ok(Some(CreateOutcome::Reversed(reversed("SZ-2")))),
                "a reversed document that is not the lookup's, lookup saw {lookup_saw:?}"
            );
        }

        // The lookup saw nothing: any holder settles.
        assert_eq!(
            settle_create(Ok(Seen::Live(sz_1())), None),
            Ok(Some(CreateOutcome::Found(sz_1())))
        );
        assert_eq!(
            settle_create(Ok(Seen::Reversed(reversed("SZ-1"))), None),
            Ok(Some(CreateOutcome::Reversed(reversed("SZ-1"))))
        );

        // The lookup saw SZ-1 reversed: still reversed proceeds, live again
        // is the contradiction.
        assert_eq!(
            settle_create(Ok(Seen::Reversed(reversed("SZ-1"))), Some("SZ-1")),
            Ok(None),
            "the reissue's one send"
        );
        assert_eq!(
            settle_create(Ok(Seen::Live(sz_1())), Some("SZ-1")),
            Ok(Some(CreateOutcome::LiveAgain(sz_1())))
        );

        // The query's failures.
        for lookup_saw in [None, Some("SZ-1")] {
            assert_eq!(
                settle_create(
                    Err(QueryError::CredentialsRejected {
                        code: "3".to_owned(),
                        message: "login".to_owned(),
                    }),
                    lookup_saw,
                ),
                Ok(Some(CreateOutcome::CredentialsRejected {
                    code: "3".to_owned(),
                    message: "login".to_owned(),
                })),
                "lookup saw {lookup_saw:?}"
            );
            for error in unplaced_query_errors() {
                assert_eq!(
                    settle_create(Err(error.clone()), lookup_saw),
                    Err(error.clone()),
                    "{error:?} is the caller's to place, lookup saw {lookup_saw:?}"
                );
            }
        }
    }

    /// The storno step's rule, as a function of what its storno-external-id
    /// query saw: the `SS` reversing the invoice settles the step
    /// (`AlreadyReversed`), nothing (or a stray holder, which `storno_seen`
    /// reads as nothing) lets the send proceed, rejected credentials settle
    /// it, and every other failure is the caller's to place.
    #[test]
    fn the_storno_step_sends_only_when_no_storno_of_ours_is_under_the_id() {
        assert_eq!(
            settle_storno(Ok(Some("SS-1".to_owned()))),
            Ok(Some(StornoOutcome::AlreadyReversed {
                storno_number: "SS-1".to_owned(),
            }))
        );
        assert_eq!(settle_storno(Ok(None)), Ok(None));
        assert_eq!(
            settle_storno(Err(QueryError::CredentialsRejected {
                code: "135".to_owned(),
                message: "session".to_owned(),
            })),
            Ok(Some(StornoOutcome::CredentialsRejected {
                code: "135".to_owned(),
                message: "session".to_owned(),
            }))
        );
        for error in unplaced_query_errors() {
            assert_eq!(
                settle_storno(Err(error.clone())),
                Err(error.clone()),
                "{error:?} is the caller's to place"
            );
        }
    }
}
