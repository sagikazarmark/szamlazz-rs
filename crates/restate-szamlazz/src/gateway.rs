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
//! (its document defaults, its seller block) is read through
//! [`Gateway::account`]; nothing of a found document is compared with the
//! account (the worker holds no account pin; ADR 0006, account-pin
//! amendment).
//!
//! Every query result is validated before it is called ours, against the
//! document's own identity (the order number and the `tipus` of the kind,
//! [`FoundDocument::is_ours`]): external ids are not unique server-side and
//! the order-number hint returns the most recently issued document of any
//! kind.
//!
//! Tracing events carry external ids, kinds, numbers and codes, never buyer
//! data.
//!
//! # Journaled types are crate-owned
//!
//! The outcome types derive `serde` so that the Restate services can journal
//! them as the result of a `ctx.run`. Restate replays an entry only on the
//! deployment that wrote it (deployments are immutable, ADR 0009), so the
//! types carry no cross-version compatibility contract; what matters is what
//! an entry holds, since the Restate UI shows every entry for the retention
//! period. So the outcomes here ([`LookupOutcome`], [`CreateOutcome`],
//! [`QueryOutcome`], [`OwnershipOutcome`], [`StornoLookupOutcome`],
//! [`StornoOutcome`], [`DeleteOutcome`], [`SetCreditEntriesOutcome`],
//! [`ProbeOutcome`], [`TaxpayerOutcome`]) carry **crate-owned types, never a `szamlazz_agent`
//! response type**: the document outcomes carry the worker's projections
//! [`FoundDocument`] (of a queried `InvoiceDocument`) and [`IssuedDocument`]
//! (of a create or storno reply), [`TaxpayerOutcome`] the crate-owned
//! [`QueryTaxpayerResponse`]. A projection holds what the handlers read and
//! nothing else: what the worker never reads of a document (the buyer block,
//! the seller block, the line items, the PDF) is not in the journal, and the
//! agent key never is. `service::journal` checks both on a sample of every
//! variant, and that each round-trips through serde.
//!
//! What szamlazz.hu answers with when it answers a code is one type wherever
//! it appears: [`SzamlazzAnswer`] (`code`, `message`) in every
//! `CredentialsRejected` and `Api` variant and flattened into
//! [`CreateOutcome::DuplicateOrderNumber`]; a write's `Rejected` carries a
//! [`Rejection`], whose [`RejectionCode`] tells szamlazz.hu's refusal from the
//! wire contract's (the `request` pseudo-code, never sent). Both serialise to
//! the two string fields the variants carried before them, so the journal
//! layout is unchanged (#128).
//!
//! [`FoundDocument`]'s methods are the checks the services make on a queried
//! document before trusting or acting on it.

use std::fmt;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use szamlazz_agent::client::BuildError;
use szamlazz_agent::ops::credit_entry::{
    CreditEntries, CreditEntry, InvoiceBalance, RegisterCreditEntry,
};
use szamlazz_agent::ops::invoice::{CreateInvoice, CreatedInvoice, CreationOutcome};
use szamlazz_agent::ops::proforma::{DeleteProforma, ProformaSelector};
use szamlazz_agent::ops::query_xml::QueryInvoiceXml;
use szamlazz_agent::ops::storno::StornoInvoice;
use szamlazz_agent::ops::taxpayer::{QueryTaxpayer, TaxpayerPrefix};
use szamlazz_agent::{
    ApiError, Client, ClientError, Credentials, Date, ErrorCode, InvoiceNumber, InvoiceSelector,
    OutcomeClass, reqwest,
};
use tracing::Instrument as _;

use crate::account::Account;
use crate::contract::{
    CreditEntryInput, DeleteReason, IssuedKind, QueryTaxpayerResponse, Selector,
};
use crate::identity::{ExternalId, OrderKey};

pub mod build;
pub mod document;

pub use build::{DocumentRefs, InputError};
pub use document::{FoundDocument, IssuedDocument, RecordedCreditEntry};

/// What szamlazz.hu answered with when the answer is a code rather than a
/// document: the code (numeric for szamlazz.hu's own, `OPERATION_FAILED`-like
/// for a NAV code the taxpayer query relays) and the message beside it, as
/// the outcome variants carry them (`CredentialsRejected`, `Api`, the
/// duplicate-order-number answer). Journaled inside those outcomes;
/// serialises as the two fields, which is what the variants
/// carried before it existed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SzamlazzAnswer {
    /// The code as szamlazz.hu wrote it.
    pub code: String,
    /// The message beside it.
    pub message: String,
}

impl SzamlazzAnswer {
    /// An answer of `code` and `message`.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// A szamlazz.hu API error as the answer it is. The code is the agent
/// crate's display of it: the wire token, or `absent` for a failure
/// szamlazz.hu reported without a code, so a fault's `szamlazz_code` and a
/// journaled answer never carry an empty string.
impl From<ApiError> for SzamlazzAnswer {
    fn from(api: ApiError) -> Self {
        Self::new(api.code.to_string(), api.message)
    }
}

impl fmt::Display for SzamlazzAnswer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

/// A refusal of a write: szamlazz.hu's, or the wire contract's before
/// anything was sent ([`RejectionCode::Request`]). What the `Rejected`
/// variants of [`CreateOutcome`], [`StornoOutcome`], [`DeleteOutcome`] and
/// [`SetCreditEntriesOutcome`] carry; serialises as `code` and `message`, the code
/// as its wire string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Rejection {
    /// Who refused, and with what.
    pub code: RejectionCode,
    /// The message: szamlazz.hu's, or the wire contract's rule.
    pub message: String,
}

impl Rejection {
    /// The wire contract's refusal, before anything was sent.
    pub fn request(message: impl Into<String>) -> Self {
        Self {
            code: RejectionCode::Request,
            message: message.into(),
        }
    }
}

/// szamlazz.hu's refusal.
impl From<SzamlazzAnswer> for Rejection {
    fn from(answer: SzamlazzAnswer) -> Self {
        Self {
            code: RejectionCode::Szamlazz(answer.code),
            message: answer.message,
        }
    }
}

impl From<ApiError> for Rejection {
    fn from(api: ApiError) -> Self {
        SzamlazzAnswer::from(api).into()
    }
}

/// The code of a [`Rejection`]: szamlazz.hu's, or the pseudo-code of a
/// rejection that never reached szamlazz.hu because the request violates the
/// Számla Agent wire contract (a sixth credit entry; a replacing credit-entry
/// request with no entries, which would clear the invoice's credit entries; a
/// document without line items). On a create or storno the pseudo-code is
/// the `rejected` outcome like any other code; `Szamlazz.Agent.set_credit_entries`
/// tells it apart and answers the caller's request as `invalid_input`, since
/// szamlazz.hu answered nothing to pass through.
///
/// Serialises as the wire string: the szamlazz.hu code as written, or
/// [`RejectionCode::REQUEST`], so the journaled shape is the one string
/// field it always was. `#[non_exhaustive]` like the outcomes that carry it:
/// a pseudo-code added later must not break a caller's match.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RejectionCode {
    /// A szamlazz.hu code.
    Szamlazz(String),
    /// The wire contract's refusal; nothing was sent.
    Request,
}

impl RejectionCode {
    /// The wire string of [`RejectionCode::Request`]. Never a szamlazz.hu
    /// code: those are numeric, or upper-case NAV tokens.
    pub const REQUEST: &'static str = "request";

    /// The code as its wire string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Szamlazz(code) => code,
            Self::Request => Self::REQUEST,
        }
    }
}

impl fmt::Display for RejectionCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<RejectionCode> for String {
    fn from(code: RejectionCode) -> Self {
        match code {
            RejectionCode::Szamlazz(code) => code,
            RejectionCode::Request => RejectionCode::REQUEST.to_owned(),
        }
    }
}

/// A refused deletion's code as the delete response's reason: szamlazz.hu's
/// code as itself. (The wire contract refuses nothing on a delete, so
/// [`RejectionCode::Request`] does not arise there; were it to, it would read
/// as the `request` pseudo-code, as every other response carries it.)
impl From<RejectionCode> for DeleteReason {
    fn from(code: RejectionCode) -> Self {
        Self::Szamlazz(String::from(code))
    }
}

/// Serializes as the wire string.
impl Serialize for RejectionCode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// Deserializes from the wire string: [`RejectionCode::REQUEST`] is the
/// pseudo-code, anything else szamlazz.hu's.
impl<'de> Deserialize<'de> for RejectionCode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = String::deserialize(deserializer)?;
        Ok(if code == Self::REQUEST {
            Self::Request
        } else {
            Self::Szamlazz(code)
        })
    }
}

/// The module that speaks to szamlazz.hu for one account: the Számla Agent
/// client plus the [`Account`] it is opened for.
///
/// Opened with [`Gateway::open`] for one handler execution from a resolved
/// account and freshly fetched credentials, or with
/// [`Gateway::open_with_http`] over a caller-built HTTP client. Not `Clone`:
/// a clone would share the client and its cookie jar, the very thing the
/// fresh-client-per-open boundary exists to prevent; the services hold one
/// in an `Arc` for the execution.
#[derive(Debug)]
pub struct Gateway {
    client: Client,
    account: Account,
}

/// The lookup step: what identifies the document whose
/// external id is queried and, for every kind but correctives, the order
/// whose hint is taken.
///
/// A found document is validated against `order` and `kind`
/// ([`FoundDocument::is_ours`]); the request carries only what identifies
/// the document.
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
/// Documents are boxed: a [`FoundDocument`] is large next to the unit
/// variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum LookupOutcome {
    /// Nothing under the external id (code 7), and the hint saw nothing
    /// foreign.
    Absent,
    /// A live document of ours under the external id. The hint is not taken:
    /// nothing will be created.
    Live(Box<FoundDocument>),
    /// A reversed document of ours under the external id, and the hint saw
    /// nothing foreign.
    Reversed {
        /// The reversed document.
        document: Box<FoundDocument>,
        /// Its storno's number, when the newest document under the order is
        /// the `SS` referencing it; absent otherwise and for correctives.
        storno_number: Option<String>,
    },
    /// The external id resolves to a document that fails validation (another
    /// order or kind).
    Collision(Box<FoundDocument>),
    /// A live invoice-kind document under the order number that is neither
    /// in `our_numbers` nor the document seen under the external id: another
    /// channel's. Reported even when our own document under the id is
    /// reversed: no create (reissue or not) may proceed past it.
    Foreign(Box<FoundDocument>),
    /// szamlazz.hu rejected the agent credentials (3, 135, 136, 164) on the
    /// external-id query or the hint; nothing may be concluded and nothing
    /// will be created. See [`ErrorCode::is_credential_error`].
    CredentialsRejected(SzamlazzAnswer),
    /// szamlazz.hu answered the external-id query with another code: an
    /// answer the step cannot conclude from, and nothing will be created. (On
    /// the hint the same answer says nothing about foreign documents and the
    /// lookup continues.)
    Api(SzamlazzAnswer),
}

/// The create step: query the external id, then send the
/// create unless a live document of ours is already there.
///
/// Carries what identifies the document and the create to send. A found
/// document is validated against `order` and `kind`
/// ([`FoundDocument::is_ours`]).
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
/// Documents are boxed: a [`FoundDocument`] is large next to the
/// code-and-message variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CreateOutcome {
    /// szamlazz.hu issued the document (or replayed a byte-identical earlier
    /// create, indistinguishable and reported either way), numbered.
    Issued(IssuedDocument),
    /// A live document of ours is under the external id, found by the
    /// leading query (an earlier execution of this step created it) or by the
    /// re-query after a lost reply. Nothing was sent, or what was sent landed.
    Found(Box<FoundDocument>),
    /// A **reversed** document of ours that the lookup step did not see is
    /// under the external id: an earlier execution of this step (or anyone)
    /// issued it and it was reversed since. Nothing was sent: a reversal
    /// the lookup did not see must be answered as `reversed`, never issued
    /// past (a new document needs an explicit `reissue`).
    Reversed(Box<FoundDocument>),
    /// The document the lookup step saw **reversed** is reported **live**
    /// by the leading query or the re-query: the server contradicts itself.
    /// Nothing was sent: sending is the least safe answer to an
    /// inconsistency; the caller sees `conflict{live}` as the lookup would
    /// have reported.
    LiveAgain(Box<FoundDocument>),
    /// szamlazz.hu refused the order number as a duplicate (71/152) and the
    /// external-id re-query found a live document of ours: an earlier send
    /// had landed.
    Reconciled(Box<FoundDocument>),
    /// The external id resolves to a document that fails validation (another
    /// order or kind). Nothing was created.
    Collision(Box<FoundDocument>),
    /// szamlazz.hu refused the order number as a duplicate (71/152) and the
    /// external-id re-query found no live document of ours: the duplicate is
    /// not ours. Never reported for correctives, which are exempt from the
    /// order-number check: their unresolved 71/152 is
    /// [`CreateOutcome::Rejected`].
    DuplicateOrderNumber {
        /// The szamlazz.hu answer (`71` or `152` and its message), flattened:
        /// `code` and `message` beside `existing_number`.
        #[serde(flatten)]
        answer: SzamlazzAnswer,
        /// The newest document under the order, when it is a live document of
        /// the kind being issued; absent when a document of another kind (or a
        /// reversed one) is newest, when the order-number query knows nothing
        /// under the order (a contradiction, logged at `warn` and settled all
        /// the same), and when the naming query itself failed.
        existing_number: Option<String>,
    },
    /// szamlazz.hu refused the document; nothing was created.
    Rejected(Rejection),
    /// szamlazz.hu rejected the agent credentials (3, 135, 136, 164) on the
    /// leading query, the create or a re-query; this execution issued
    /// nothing. Settled data, not [`Unconfirmed`]: re-executing with the same
    /// key would only repeat the answer. See [`ErrorCode::is_credential_error`].
    CredentialsRejected(SzamlazzAnswer),
    /// szamlazz.hu answered the **leading** query with another code (neither
    /// 7 nor a credential code): an answer the step cannot conclude from, so
    /// nothing was sent. Settled data, as [`LookupOutcome::Api`] is for the
    /// same code one step earlier, not [`Unconfirmed`], which would spend the
    /// issue policy, sized for the post-send window, on a read and report an
    /// answer as silence. The same code on a post-send re-query is
    /// [`Unconfirmed::ReQueryFailed`]: there a send happened.
    Api(SzamlazzAnswer),
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
/// Reserved for exchanges whose outcome is not established. An *answer* to the
/// leading query (another code, `szlahu_down`) is settled data
/// ([`CreateOutcome::Api`], [`CreateOutcome::Unavailable`] and the storno
/// twins), never this. Every variant but [`Unconfirmed::Transport`] on the
/// leading query follows an immediate external-id re-query: one that found no
/// live document of ours,
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
    /// A numbered storno reply did not establish a reversal, and querying
    /// that number did not confirm its identity as the original's storno.
    #[error("storno send returned {number}, but its reversal identity is unconfirmed: {message}")]
    StornoVerification {
        /// The number returned by the send.
        number: String,
        /// The identity mismatch or the failure of the by-number query.
        message: String,
    },
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
/// [`lookup_ours`], [`verify`], [`query`], [`hint`], [`lookup_storno`],
/// [`query_taxpayer`], [`probe`]), and never of a write.
///
/// Every szamlazz.hu *answer* (a document, code 7, rejected credentials,
/// another API code) is the read's data; this is only the exchange that
/// produced none. A read writes nothing, so re-executing it is safe and a
/// re-executed closure's answer is exactly as fresh as a first one; its
/// exhaustion is the handler's `unavailable` fault.
///
/// The one-shot writes ([`delete_proforma`], [`set_credit_entries`]) carry the same
/// two shapes as data, [`DeleteOutcome::Lost`] / [`SetCreditEntriesOutcome::Lost`]
/// (the *Lost answer*): their step runs once and re-executes nothing, so the
/// send that drew no answer is journaled and answered as `outcome_unknown`.
/// Serialisable for that one use; never journaled on its own.
///
/// [`lookup`]: Gateway::lookup
/// [`lookup_ours`]: Gateway::lookup_ours
/// [`verify`]: Gateway::verify
/// [`query`]: Gateway::query
/// [`hint`]: Gateway::hint
/// [`lookup_storno`]: Gateway::lookup_storno
/// [`query_taxpayer`]: Gateway::query_taxpayer
/// [`probe`]: Gateway::probe
/// [`delete_proforma`]: Gateway::delete_proforma
/// [`set_credit_entries`]: Gateway::set_credit_entries
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[non_exhaustive]
pub enum Unanswered {
    /// The HTTP exchange or the response parse failed.
    #[error("transport failure: {0}")]
    Transport(String),
    /// szamlazz.hu reported unavailability (`szlahu_down`).
    #[error("szamlazz.hu is unavailable: {0}")]
    Unavailable(String),
}

impl Unanswered {
    /// The failure of an exchange szamlazz.hu did not answer: `szlahu_down`
    /// is [`Unanswered::Unavailable`], anything else [`Unanswered::Transport`].
    /// The caller has matched the answers (`ClientError::Api`, and for a
    /// write `ClientError::Request`) off first.
    fn from_exchange(error: ClientError) -> Self {
        match error {
            ClientError::ServiceUnavailable(message) => Self::Unavailable(message),
            other => Self::Transport(other.to_string()),
        }
    }
}

/// The answered result of a query by number, external id or order number
/// ([`Gateway::verify`], [`Gateway::query`], [`Gateway::hint`]). A query
/// szamlazz.hu did not answer is [`Unanswered`], never an outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum QueryOutcome {
    /// The document.
    Found(Box<FoundDocument>),
    /// szamlazz.hu does not know the selector (code 7): unknown number, order
    /// number or external id, or a deleted / consumed proforma.
    NotFound,
    /// szamlazz.hu rejected the agent credentials (3, 135, 136, 164); the
    /// check was not made. See [`ErrorCode::is_credential_error`].
    CredentialsRejected(SzamlazzAnswer),
    /// szamlazz.hu answered with another code: an answer the caller cannot
    /// conclude a document from.
    Api(SzamlazzAnswer),
}

/// The answered result of a query by one of **our** external ids, validated
/// against the document it should hold ([`Gateway::lookup_ours`]): the one
/// "is this document ours?" read, journaled by every step that decides on
/// what an external id of the order holds without issuing (the exclusivity
/// checks, the proforma link, `get`, the delete's read). The lookup and
/// create steps ask the same question of the same query inside their own
/// outcomes ([`LookupOutcome`], [`CreateOutcome`]); the validation is one
/// fn, [`FoundDocument::is_ours`], applied in one place.
///
/// A query szamlazz.hu did not answer is [`Unanswered`], never an outcome.
/// Documents are boxed: a [`FoundDocument`] is large next to the unit
/// variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OwnershipOutcome {
    /// szamlazz.hu holds nothing under the id (code 7).
    Absent,
    /// A live document of ours: it carries the order number and the `tipus`
    /// of the kind.
    Live(Box<FoundDocument>),
    /// A reversed document of ours.
    Reversed(Box<FoundDocument>),
    /// The newest holder of the id fails validation: another order's or
    /// kind's document. Never trusted, and never read as "absent": a
    /// document of ours may be hidden behind it.
    Collision(Box<FoundDocument>),
    /// szamlazz.hu rejected the agent credentials (3, 135, 136, 164); the
    /// check was not made. See [`ErrorCode::is_credential_error`].
    CredentialsRejected(SzamlazzAnswer),
    /// szamlazz.hu answered with another code: an answer the caller cannot
    /// conclude a document from.
    Api(SzamlazzAnswer),
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
    /// [`ErrorCode::is_credential_error`].
    CredentialsRejected(SzamlazzAnswer),
}

/// What the taxpayer query of `Szamlazz.Agent.query_taxpayer` learned from
/// one `xmltaxpayer` exchange ([`Gateway::query_taxpayer`]).
///
/// Every szamlazz.hu answer is data: NAV's verdict on the prefix (valid or
/// not) is [`TaxpayerOutcome::Found`], a credential code is
/// [`TaxpayerOutcome::CredentialsRejected`], any other `funcCode ≠ OK` (a
/// NAV-side failure szamlazz.hu relays, a szamlazz.hu code of its own) is
/// [`TaxpayerOutcome::Api`]. An exchange that produced no answer is
/// [`Unanswered`], never an outcome. Journaled as the read step's result; it
/// carries the crate-owned [`QueryTaxpayerResponse`], never the agent crate's
/// `TaxpayerInfo`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TaxpayerOutcome {
    /// NAV answered: the taxpayer as registered, or `valid: false`.
    Found(QueryTaxpayerResponse),
    /// szamlazz.hu rejected the agent credentials (3, 135, 136, 164). See
    /// [`ErrorCode::is_credential_error`].
    CredentialsRejected(SzamlazzAnswer),
    /// szamlazz.hu answered with another code: its own, or NAV's
    /// `errorCode` relayed under `funcCode ERROR`.
    Api(SzamlazzAnswer),
}

/// Why a raw query returned no document: szamlazz.hu's answers as the
/// gateway classifies them internally, before each read fn splits them into
/// its outcome (the answers: 7, a credential code, another code) and
/// [`Unanswered`] (the rest). Crate-private: it appears in no public
/// signature; the read fns answer [`Unanswered`], the write steps
/// [`Unconfirmed`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum QueryError {
    /// szamlazz.hu does not know the selector (code 7).
    #[error("szamlazz.hu does not know the document (code 7)")]
    NotFound,
    /// szamlazz.hu rejected the agent credentials (3, 135, 136, 164).
    #[error("szamlazz.hu rejected the agent credentials ({0})")]
    CredentialsRejected(SzamlazzAnswer),
    /// szamlazz.hu reported another error.
    #[error("szamlazz.hu error {0}")]
    Api(SzamlazzAnswer),
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
    CredentialsRejected(SzamlazzAnswer),
    /// Any other code.
    Api(SzamlazzAnswer),
}

impl QueryError {
    /// Splits the error into what szamlazz.hu answered and what it did not:
    /// `Ok` is an [`Answer`] for the read fn to turn into its outcome, `Err`
    /// the [`Unanswered`] exchange the read policy re-executes.
    fn answered(self) -> Result<Answer, Unanswered> {
        match self {
            Self::NotFound => Ok(Answer::NotFound),
            Self::CredentialsRejected(answer) => Ok(Answer::CredentialsRejected(answer)),
            Self::Api(answer) => Ok(Answer::Api(answer)),
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
    /// [`ErrorCode::is_credential_error`].
    CredentialsRejected(SzamlazzAnswer),
    /// szamlazz.hu answered with another code: an answer the step cannot
    /// conclude from, and nothing will be sent.
    Api(SzamlazzAnswer),
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

/// What a numbered storno reply establishes before any further query.
#[derive(Debug, PartialEq, Eq)]
enum StornoReplyEvidence {
    Reversal,
    SameNumberEcho,
    NeedsVerification,
}

impl StornoReplyEvidence {
    fn of(created: &CreatedInvoice, original: &InvoiceNumber) -> Self {
        if created.reverses(original) {
            Self::Reversal
        } else if created.invoice_number == *original {
            Self::SameNumberEcho
        } else {
            Self::NeedsVerification
        }
    }
}

/// The settled result of the storno step: szamlazz.hu's answer is known.
/// What is *not* settled is an [`Unconfirmed`] error, which the run retry
/// policy re-executes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum StornoOutcome {
    /// The invoice is reversed by the storno invoice szamlazz.hu issued (now,
    /// or echoed by an idempotent repeat), established by the reply heuristic
    /// [`CreatedInvoice::reverses`](szamlazz_agent::ops::invoice::CreatedInvoice::reverses)
    /// or, for a changed number with absent/positive gross, a by-number query
    /// confirming its storno type and original reference in [`Gateway::storno`].
    Reversed(IssuedDocument),
    /// The storno invoice is under the storno external id, found by the
    /// leading query (an earlier execution of this step, or the lookup step's
    /// race, sent it) or by reconciliation after a lost or ambiguous reply. Nothing was
    /// sent, or what was sent landed.
    AlreadyReversed {
        /// The storno invoice number.
        storno_number: String,
    },
    /// szamlazz.hu answered success but echoed the requested number. This
    /// retains the no-op policy observed on proformas and delivery notes with
    /// positive totals; the number comparison does not require totals.
    NotStornoable,
    /// szamlazz.hu refused (14: the document is itself a storno; 221: it has a
    /// corrective; …).
    Rejected(Rejection),
    /// szamlazz.hu rejected the agent credentials (3, 135, 136, 164) on the
    /// leading query or the storno send itself; that request was not acted
    /// on. A credential failure during post-send verification/reconciliation
    /// is instead [`Unconfirmed`]. Settled data: re-executing with the same
    /// key would only repeat the answer. See [`ErrorCode::is_credential_error`].
    CredentialsRejected(SzamlazzAnswer),
    /// szamlazz.hu answered the **leading** query with another code (neither
    /// 7 nor a credential code): nothing was sent. Settled data, as
    /// [`CreateOutcome::Api`] is for the create step.
    Api(SzamlazzAnswer),
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
#[non_exhaustive]
pub enum DeleteOutcome {
    /// Deleted now.
    Deleted,
    /// szamlazz.hu no longer knows the proforma (335): already deleted or
    /// consumed.
    AlreadyGone,
    /// szamlazz.hu refused.
    Rejected(Rejection),
    /// szamlazz.hu rejected the agent credentials (3, 135, 136, 164); nothing
    /// was deleted. See [`ErrorCode::is_credential_error`].
    CredentialsRejected(SzamlazzAnswer),
    /// The *Lost answer*: the delete was sent and szamlazz.hu did not answer
    /// it (a transport or parse failure, or `szlahu_down`), so whether it
    /// acted is not known. Data, not an error: the step runs once
    /// (`run_once`) and the handler answers `outcome_unknown`.
    Lost(Unanswered),
}

/// A szamlazz.hu error on a deletion: 335 is [`DeleteOutcome::AlreadyGone`],
/// a credential code [`DeleteOutcome::CredentialsRejected`], anything else
/// [`DeleteOutcome::Rejected`].
impl From<ApiError> for DeleteOutcome {
    fn from(api: ApiError) -> Self {
        if api.code == ErrorCode::ProformaNotFound {
            Self::AlreadyGone
        } else if api.code.is_credential_error() {
            Self::CredentialsRejected(api.into())
        } else {
            Self::Rejected(api.into())
        }
    }
}

/// The result of registering credit entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SetCreditEntriesOutcome {
    /// The entries are registered.
    Done {
        /// Outstanding amount after the update.
        outstanding: Option<Decimal>,
        /// Gross total of the invoice.
        gross: Option<Decimal>,
    },
    /// szamlazz.hu (or the wire contract: more than five entries) refused.
    Rejected(Rejection),
    /// szamlazz.hu rejected the agent credentials (3, 135, 136, 164); nothing
    /// was registered. See [`ErrorCode::is_credential_error`].
    CredentialsRejected(SzamlazzAnswer),
    /// The *Lost answer*: the entries were sent and szamlazz.hu did not
    /// answer (a transport or parse failure, or `szlahu_down`), so whether
    /// they landed is not known. Data, not an error: the step runs once
    /// without a run retry and the handler answers `outcome_unknown`. Query
    /// first: an additive caller sends only missing entries; a replacing
    /// caller sends the current intended snapshot if replacement is still wanted.
    Lost(Unanswered),
}

/// A successful registration: [`SetCreditEntriesOutcome::Done`] with the reported
/// totals.
impl From<InvoiceBalance> for SetCreditEntriesOutcome {
    fn from(balance: InvoiceBalance) -> Self {
        Self::Done {
            outstanding: balance.outstanding,
            gross: balance.gross_total,
        }
    }
}

/// A szamlazz.hu error on a registration is a rejection, unless it is a
/// credential code.
impl From<ApiError> for SetCreditEntriesOutcome {
    fn from(api: ApiError) -> Self {
        if api.code.is_credential_error() {
            Self::CredentialsRejected(api.into())
        } else {
            Self::Rejected(api.into())
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
    /// The client is the Számla Agent crate's default one, with its
    /// [`REQUEST_TIMEOUT`](szamlazz_agent::client::REQUEST_TIMEOUT): the
    /// issue policy's floor
    /// ([`IssueConfig::MIN_INITIAL_DELAY`](crate::config::IssueConfig::MIN_INITIAL_DELAY))
    /// is derived from that constant, and holds because this constructor,
    /// the one the prologue opens every execution's gateway with, never
    /// supplies a client of its own. [`Gateway::open_with_http`] does, and
    /// the timeout on it is the caller's; a deployment that opened its
    /// gateways that way would have to size the floor itself.
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
                Answer::CredentialsRejected(answer) => {
                    return Ok(LookupOutcome::CredentialsRejected(answer));
                }
                Answer::Api(answer) => {
                    tracing::warn!(code = %answer.code, "the external-id query was answered with another code");
                    return Ok(LookupOutcome::Api(answer));
                }
            },
        };

        // Step 2: the order-number hint; correctives are exempt.
        let mut storno_number = None;
        if request.kind != IssuedKind::Corrective {
            match self.hint_raw(request.order).await {
                Ok(hint) => {
                    let seen = reversed.as_deref().map(|found| found.number.as_str());
                    if is_foreign(&hint, request.our_numbers, seen) {
                        tracing::warn!(
                            number = %hint.number,
                            tipus = %hint.document_type,
                            "foreign document under the order"
                        );
                        return Ok(LookupOutcome::Foreign(Box::new(hint)));
                    }
                    if let Some(reversed) = &reversed
                        && hint.is_storno_of(&reversed.number)
                    {
                        storno_number = Some(hint.number.clone());
                    }
                }
                Err(error) => match error.answered()? {
                    Answer::CredentialsRejected(answer) => {
                        return Ok(LookupOutcome::CredentialsRejected(answer));
                    }
                    // A miss or another code says nothing about foreign
                    // documents.
                    Answer::NotFound | Answer::Api(_) => {}
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
            Err(QueryError::Api(answer)) => {
                tracing::warn!(code = %answer.code, "the leading query was answered with another code");
                return Ok(CreateOutcome::Api(answer));
            }
            Err(QueryError::Unavailable(message)) => {
                tracing::warn!("the leading query was answered with szlahu_down");
                return Ok(CreateOutcome::Unavailable { message });
            }
            // `settled_by_query` settles the credential codes; likewise.
            Err(QueryError::CredentialsRejected(answer)) => {
                return Ok(CreateOutcome::CredentialsRejected(answer));
            }
            Err(QueryError::Transport(message)) => return Err(Unconfirmed::Transport(message)),
        }

        // Step 2: create.
        match self.client.send(request.create).await {
            Ok(CreationOutcome::Issued(created)) => {
                let issued = IssuedDocument::from(created);
                tracing::info!(number = %issued.number, "document issued");
                Ok(CreateOutcome::Issued(issued))
            }
            // A success without a document number (a preview, which the
            // worker never asks for, or an arm the agent crate adds later):
            // nothing the step can name, so it re-queries.
            Ok(_) => {
                let open = Unconfirmed::Open {
                    code: None,
                    message: "the create succeeded without a document number".to_owned(),
                };
                self.settle_or(request, open).await
            }
            Err(error) => match classify_failure(error) {
                Failure::Rejected(rejection) => {
                    tracing::info!(code = %rejection.code, "document rejected");
                    Ok(CreateOutcome::Rejected(rejection))
                }
                Failure::CredentialsRejected(answer) => {
                    Ok(CreateOutcome::CredentialsRejected(answer))
                }
                Failure::Unknown(answer) => {
                    tracing::warn!(code = %answer.code, "open code; re-querying");
                    let open = Unconfirmed::Open {
                        code: Some(answer.code),
                        message: answer.message,
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
                Failure::Duplicate(answer) => {
                    tracing::info!(code = %answer.code, "duplicate order number; re-querying");
                    self.after_duplicate(request, answer).await
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
        answer: SzamlazzAnswer,
    ) -> Result<CreateOutcome, Unconfirmed> {
        match self.settled_by_query(request).await {
            Ok(Some(CreateOutcome::Found(found))) => {
                tracing::info!(number = %found.number, "reconciled after duplicate");
                return Ok(CreateOutcome::Reconciled(found));
            }
            Ok(Some(settled)) => return Ok(settled),
            Ok(None) => {}
            // Whether the duplicate is ours is what the re-query was to
            // settle; unconfirmed, naming the refusal it was resolving.
            Err(error) => {
                return Err(Unconfirmed::ReQueryFailed {
                    sent: format!("duplicate order number {answer}"),
                    re_query: error.to_string(),
                });
            }
        }

        if request.kind == IssuedKind::Corrective {
            tracing::info!(code = %answer.code, "duplicate order number on a corrective: rejected");
            return Ok(CreateOutcome::Rejected(answer.into()));
        }

        let existing_number = match self.hint_raw(request.order).await {
            Ok(newest)
                if newest.is_live() && newest.document_type == request.kind.document_type() =>
            {
                Some(newest.number)
            }
            Ok(_) => None,
            Err(QueryError::NotFound) => {
                // A contradiction (szamlazz.hu refused the order number yet
                // knows nothing under it), but still a refusal it has already
                // given: settled, not re-sent.
                tracing::warn!(
                    code = %answer.code,
                    order = %request.order,
                    "duplicate order number reported but nothing is under the order"
                );
                None
            }
            Err(QueryError::CredentialsRejected(answer)) => {
                return Ok(CreateOutcome::CredentialsRejected(answer));
            }
            Err(error) => {
                tracing::warn!(error = %error, "could not name the duplicate");
                None
            }
        };
        Ok(CreateOutcome::DuplicateOrderNumber {
            answer,
            existing_number,
        })
    }

    /// The external-id query of the create step, decided by
    /// [`settle_create`] against `request.reversed`: `Some` when it settles
    /// the step, `None` when the send may proceed. The two settlements that
    /// are worth an operator's attention (the lookup's reversed document
    /// live again, a document reversed since the lookup) are logged here,
    /// so the decision itself has no effect but its answer.
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
        let settled = settle_create(
            self.seen(request.external_id, request.order, request.kind)
                .await,
            request.reversed,
        );
        match &settled {
            // The document the lookup saw reversed, reported live: a server
            // inconsistency the step never sends past.
            Ok(Some(CreateOutcome::LiveAgain(found))) => tracing::warn!(
                number = %found.number,
                "the document the lookup saw reversed is reported live"
            ),
            // A reversed document the lookup did not see: issued and reversed
            // since. Never sent past: the caller has not acknowledged it.
            Ok(Some(CreateOutcome::Reversed(found))) => tracing::warn!(
                number = %found.number,
                "a document reversed since the lookup holds the external id"
            ),
            _ => {}
        }
        settled
    }

    /// The external-id query of the lookup, create and ownership reads,
    /// validated against the `order` and `kind` the document should have
    /// ([`FoundDocument::is_ours`]): the one place the question is asked, so
    /// the collision warning is written once.
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
            Ok(found) if !found.is_ours(order, kind) => {
                tracing::warn!(number = %found.number, "external id collision");
                Ok(Seen::Collision(Box::new(found)))
            }
            Ok(found) if found.is_live() => {
                tracing::info!(number = %found.number, "found live under external id");
                Ok(Seen::Live(Box::new(found)))
            }
            Ok(found) => {
                tracing::info!(number = %found.number, "found reversed under external id");
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

    /// Queries one of our external ids and validates what it holds against
    /// the `order` and `kind` the document should have: the "is this document
    /// ours?" read of every step that decides on an external id of the order
    /// without issuing. A holder that is not ours is
    /// [`OwnershipOutcome::Collision`], logged at `warn`; code 7 is
    /// [`OwnershipOutcome::Absent`]; a credential code and another code are
    /// the two answered variants.
    ///
    /// # Errors
    ///
    /// [`Unanswered`] when the query got no answer; the caller's read policy
    /// re-executes the step.
    pub async fn lookup_ours(
        &self,
        external_id: &ExternalId,
        order: &OrderKey,
        kind: IssuedKind,
    ) -> Result<OwnershipOutcome, Unanswered> {
        let span = tracing::info_span!(
            "gateway.lookup_ours",
            external_id = %external_id,
            kind = %kind,
        );
        match self.seen(external_id, order, kind).instrument(span).await {
            Ok(Seen::Absent) => Ok(OwnershipOutcome::Absent),
            Ok(Seen::Live(found)) => Ok(OwnershipOutcome::Live(found)),
            Ok(Seen::Reversed(found)) => Ok(OwnershipOutcome::Reversed(found)),
            Ok(Seen::Collision(found)) => Ok(OwnershipOutcome::Collision(found)),
            Err(error) => Ok(match error.answered()? {
                // `seen` maps code 7 to `Seen::Absent`; the arm keeps the
                // match exhaustive.
                Answer::NotFound => OwnershipOutcome::Absent,
                Answer::CredentialsRejected(answer) => {
                    OwnershipOutcome::CredentialsRejected(answer)
                }
                Answer::Api(answer) => OwnershipOutcome::Api(answer),
            }),
        }
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
                    number = %found.number,
                    "a document carries the probe's sentinel external id; it was not issued by this service"
                );
                Ok(ProbeOutcome::Accepted)
            }
            Err(error) => match error.answered()? {
                Answer::NotFound => Ok(ProbeOutcome::Accepted),
                Answer::Api(answer) => {
                    tracing::debug!(code = %answer.code, "the probe was answered with a non-credential code");
                    Ok(ProbeOutcome::Accepted)
                }
                Answer::CredentialsRejected(answer) => {
                    Ok(ProbeOutcome::CredentialsRejected(answer))
                }
            },
        }
    }

    /// The taxpayer lookup of `Szamlazz.Agent.query_taxpayer`: one
    /// `xmltaxpayer` query of the eight-digit `prefix`, read-only. NAV's
    /// verdict (the registered taxpayer, or `valid: false`) is
    /// [`TaxpayerOutcome::Found`]; rejected credentials are
    /// [`TaxpayerOutcome::CredentialsRejected`]; any other code, szamlazz.hu's
    /// or NAV's relayed one, is [`TaxpayerOutcome::Api`]. Finds no document.
    /// Issues nothing.
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
            Err(ClientError::Api(api)) if api.code.is_credential_error() => {
                Ok(TaxpayerOutcome::CredentialsRejected(api.into()))
            }
            Err(ClientError::Api(api)) => Ok(TaxpayerOutcome::Api(api.into())),
            Err(error) => Err(Unanswered::from_exchange(error)),
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
                Answer::CredentialsRejected(answer) => {
                    Ok(StornoLookupOutcome::CredentialsRejected(answer))
                }
                Answer::Api(answer) => {
                    tracing::warn!(code = %answer.code, "the storno lookup was answered with another code");
                    Ok(StornoLookupOutcome::Api(answer))
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
    ///    (352 otherwise): a response satisfying
    ///    [`CreatedInvoice::reverses`](szamlazz_agent::ops::invoice::CreatedInvoice::reverses)
    ///    is [`StornoOutcome::Reversed`] by the existing heuristic. A changed
    ///    number with absent/positive gross is queried by number: a matching
    ///    storno referencing the original is also [`StornoOutcome::Reversed`].
    ///    An inconclusive verification (including wrong identity, not-found,
    ///    credentials or unavailability) takes external-id reconciliation;
    ///    no conclusive result leaves [`Unconfirmed::StornoVerification`],
    ///    or [`Unconfirmed::ReQueryFailed`] if reconciliation itself failed.
    ///    An echo of the requested number is [`StornoOutcome::NotStornoable`], a
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
            Err(QueryError::Api(answer)) => {
                tracing::warn!(code = %answer.code, "the leading query was answered with another code");
                return Ok(StornoOutcome::Api(answer));
            }
            Err(QueryError::Unavailable(message)) => {
                tracing::warn!("the leading query was answered with szlahu_down");
                return Ok(StornoOutcome::Unavailable { message });
            }
            // `storno_settled_by_query` settles the credential codes; likewise.
            Err(QueryError::CredentialsRejected(answer)) => {
                return Ok(StornoOutcome::CredentialsRejected(answer));
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
            Ok(created) => match StornoReplyEvidence::of(&created, &storno.invoice_number) {
                StornoReplyEvidence::Reversal => {
                    tracing::info!(storno_number = %created.invoice_number, "invoice reversed");
                    Ok(StornoOutcome::Reversed(IssuedDocument::from(created)))
                }
                StornoReplyEvidence::SameNumberEcho => {
                    tracing::info!(echoed = %created.invoice_number, "storno was a no-op");
                    Ok(StornoOutcome::NotStornoable)
                }
                StornoReplyEvidence::NeedsVerification => {
                    self.verify_storno_reply(&request, created).await
                }
            },
            Err(error) => match classify_failure(error) {
                Failure::Rejected(rejection) => {
                    tracing::info!(code = %rejection.code, "storno rejected");
                    Ok(StornoOutcome::Rejected(rejection))
                }
                Failure::Duplicate(answer) => {
                    tracing::info!(code = %answer.code, "storno rejected");
                    Ok(StornoOutcome::Rejected(answer.into()))
                }
                Failure::CredentialsRejected(answer) => {
                    Ok(StornoOutcome::CredentialsRejected(answer))
                }
                Failure::Unknown(answer) => {
                    tracing::warn!(code = %answer.code, "open code; re-querying");
                    let open = Unconfirmed::Open {
                        code: Some(answer.code),
                        message: answer.message,
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

    /// A changed number without non-positive gross needs identity evidence,
    /// inside the same query-first step as the send. A mismatch or query
    /// failure cannot prove a no-op: try the storno external id before
    /// leaving the step unconfirmed.
    async fn verify_storno_reply(
        &self,
        request: &StornoStepRequest<'_>,
        created: CreatedInvoice,
    ) -> Result<StornoOutcome, Unconfirmed> {
        let selector = InvoiceSelector::InvoiceNumber(created.invoice_number.clone());
        let message = match self.query_raw(selector).await {
            Ok(document)
                if document.number == created.invoice_number.as_str()
                    && document.is_storno_of(request.invoice_number) =>
            {
                tracing::info!(storno_number = %created.invoice_number, "storno identity verified");
                return Ok(StornoOutcome::Reversed(IssuedDocument::from(created)));
            }
            Ok(document) => format!(
                "queried {} with document type {} and original {:?}; expected a storno of {}",
                document.number,
                document.document_type,
                document.referenced_invoice_number,
                request.invoice_number,
            ),
            Err(error) => error.to_string(),
        };
        let unconfirmed = Unconfirmed::StornoVerification {
            number: created.invoice_number.to_string(),
            message,
        };
        self.storno_settle_or(request, unconfirmed).await
    }

    /// The immediate re-query after a storno whose reply was lost, open or ambiguous:
    /// a landed storno settles the step; nothing is `unconfirmed`, and a
    /// re-query that fails itself is unconfirmed naming both causes.
    async fn storno_settle_or(
        &self,
        request: &StornoStepRequest<'_>,
        unconfirmed: Unconfirmed,
    ) -> Result<StornoOutcome, Unconfirmed> {
        // Unlike the leading query, a credential failure here says nothing
        // about whether the preceding send landed.
        match self
            .storno_seen(request.external_id, request.invoice_number)
            .await
        {
            Ok(Some(storno_number)) => Ok(StornoOutcome::AlreadyReversed { storno_number }),
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
                tracing::info!(storno_number = %document.number, "storno already issued");
                Ok(Some(document.number))
            }
            Ok(document) => {
                tracing::warn!(
                    number = %document.number,
                    tipus = %document.document_type,
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
            // The wire contract refused before the send: nothing left, so
            // never `Lost`.
            Err(ClientError::Request(error)) => {
                DeleteOutcome::Rejected(Rejection::request(error.to_string()))
            }
            Err(error) => DeleteOutcome::Lost(Unanswered::from_exchange(error)),
        }
    }

    /// Registers `entries` on invoice `number`, replacing the existing entries
    /// unless `additive`.
    pub async fn set_credit_entries(
        &self,
        number: &str,
        entries: &[CreditEntryInput],
        additive: bool,
    ) -> SetCreditEntriesOutcome {
        let credit_entries = entries.iter().map(CreditEntry::from).collect::<Vec<_>>();
        let credit_entries = match CreditEntries::try_from(credit_entries) {
            Ok(entries) => entries,
            Err(error) => {
                return SetCreditEntriesOutcome::Rejected(Rejection::request(error.to_string()));
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
                SetCreditEntriesOutcome::from(result)
            }
            Err(ClientError::Api(api)) => SetCreditEntriesOutcome::from(api),
            Err(ClientError::Request(error)) => {
                SetCreditEntriesOutcome::Rejected(Rejection::request(error.to_string()))
            }
            Err(error) => SetCreditEntriesOutcome::Lost(Unanswered::from_exchange(error)),
        }
    }

    /// The order-number hint as a raw query result.
    async fn hint_raw(&self, order: &OrderKey) -> Result<FoundDocument, QueryError> {
        self.query_raw(InvoiceSelector::OrderNumber(order.as_str().to_owned()))
            .await
    }

    /// One `xmlszamlaxml` query, its answer projected onto the worker's
    /// [`FoundDocument`] at this boundary: nothing past it holds the agent
    /// crate's document.
    async fn query_raw(&self, selector: InvoiceSelector) -> Result<FoundDocument, QueryError> {
        match self.client.send(&QueryInvoiceXml::new(selector)).await {
            Ok(document) => Ok(FoundDocument::from(document)),
            Err(ClientError::Api(api)) if api.code == ErrorCode::MissingData => {
                Err(QueryError::NotFound)
            }
            Err(ClientError::Api(api)) if api.code.is_credential_error() => {
                Err(QueryError::CredentialsRejected(api.into()))
            }
            Err(ClientError::Api(api)) => Err(QueryError::Api(api.into())),
            Err(ClientError::ServiceUnavailable(message)) => Err(QueryError::Unavailable(message)),
            Err(error) => Err(QueryError::Transport(error.to_string())),
        }
    }
}

/// What the external-id query of the lookup, create and ownership reads saw,
/// validated ([`Gateway::seen`]); each read's outcome is projected from it.
enum Seen {
    /// Code 7.
    Absent,
    /// A live document of ours.
    Live(Box<FoundDocument>),
    /// A reversed document of ours.
    Reversed(Box<FoundDocument>),
    /// A document that fails validation. Never trusted.
    Collision(Box<FoundDocument>),
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
    /// refused before acting (on a write, 7 is a missing field), or the wire
    /// contract refused before anything was sent.
    Rejected(Rejection),
    /// [`OutcomeClass::DuplicateOrderNumber`] (71/152).
    Duplicate(SzamlazzAnswer),
    /// See [`ErrorCode::is_credential_error`].
    CredentialsRejected(SzamlazzAnswer),
    /// [`OutcomeClass::Unknown`]: 1, 55, 56 without a number, a code the
    /// agent crate does not know, or any class added to the crate later: the
    /// outcome is open, re-query.
    Unknown(SzamlazzAnswer),
    /// `szlahu_down`: whether szamlazz.hu acted before answering is not
    /// known, re-query.
    Unavailable(String),
    Transport(String),
}

fn classify_failure(error: ClientError) -> Failure {
    match error {
        ClientError::Api(api) if api.code.is_credential_error() => {
            Failure::CredentialsRejected(api.into())
        }
        ClientError::Api(api) => match api.code.outcome_class() {
            OutcomeClass::DuplicateOrderNumber => Failure::Duplicate(api.into()),
            OutcomeClass::Rejected | OutcomeClass::NotFound => Failure::Rejected(api.into()),
            // `Unknown`, and any class the agent crate adds later: a
            // document may exist, so the step re-queries rather than
            // claims `rejected`.
            _ => Failure::Unknown(api.into()),
        },
        ClientError::ServiceUnavailable(message) => Failure::Unavailable(message),
        ClientError::Request(error) => Failure::Rejected(Rejection::request(error.to_string())),
        other => Failure::Transport(other.to_string()),
    }
}

/// Step 2 of [`Gateway::lookup`]: whether the order-number hint is a live
/// invoice-kind document that is neither known to be ours nor the document
/// seen under our external id.
fn is_foreign(found: &FoundDocument, our_numbers: &[String], seen: Option<&str>) -> bool {
    found.is_invoice_family()
        && found.is_live()
        && Some(found.number.as_str()) != seen
        && !our_numbers.contains(&found.number)
}

/// A raw query result as the read's outcome: every answer is data, no answer
/// is [`Unanswered`].
fn outcome(result: Result<FoundDocument, QueryError>) -> Result<QueryOutcome, Unanswered> {
    match result {
        Ok(document) => Ok(QueryOutcome::Found(Box::new(document))),
        Err(error) => Ok(match error.answered()? {
            Answer::NotFound => QueryOutcome::NotFound,
            Answer::CredentialsRejected(answer) => QueryOutcome::CredentialsRejected(answer),
            Answer::Api(answer) => QueryOutcome::Api(answer),
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
/// ([`CreateOutcome::CredentialsRejected`]). A function of its two inputs
/// with no other effect; the wrapper that queries logs the settlements worth
/// noting.
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
        Ok(Seen::Live(found)) if Some(found.number.as_str()) != reversed => {
            Ok(Some(CreateOutcome::Found(found)))
        }
        // The document the lookup saw reversed, reported live: a server
        // inconsistency. Never send past it.
        Ok(Seen::Live(found)) => Ok(Some(CreateOutcome::LiveAgain(found))),
        // A reversed document the lookup did not see: issued and reversed
        // since. Never send past a reversal the caller has not acknowledged.
        Ok(Seen::Reversed(found)) if Some(found.number.as_str()) != reversed => {
            Ok(Some(CreateOutcome::Reversed(found)))
        }
        // Nothing (code 7), or the document the lookup saw reversed, still
        // reversed.
        Ok(Seen::Reversed(_) | Seen::Absent) => Ok(None),
        Err(QueryError::CredentialsRejected(answer)) => {
            Ok(Some(CreateOutcome::CredentialsRejected(answer)))
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
        Err(QueryError::CredentialsRejected(answer)) => {
            Ok(Some(StornoOutcome::CredentialsRejected(answer)))
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
    use rust_decimal::dec;
    use szamlazz_agent::wire::{AgentRequest as _, RawResponse};
    use szamlazz_agent::{ParseError, RequestError};

    use super::*;
    use crate::test_support::Doc;

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
    fn set_credit_entries_outcome_from_credit_entry_result() {
        let result = RegisterCreditEntry::new("SZ-1")
            .parse(&response(&created("SZ-1", "1000", "1270", "270")))
            .expect("parse");
        assert_eq!(
            SetCreditEntriesOutcome::from(result),
            SetCreditEntriesOutcome::Done {
                outstanding: Some(dec!(270)),
                gross: Some(dec!(1270)),
            }
        );
    }

    #[test]
    fn api_errors_map_to_delete_and_set_credit_entries_outcomes() {
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
            DeleteOutcome::Rejected(Rejection::from(SzamlazzAnswer::new("57", "xml")))
        );
        assert_eq!(
            SetCreditEntriesOutcome::from(malformed),
            SetCreditEntriesOutcome::Rejected(Rejection::from(SzamlazzAnswer::new("57", "xml")))
        );

        // A failure szamlazz.hu reported without a code is answered as
        // `absent`, never as an empty code, in the journal and in a fault.
        let codeless = ApiError {
            code: ErrorCode::Absent,
            message: "Hiba".to_owned(),
        };
        assert_eq!(
            SzamlazzAnswer::from(codeless.clone()),
            SzamlazzAnswer::new("absent", "Hiba")
        );
        assert_eq!(
            SetCreditEntriesOutcome::from(codeless),
            SetCreditEntriesOutcome::Rejected(Rejection::from(SzamlazzAnswer::new(
                "absent", "Hiba"
            )))
        );

        for code in [
            ErrorCode::InvalidCredentials,
            ErrorCode::BrowserSessionActive,
            ErrorCode::LoginBlocked,
            ErrorCode::MultipleAccounts,
        ] {
            assert!(code.is_credential_error(), "{code:?}");
            let login = ApiError {
                code: code.clone(),
                message: "login".to_owned(),
            };
            assert_eq!(
                DeleteOutcome::from(login.clone()),
                DeleteOutcome::CredentialsRejected(SzamlazzAnswer::new(
                    code.code().to_owned(),
                    "login"
                ))
            );
            assert_eq!(
                SetCreditEntriesOutcome::from(login),
                SetCreditEntriesOutcome::CredentialsRejected(SzamlazzAnswer::new(
                    code.code().to_owned(),
                    "login"
                ))
            );
        }
        for code in [
            ErrorCode::MissingData,
            ErrorCode::Maintenance,
            ErrorCode::ProformaNotFound,
            ErrorCode::Unknown("999".to_owned()),
        ] {
            assert!(!code.is_credential_error(), "{code:?}");
        }
    }

    // The pure classifiers behind the async steps, each on its own table.
    // The wiremock suite (`tests/gateway/`) reaches them through HTTP and
    // keeps one exchange per operation for the header-vs-body parse path;
    // the branches are asserted here.

    #[test]
    fn numbered_storno_reply_evidence_distinguishes_echo_from_uncertainty() {
        use StornoReplyEvidence::{NeedsVerification, Reversal, SameNumberEcho};

        // Zero and negative-original shapes are synthetic policy controls,
        // not evidence of szamlazz.hu accepting those originals.
        let rows = [
            ("SZ-1", None, SameNumberEcho),
            ("SZ-1", Some("-1270"), SameNumberEcho),
            ("SZ-1", Some("0"), SameNumberEcho),
            ("SZ-1", Some("1270"), SameNumberEcho),
            ("SS-1", None, NeedsVerification),
            ("SS-1", Some("-1270"), Reversal),
            ("SS-1", Some("0"), Reversal),
            ("SS-1", Some("1270"), NeedsVerification),
        ];
        let request = StornoInvoice::new("SZ-1");
        for (number, gross, expected) in rows {
            let body = crate::test_support::numbered_reply_body(number, gross);
            let created = request.parse(&response(&body)).expect("numbered reply");
            assert_eq!(
                StornoReplyEvidence::of(&created, &request.invoice_number),
                expected,
                "number {number}, gross {gross:?}",
            );
        }
    }

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
    /// its outcome class, on representative codes of each class (the
    /// exhaustive code → class table is the agent crate's, under its own
    /// tests; every code here asserts its class first): the credential codes
    /// before their class (`Rejected`); 71 and 152 as the duplicate; the
    /// `Rejected` class as `Rejected`, and 7 with it (on a write it is a
    /// missing field, not a missing document; the `NotFound` class); the
    /// open codes 1, 55, 56 and a code the agent crate does not know as
    /// `Unknown`, through the wildcard arm that a class the crate adds later
    /// falls into too.
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
                Failure::CredentialsRejected(SzamlazzAnswer::new(code.code().to_owned(), "üzenet")),
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
                Failure::Duplicate(SzamlazzAnswer::new(
                    code.code().to_owned(),
                    "üzenet".to_owned()
                )),
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
                Failure::Rejected(Rejection::from(SzamlazzAnswer::new(
                    code.code().to_owned(),
                    "üzenet"
                ))),
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
                Failure::Unknown(SzamlazzAnswer::new(
                    code.code().to_owned(),
                    "üzenet".to_owned()
                )),
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
            Failure::Rejected(Rejection::request(message))
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

    /// What the run journals as its last failure (`Unconfirmed`'s display) names
    /// the cause it stands for: `szlahu_down` after a send is unavailability, not
    /// an "open code", and an open answer without a code is szamlazz.hu's success
    /// without a document number, never `szlahu_down` (#63).
    #[test]
    fn unconfirmed_displays_name_their_cause() {
        let open = Unconfirmed::Open {
            code: Some("56".to_owned()),
            message: "signing".to_owned(),
        }
        .to_string();
        assert_eq!(open, "open code 56: signing");

        let no_number = Unconfirmed::Open {
            code: None,
            message: "create succeeded without a document number".to_owned(),
        }
        .to_string();
        assert!(!no_number.contains("szlahu_down"), "{no_number}");
        assert!(no_number.contains("document number"), "{no_number}");

        let down = Unconfirmed::Unavailable("maintenance".to_owned()).to_string();
        assert!(down.contains("szlahu_down"), "{down}");
        assert!(down.contains("maintenance"), "{down}");

        let transport = Unconfirmed::Transport("empty response".to_owned()).to_string();
        assert_eq!(transport, "transport failure: empty response");
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
        let rejected = || {
            QueryError::CredentialsRejected(SzamlazzAnswer::new("3", "Sikertelen bejelentkezés."))
        };
        let other = || QueryError::Api(SzamlazzAnswer::new("57", "Hibás XML."));

        assert_eq!(QueryError::NotFound.answered(), Ok(Answer::NotFound));
        assert_eq!(
            rejected().answered(),
            Ok(Answer::CredentialsRejected(SzamlazzAnswer::new(
                "3",
                "Sikertelen bejelentkezés."
            )))
        );
        assert_eq!(
            other().answered(),
            Ok(Answer::Api(SzamlazzAnswer::new("57", "Hibás XML.")))
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
            Ok(QueryOutcome::CredentialsRejected(SzamlazzAnswer::new(
                "3",
                "Sikertelen bejelentkezés."
            )))
        );
        assert_eq!(
            outcome(Err(other())),
            Ok(QueryOutcome::Api(SzamlazzAnswer::new("57", "Hibás XML.")))
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
            QueryError::Api(SzamlazzAnswer::new("57", "xml")),
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
                    Err(QueryError::CredentialsRejected(SzamlazzAnswer::new(
                        "3", "login"
                    ))),
                    lookup_saw,
                ),
                Ok(Some(CreateOutcome::CredentialsRejected(
                    SzamlazzAnswer::new("3", "login")
                ))),
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
            settle_storno(Err(QueryError::CredentialsRejected(SzamlazzAnswer::new(
                "135", "session"
            )))),
            Ok(Some(StornoOutcome::CredentialsRejected(
                SzamlazzAnswer::new("135", "session")
            )))
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
