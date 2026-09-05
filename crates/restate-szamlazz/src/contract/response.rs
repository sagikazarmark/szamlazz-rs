//! Handler outputs of the `Szamlazz.Order` Virtual Object and the
//! `Szamlazz.Agent` service.
//!
//! Domain outcomes are data, returned with HTTP 200 through the ingress. A
//! `TerminalError` (see [`TerminalCode`](super::TerminalCode)) is reserved
//! for faults and always means "outcome unknown — retry with a new
//! `Idempotency-Key`".

use jiff::civil::Date;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use szamlazz_agent::ops::query_xml::{InvoiceDocument, RecordedPayment};

use super::IssuedKind;
use crate::account::Account;
use crate::config::AccountMode;

/// The domain outcome of a create or correct request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Outcome {
    /// szamlazz.hu issued the document in this invocation.
    Issued,
    /// A live document of this kind already exists under our external id;
    /// nothing new was issued.
    AlreadyIssued,
    /// szamlazz.hu refused the order number as a duplicate (71/152) and the
    /// external-id re-query found our live document: an earlier attempt had
    /// landed.
    Reconciled,
    /// The document of this kind was reversed; nothing new was issued. Pass
    /// `reissue: true` (with a new `Idempotency-Key`) to issue a new one.
    Reversed,
    /// szamlazz.hu refused the document; see `code` and `message`.
    Rejected,
    /// The request contradicts what szamlazz.hu holds for the order; see
    /// `conflict_reason`.
    Conflict,
}

/// Why a request was answered with [`Outcome::Conflict`] or
/// [`StornoOutcome::Conflict`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConflictReason {
    /// A live prepayment invoice (or, for `create_prepayment`, a live invoice)
    /// exists for the order: the two chains are exclusive.
    PrepaidChain,
    /// `reissue: true` while the document is live.
    Live,
    /// A live invoice-kind document that is not ours exists under the order
    /// number; see `existing_number`.
    Foreign,
    /// szamlazz.hu refuses the order number as a duplicate (71/152) and no
    /// live document of ours can be found under our external id.
    DuplicateOrderNumber,
    /// A document found under our external id belongs to another order,
    /// kind, account mode or supplier.
    ExternalIdCollision,
    /// `proforma: none` while a live proforma of ours exists.
    ProformaLive,
    /// The referenced proforma cannot be found.
    ProformaMissing,
    /// `create_final` without a prepayment invoice of ours.
    PrepaymentMissing,
    /// `create_final` while the prepayment invoice is reversed.
    PrepaymentReversed,
    /// `correct_invoice` on a reversed invoice.
    BaseReversed,
    /// The document does not carry this order's number; use
    /// `Szamlazz.Agent.storno` or the managing order.
    NotManaged,
}

/// Informational flags attached to a successful response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Warning {
    /// The document was issued but szamlazz.hu could not deliver its
    /// notification email (code 56).
    NotificationDeliveryFailed,
}

/// Output of every create and correct handler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct CreateResponse {
    /// The domain outcome.
    pub outcome: Outcome,
    /// Present when `outcome` is [`Outcome::Conflict`].
    #[serde(default)]
    pub conflict_reason: Option<ConflictReason>,
    /// The document kind.
    pub kind: IssuedKind,
    /// The external id (`szamlaKulsoAzon`) the document carries.
    pub external_id: String,
    /// The document's number, when one exists.
    #[serde(default)]
    pub invoice_number: Option<String>,
    /// The storno invoice number, when `outcome` is [`Outcome::Reversed`]
    /// and it is known.
    #[serde(default)]
    pub storno_number: Option<String>,
    /// Net total (`nettó végösszeg`).
    #[serde(default)]
    pub net_total: Option<Decimal>,
    /// Gross total (`bruttó végösszeg`).
    #[serde(default)]
    pub gross_total: Option<Decimal>,
    /// Outstanding amount (`kintlévőség`).
    #[serde(default)]
    pub outstanding: Option<Decimal>,
    /// Buyer-facing account URL (`vevői fiók URL`).
    #[serde(default)]
    pub customer_account_url: Option<String>,
    /// The number of the document a conflict is about (the live document on
    /// `live`, the foreign document on `foreign`, …).
    #[serde(default)]
    pub existing_number: Option<String>,
    /// szamlazz.hu error code on [`Outcome::Rejected`] (and on
    /// `conflict{duplicate_order_number}`).
    #[serde(default)]
    pub code: Option<String>,
    /// szamlazz.hu error message on [`Outcome::Rejected`].
    #[serde(default)]
    pub message: Option<String>,
    /// Informational flags.
    #[serde(default)]
    pub warnings: Vec<Warning>,
}

impl CreateResponse {
    /// A response with the identity fields set and every optional field
    /// absent.
    #[must_use]
    pub fn new(outcome: Outcome, kind: IssuedKind, external_id: impl Into<String>) -> Self {
        Self {
            outcome,
            conflict_reason: None,
            kind,
            external_id: external_id.into(),
            invoice_number: None,
            storno_number: None,
            net_total: None,
            gross_total: None,
            outstanding: None,
            customer_account_url: None,
            existing_number: None,
            code: None,
            message: None,
            warnings: Vec::new(),
        }
    }

    /// A [`Outcome::Conflict`] response with its reason.
    #[must_use]
    pub fn conflict(
        reason: ConflictReason,
        kind: IssuedKind,
        external_id: impl Into<String>,
    ) -> Self {
        let mut response = Self::new(Outcome::Conflict, kind, external_id);
        response.conflict_reason = Some(reason);
        response
    }

    /// Sets the conflict reason.
    #[must_use]
    pub fn with_conflict_reason(mut self, reason: ConflictReason) -> Self {
        self.conflict_reason = Some(reason);
        self
    }

    /// Sets the document's number.
    #[must_use]
    pub fn with_invoice_number(mut self, number: impl Into<String>) -> Self {
        self.invoice_number = Some(number.into());
        self
    }

    /// Sets the storno invoice number.
    #[must_use]
    pub fn with_storno_number(mut self, number: impl Into<String>) -> Self {
        self.storno_number = Some(number.into());
        self
    }

    /// Sets the net total.
    #[must_use]
    pub fn with_net_total(mut self, net_total: Decimal) -> Self {
        self.net_total = Some(net_total);
        self
    }

    /// Sets the gross total.
    #[must_use]
    pub fn with_gross_total(mut self, gross_total: Decimal) -> Self {
        self.gross_total = Some(gross_total);
        self
    }

    /// Sets the outstanding amount.
    #[must_use]
    pub fn with_outstanding(mut self, outstanding: Decimal) -> Self {
        self.outstanding = Some(outstanding);
        self
    }

    /// Sets the buyer-facing account URL.
    #[must_use]
    pub fn with_customer_account_url(mut self, url: impl Into<String>) -> Self {
        self.customer_account_url = Some(url.into());
        self
    }

    /// Sets the number of the document a conflict is about.
    #[must_use]
    pub fn with_existing_number(mut self, number: impl Into<String>) -> Self {
        self.existing_number = Some(number.into());
        self
    }

    /// Sets the szamlazz.hu error code.
    #[must_use]
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Sets the szamlazz.hu error message.
    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Appends a warning.
    #[must_use]
    pub fn with_warning(mut self, warning: Warning) -> Self {
        self.warnings.push(warning);
        self
    }
}

/// The domain outcome of a storno request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum StornoOutcome {
    /// The invoice is reversed (now or already); `storno_number` is set when
    /// known.
    Reversed,
    /// szamlazz.hu refused the storno; see `code` and `message`.
    Rejected,
    /// The request contradicts what szamlazz.hu holds; see
    /// `conflict_reason`.
    Conflict,
    /// `Szamlazz.Agent.storno` only: the document carries an order number, so
    /// it is managed by the `Order` with key `order_key` — call
    /// `Szamlazz.Order.storno_invoice` there instead.
    ManagedByOrder,
}

/// Output of `Szamlazz.Order.storno_invoice` and `Szamlazz.Agent.storno`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct StornoResponse {
    /// The domain outcome.
    pub outcome: StornoOutcome,
    /// Present when `outcome` is [`StornoOutcome::Conflict`].
    #[serde(default)]
    pub conflict_reason: Option<ConflictReason>,
    /// The invoice the request was about.
    pub invoice_number: String,
    /// The storno invoice number, when known.
    #[serde(default)]
    pub storno_number: Option<String>,
    /// The `Order` key managing the document on
    /// [`StornoOutcome::ManagedByOrder`].
    #[serde(default)]
    pub order_key: Option<String>,
    /// szamlazz.hu error code on [`StornoOutcome::Rejected`].
    #[serde(default)]
    pub code: Option<String>,
    /// szamlazz.hu error message on [`StornoOutcome::Rejected`].
    #[serde(default)]
    pub message: Option<String>,
}

impl StornoResponse {
    /// A response with the identity fields set and every optional field
    /// absent.
    pub fn new(outcome: StornoOutcome, invoice_number: impl Into<String>) -> Self {
        Self {
            outcome,
            conflict_reason: None,
            invoice_number: invoice_number.into(),
            storno_number: None,
            order_key: None,
            code: None,
            message: None,
        }
    }

    /// Sets the conflict reason.
    #[must_use]
    pub fn with_conflict_reason(mut self, reason: ConflictReason) -> Self {
        self.conflict_reason = Some(reason);
        self
    }

    /// Sets the storno invoice number.
    #[must_use]
    pub fn with_storno_number(mut self, number: impl Into<String>) -> Self {
        self.storno_number = Some(number.into());
        self
    }

    /// Sets the managing `Order` key.
    #[must_use]
    pub fn with_order_key(mut self, key: impl Into<String>) -> Self {
        self.order_key = Some(key.into());
        self
    }

    /// Sets the szamlazz.hu error code.
    #[must_use]
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Sets the szamlazz.hu error message.
    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

/// Output of `Szamlazz.Order.delete_proforma`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct DeleteProformaResponse {
    /// Whether the proforma is deleted (now or already).
    pub deleted: bool,
    /// Why it is not deleted (`proforma_paid`, a szamlazz.hu error code, …),
    /// or `absent` when there was nothing to delete (deleted earlier or
    /// consumed — `get` tells which).
    #[serde(default)]
    pub reason: Option<String>,
}

impl DeleteProformaResponse {
    /// The proforma is deleted.
    #[must_use]
    pub const fn deleted() -> Self {
        Self {
            deleted: true,
            reason: None,
        }
    }

    /// There was nothing to delete: szamlazz.hu holds no proforma under our
    /// external id.
    #[must_use]
    pub fn absent() -> Self {
        Self {
            deleted: true,
            reason: Some("absent".to_owned()),
        }
    }

    /// The proforma is not deleted for `reason`.
    pub fn not_deleted(reason: impl Into<String>) -> Self {
        Self {
            deleted: false,
            reason: Some(reason.into()),
        }
    }
}

/// Output of `Szamlazz.Agent.set_payments`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct SetPaymentsResponse {
    /// The invoice the entries were registered on.
    pub invoice_number: String,
    /// Outstanding amount after the update (`kintlévőség`).
    #[serde(default)]
    pub outstanding: Option<Decimal>,
    /// Gross total of the invoice.
    #[serde(default)]
    pub gross_total: Option<Decimal>,
}

impl SetPaymentsResponse {
    /// A response for `invoice_number` without totals.
    pub fn new(invoice_number: impl Into<String>) -> Self {
        Self {
            invoice_number: invoice_number.into(),
            outstanding: None,
            gross_total: None,
        }
    }
}

/// One registered credit entry as szamlazz.hu reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct PaymentRecord {
    /// Payment date.
    #[serde(default)]
    pub date: Option<Date>,
    /// Title / payment method text (`jogcím`).
    #[serde(default)]
    pub title: Option<String>,
    /// Amount in the invoice currency.
    pub amount: Decimal,
    /// Free-text comment.
    #[serde(default)]
    pub comment: Option<String>,
    /// Bank account the payment arrived on.
    #[serde(default)]
    pub bank_account: Option<String>,
}

impl PaymentRecord {
    /// A record of `amount` with every optional field absent.
    #[must_use]
    pub const fn new(amount: Decimal) -> Self {
        Self {
            date: None,
            title: None,
            amount,
            comment: None,
            bank_account: None,
        }
    }
}

impl From<&RecordedPayment> for PaymentRecord {
    fn from(payment: &RecordedPayment) -> Self {
        let mut record = Self::new(payment.amount);
        record.date = Some(payment.date);
        record.title = Some(payment.title.clone());
        record.comment.clone_from(&payment.comment);
        record.bank_account.clone_from(&payment.bank_account);
        record
    }
}

/// Output of `Szamlazz.Agent.query`: a projection of the queried document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct QueryResponse {
    /// Document number (`számlaszám`).
    pub invoice_number: String,
    /// Document type code (`tipus`): `SZ` invoice, `D` proforma, `ES`
    /// prepayment, `VS` final, `SS` storno, `HS` corrective, ….
    pub document_type: String,
    /// Whether the document has been reversed; `None` when szamlazz.hu did
    /// not report it.
    #[serde(default)]
    pub reversed: Option<bool>,
    /// The invoice this document references (`hivszamlaszam`): the reversed
    /// invoice of a storno, the corrected one of a corrective.
    #[serde(default)]
    pub referenced_invoice_number: Option<String>,
    /// The proforma this document converted (`hivdijbekszam`).
    #[serde(default)]
    pub referenced_proforma_number: Option<String>,
    /// Order number (`rendelésszám`).
    #[serde(default)]
    pub order_number: Option<String>,
    /// Issue date (`kelt`).
    #[serde(default)]
    pub issue_date: Option<Date>,
    /// Fulfillment date (`teljesítés`).
    #[serde(default)]
    pub fulfillment_date: Option<Date>,
    /// Payment due date (`fizetési határidő`).
    #[serde(default)]
    pub due_date: Option<Date>,
    /// Currency code (`pénznem`).
    #[serde(default)]
    pub currency: Option<String>,
    /// Net total.
    #[serde(default)]
    pub net_total: Option<Decimal>,
    /// VAT total.
    #[serde(default)]
    pub vat_total: Option<Decimal>,
    /// Gross total.
    #[serde(default)]
    pub gross_total: Option<Decimal>,
    /// Registered credit entries.
    #[serde(default)]
    pub payments: Vec<PaymentRecord>,
    /// Outstanding amount: gross total minus the sum of payments.
    #[serde(default)]
    pub outstanding: Option<Decimal>,
    /// The issuing account's supplier id (`szállító/id`).
    #[serde(default)]
    pub supplier_id: Option<u64>,
    /// Issued from a test account (`teszt`).
    #[serde(default)]
    pub test: bool,
}

impl QueryResponse {
    /// A response with the number and type set and everything else absent.
    pub fn new(invoice_number: impl Into<String>, document_type: impl Into<String>) -> Self {
        Self {
            invoice_number: invoice_number.into(),
            document_type: document_type.into(),
            reversed: None,
            referenced_invoice_number: None,
            referenced_proforma_number: None,
            order_number: None,
            issue_date: None,
            fulfillment_date: None,
            due_date: None,
            currency: None,
            net_total: None,
            vat_total: None,
            gross_total: None,
            payments: Vec::new(),
            outstanding: None,
            supplier_id: None,
            test: false,
        }
    }
}

/// The projection of a queried document: identity, references, dates,
/// totals and payments — no buyer data. `outstanding` is `gross − Σ payments`.
impl From<&InvoiceDocument> for QueryResponse {
    fn from(document: &InvoiceDocument) -> Self {
        let info = &document.info;
        let mut response = Self::new(info.invoice_number.as_str(), info.document_type.clone());
        response.reversed = info.reversed;
        response.referenced_invoice_number = info
            .referenced_invoice_number
            .as_ref()
            .map(|number| number.as_str().to_owned());
        response.referenced_proforma_number = info
            .referenced_proforma_number
            .as_ref()
            .map(|number| number.as_str().to_owned());
        response.order_number.clone_from(&info.order_number);
        response.issue_date = info.issue_date;
        response.fulfillment_date = info.fulfillment_date;
        response.due_date = info.due_date;
        response.currency.clone_from(&info.currency);
        response.net_total = Some(document.totals.total.net);
        response.vat_total = Some(document.totals.total.vat);
        response.gross_total = Some(document.totals.total.gross);
        response.payments = document.payments.iter().map(PaymentRecord::from).collect();
        let amounts: Vec<_> = document
            .payments
            .iter()
            .map(|payment| payment.amount)
            .collect();
        response.outstanding = outstanding(response.gross_total, &amounts);
        response.supplier_id = document.supplier.id;
        response.test = info.test;
        response
    }
}

/// Output of `Szamlazz.Order.get`: what szamlazz.hu holds under the order's
/// four external ids right now. Carries numbers and totals — never buyer
/// data.
///
/// A slot is `None` when szamlazz.hu holds nothing under its external id
/// *or* when the newest holder of the id fails validation (an external-id
/// collision: another order, kind, account mode or supplier). A read must
/// not fail, so `get` reports such a slot as absent; the issuing handlers
/// refuse the same situation as `conflict{external_id_collision}`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(default)]
#[non_exhaustive]
pub struct OrderStatus {
    /// The proforma, when szamlazz.hu holds one — or, when an invoice or
    /// prepayment of the order references a proforma szamlazz.hu no longer
    /// returns, the consumed proforma.
    pub proforma: Option<DocumentStatus>,
    /// The invoice.
    pub invoice: Option<DocumentStatus>,
    /// The prepayment invoice.
    pub prepayment: Option<DocumentStatus>,
    /// The final invoice.
    #[serde(rename = "final")]
    pub r#final: Option<DocumentStatus>,
}

impl OrderStatus {
    /// The status of the `kind` document.
    #[must_use]
    pub const fn get(&self, kind: super::DocumentKind) -> Option<&DocumentStatus> {
        match kind {
            super::DocumentKind::Proforma => self.proforma.as_ref(),
            super::DocumentKind::Invoice => self.invoice.as_ref(),
            super::DocumentKind::Prepayment => self.prepayment.as_ref(),
            super::DocumentKind::Final => self.r#final.as_ref(),
        }
    }

    /// Sets the status of the `kind` document.
    pub fn set(&mut self, kind: super::DocumentKind, status: Option<DocumentStatus>) {
        match kind {
            super::DocumentKind::Proforma => self.proforma = status,
            super::DocumentKind::Invoice => self.invoice = status,
            super::DocumentKind::Prepayment => self.prepayment = status,
            super::DocumentKind::Final => self.r#final = status,
        }
    }
}

/// The live view of one document of an order, as szamlazz.hu reports it.
///
/// The state is flattened: `{"number": "SZ-1", "state": "live", …}`,
/// `{"state": "reversed", "storno_number": "SS-1", …}` or
/// `{"state": "consumed", "by": "SZ-1", …}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct DocumentStatus {
    /// The document number.
    pub number: String,
    /// Whether it is live, reversed or (proformas) consumed.
    #[serde(flatten)]
    pub state: DocumentState,
    /// Gross total.
    #[serde(default)]
    pub gross: Option<Decimal>,
    /// Net total.
    #[serde(default)]
    pub net: Option<Decimal>,
    /// Registered credit entry amounts, in the order szamlazz.hu lists them.
    #[serde(default)]
    pub payments: Vec<Decimal>,
    /// The proforma this document converted (`hivdijbekszam`).
    #[serde(default)]
    pub referenced_proforma: Option<String>,
    /// Whether it is an e-invoice; `None` for proformas and unknown codes.
    #[serde(default)]
    pub e_invoice: Option<bool>,
}

impl DocumentStatus {
    /// A status of `number` in `state` with no totals, payments or
    /// references.
    pub fn new(number: impl Into<String>, state: DocumentState) -> Self {
        Self {
            number: number.into(),
            state,
            gross: None,
            net: None,
            payments: Vec::new(),
            referenced_proforma: None,
            e_invoice: None,
        }
    }
}

/// The state of a document as szamlazz.hu reports it.
///
/// Tagged by `state`: `live`, `reversed` (with `storno_number` when known) or
/// `consumed` (with the consuming document in `by`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "state", rename_all = "snake_case")]
#[non_exhaustive]
pub enum DocumentState {
    /// The document exists and is not reversed.
    Live,
    /// The document carries `<sztornozott>true</sztornozott>`.
    Reversed {
        /// The storno invoice number, when known.
        #[serde(default)]
        storno_number: Option<String>,
    },
    /// A proforma szamlazz.hu no longer returns because the document `by`
    /// converted it.
    Consumed {
        /// The invoice or prepayment that references the proforma.
        by: String,
    },
}

/// Output of `Szamlazz.Agent.check_account`: what the deploy pipeline needs
/// to prove, per scope, that the scope reaches the worker, resolves to the
/// intended account and its credentials work — without issuing anything.
///
/// Credential acceptance is the only szamlazz.hu-verified fact here; the
/// account fields echo the *configured* account (the supplier id appears only
/// in found-document bodies, so a not-found probe cannot cross-check it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct CheckAccountResponse {
    /// The scope the SDK saw; `null` for an unscoped request.
    #[serde(default)]
    pub scope: Option<String>,
    /// The configured account the request resolved to.
    pub account: CheckedAccount,
    /// The deployment's namespace (the external-id prefix), as pinned.
    pub namespace: String,
    /// Whether szamlazz.hu accepted the account's credentials.
    pub credentials: CredentialsCheck,
}

impl CheckAccountResponse {
    /// A response for `account` under `scope` in `namespace`.
    pub fn new(
        scope: Option<String>,
        account: CheckedAccount,
        namespace: impl Into<String>,
        credentials: CredentialsCheck,
    ) -> Self {
        Self {
            scope,
            account,
            namespace: namespace.into(),
            credentials,
        }
    }
}

/// The configured identity of the account `check_account` resolved to: the
/// pins the worker validates found documents against, never the agent key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct CheckedAccount {
    /// The account's id as the resolver knows it.
    pub id: String,
    /// `live` or `test`.
    pub mode: AccountMode,
    /// The configured supplier id pin, when set.
    #[serde(default)]
    pub supplier_id: Option<u64>,
}

impl CheckedAccount {
    /// The identity `id` in `mode`, pinned to `supplier_id` when set.
    pub fn new(id: impl Into<String>, mode: AccountMode, supplier_id: Option<u64>) -> Self {
        Self {
            id: id.into(),
            mode,
            supplier_id,
        }
    }
}

impl From<&Account> for CheckedAccount {
    fn from(account: &Account) -> Self {
        Self::new(account.id.to_string(), account.mode, account.supplier_id)
    }
}

/// Whether szamlazz.hu accepted the account's credentials on the probe query.
///
/// Tagged by `state`: `ok`, or `rejected` with the szamlazz.hu code (3, 135,
/// 136 or 164) and message. Data, not a fault: the probe's purpose is to
/// report it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "state", rename_all = "snake_case")]
#[non_exhaustive]
pub enum CredentialsCheck {
    /// szamlazz.hu answered the query: the agent key works.
    Ok,
    /// szamlazz.hu refused the agent key.
    Rejected {
        /// The szamlazz.hu code.
        code: String,
        /// The szamlazz.hu message.
        message: String,
    },
}

/// `gross − Σ payments`, when the gross total is known.
pub(crate) fn outstanding(gross: Option<Decimal>, payments: &[Decimal]) -> Option<Decimal> {
    gross.map(|gross| gross - payments.iter().copied().sum::<Decimal>())
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use rust_decimal::dec;
    use serde_json::json;
    use szamlazz_agent::InvoiceNumber;
    use szamlazz_agent::ops::query_pdf::InvoiceSelector;
    use szamlazz_agent::ops::query_xml::QueryInvoiceXml;
    use szamlazz_agent::wire::{AgentRequest as _, RawResponse};

    use super::*;

    fn round_trip<T>(value: &T) -> serde_json::Value
    where
        T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_value(value).expect("serialize");
        let back: T = serde_json::from_value(json.clone()).expect("deserialize");
        assert_eq!(&back, value);
        json
    }

    #[test]
    fn create_response_round_trips() {
        let response =
            CreateResponse::new(Outcome::Issued, IssuedKind::Invoice, "acct:ORD-1:invoice")
                .with_invoice_number("SZ-1")
                .with_net_total(dec!(20000))
                .with_gross_total(dec!(25400))
                .with_outstanding(dec!(25400))
                .with_customer_account_url("https://example.test/acct")
                .with_warning(Warning::NotificationDeliveryFailed);
        let json = round_trip(&response);
        assert_eq!(json["outcome"], "issued");
        assert_eq!(json["conflict_reason"], serde_json::Value::Null);
        assert_eq!(json["kind"], "invoice");
        assert_eq!(json.get("gen"), None);
        assert_eq!(json.get("request_id"), None);
        assert_eq!(json["warnings"], json!(["notification_delivery_failed"]));
    }

    #[test]
    fn create_response_conflict_and_rejection() {
        let conflict = CreateResponse::conflict(
            ConflictReason::Live,
            IssuedKind::Invoice,
            "acct:ORD-1:invoice",
        )
        .with_existing_number("SZ-1");
        let json = round_trip(&conflict);
        assert_eq!(json["outcome"], "conflict");
        assert_eq!(json["conflict_reason"], "live");
        assert_eq!(json["existing_number"], "SZ-1");

        let rejected = CreateResponse::new(
            Outcome::Rejected,
            IssuedKind::Corrective,
            "acct:ORD-1:corrective:c-2",
        )
        .with_code("259")
        .with_message("net value mismatch");
        let json = round_trip(&rejected);
        assert_eq!(json["code"], "259");
    }

    #[test]
    fn create_response_defaults_optional_fields() {
        let response: CreateResponse = serde_json::from_value(json!({
            "outcome": "reversed",
            "kind": "final",
            "external_id": "acct:ORD-1:final",
        }))
        .expect("deserialize");
        assert_eq!(response.outcome, Outcome::Reversed);
        assert!(response.warnings.is_empty());
        assert_eq!(response.invoice_number, None);
    }

    #[test]
    fn every_conflict_reason_is_snake_case() {
        let reasons = [
            (ConflictReason::PrepaidChain, "prepaid_chain"),
            (ConflictReason::Live, "live"),
            (ConflictReason::Foreign, "foreign"),
            (
                ConflictReason::DuplicateOrderNumber,
                "duplicate_order_number",
            ),
            (ConflictReason::ExternalIdCollision, "external_id_collision"),
            (ConflictReason::ProformaLive, "proforma_live"),
            (ConflictReason::ProformaMissing, "proforma_missing"),
            (ConflictReason::PrepaymentMissing, "prepayment_missing"),
            (ConflictReason::PrepaymentReversed, "prepayment_reversed"),
            (ConflictReason::BaseReversed, "base_reversed"),
            (ConflictReason::NotManaged, "not_managed"),
        ];
        for (reason, token) in reasons {
            assert_eq!(
                serde_json::to_value(reason).expect("serialize"),
                json!(token)
            );
        }
    }

    #[test]
    fn storno_response_round_trips() {
        let reversed =
            StornoResponse::new(StornoOutcome::Reversed, "SZ-1").with_storno_number("SS-1");
        let json = round_trip(&reversed);
        assert_eq!(json["outcome"], "reversed");
        assert_eq!(json["storno_number"], "SS-1");

        let managed =
            StornoResponse::new(StornoOutcome::ManagedByOrder, "SZ-2").with_order_key("ORD-2");
        let json = round_trip(&managed);
        assert_eq!(json["outcome"], "managed_by_order");
        assert_eq!(json["order_key"], "ORD-2");

        let conflict = StornoResponse::new(StornoOutcome::Conflict, "SZ-3")
            .with_conflict_reason(ConflictReason::NotManaged);
        let json = round_trip(&conflict);
        assert_eq!(json["conflict_reason"], "not_managed");
        let rejected = StornoResponse::new(StornoOutcome::Rejected, "SZ-4")
            .with_code("221")
            .with_message("has corrective");
        round_trip(&rejected);
    }

    #[test]
    fn small_responses_round_trip() {
        let json = round_trip(&DeleteProformaResponse::deleted());
        assert_eq!(json, json!({"deleted": true, "reason": null}));
        let json = round_trip(&DeleteProformaResponse::absent());
        assert_eq!(json, json!({"deleted": true, "reason": "absent"}));
        round_trip(&DeleteProformaResponse::not_deleted("proforma_paid"));

        let mut payments = SetPaymentsResponse::new("SZ-1");
        payments.outstanding = Some(dec!(0));
        payments.gross_total = Some(dec!(25400));
        round_trip(&payments);
    }

    #[test]
    fn query_response_round_trips() {
        let mut response = QueryResponse::new("SZ-1", "SZ");
        response.reversed = Some(false);
        response.referenced_proforma_number = Some("D-1".to_owned());
        response.order_number = Some("ORD-1".to_owned());
        response.issue_date = Some(date(2026, 7, 4));
        response.fulfillment_date = Some(date(2026, 7, 4));
        response.due_date = Some(date(2026, 7, 12));
        response.currency = Some("HUF".to_owned());
        response.net_total = Some(dec!(20000));
        response.vat_total = Some(dec!(5400));
        response.gross_total = Some(dec!(25400));
        let mut payment = PaymentRecord::new(dec!(10000));
        payment.date = Some(date(2026, 7, 10));
        payment.title = Some("átutalás".to_owned());
        response.payments = vec![payment];
        response.outstanding = Some(dec!(15400));
        response.supplier_id = Some(972_720);
        response.test = true;
        let json = round_trip(&response);
        assert_eq!(json["document_type"], "SZ");
        assert_eq!(json["supplier_id"], 972_720);
        assert_eq!(json["payments"][0]["amount"], "10000");

        let minimal: QueryResponse =
            serde_json::from_value(json!({"invoice_number": "D-1", "document_type": "D"}))
                .expect("deserialize");
        assert_eq!(minimal, QueryResponse::new("D-1", "D"));
    }

    /// A queried document, as the `szamla` response XML.
    fn queried_document(payments: &str) -> InvoiceDocument {
        let body = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<szamla xmlns="http://www.szamlazz.hu/szamla">
  <szallito><id>972720</id><nev>Seller</nev><cim><irsz>1111</irsz><telepules>Budapest</telepules><cim>Fő u. 1.</cim></cim></szallito>
  <alap><id>924307338</id><szamlaszam>SZ-1</szamlaszam><tipus>SZ</tipus><eszamla>2</eszamla><hivszamlaszam>ES-1</hivszamlaszam><hivdijbekszam>D-1</hivdijbekszam><kelt>2026-07-04</kelt><telj>2026-07-04</telj><fizh>2026-07-12</fizh><rendelesszam>ORD-1</rendelesszam><devizanem>HUF</devizanem><teszt>true</teszt></alap>
  <vevo><nev>Buyer</nev><email>buyer@example.com</email></vevo>
  <tetelek></tetelek>
  <osszegek><totalossz><netto>20000</netto><afa>5400</afa><brutto>25400</brutto></totalossz></osszegek>
  {payments}
</szamla>"#
        );
        QueryInvoiceXml::new(InvoiceSelector::InvoiceNumber(InvoiceNumber::new("SZ-1")))
            .parse(&RawResponse::new::<&str, &str>([], body.into_bytes()))
            .expect("parse")
    }

    #[test]
    fn query_response_projects_a_queried_document() {
        let document = queried_document(
            "<kifizetesek>\
             <kifizetes><datum>2026-07-10</datum><jogcim>átutalás</jogcim><osszeg>10000</osszeg><megjegyzes>first</megjegyzes><bankszamlaszam>1234-5678</bankszamlaszam></kifizetes>\
             <kifizetes><datum>2026-07-11</datum><jogcim>bankkártya</jogcim><osszeg>5000</osszeg></kifizetes>\
             </kifizetesek>",
        );
        let response = QueryResponse::from(&document);

        let mut expected = QueryResponse::new("SZ-1", "SZ");
        expected.reversed = None;
        expected.referenced_invoice_number = Some("ES-1".to_owned());
        expected.referenced_proforma_number = Some("D-1".to_owned());
        expected.order_number = Some("ORD-1".to_owned());
        expected.issue_date = Some(date(2026, 7, 4));
        expected.fulfillment_date = Some(date(2026, 7, 4));
        expected.due_date = Some(date(2026, 7, 12));
        expected.currency = Some("HUF".to_owned());
        expected.net_total = Some(dec!(20000));
        expected.vat_total = Some(dec!(5400));
        expected.gross_total = Some(dec!(25400));
        let mut first = PaymentRecord::new(dec!(10000));
        first.date = Some(date(2026, 7, 10));
        first.title = Some("átutalás".to_owned());
        first.comment = Some("first".to_owned());
        first.bank_account = Some("1234-5678".to_owned());
        let mut second = PaymentRecord::new(dec!(5000));
        second.date = Some(date(2026, 7, 11));
        second.title = Some("bankkártya".to_owned());
        expected.payments = vec![first, second];
        expected.outstanding = Some(dec!(10400));
        expected.supplier_id = Some(972_720);
        expected.test = true;
        assert_eq!(response, expected);
        assert_eq!(
            PaymentRecord::from(&document.payments[0]),
            expected.payments[0]
        );
    }

    #[test]
    fn query_response_without_payments_owes_the_gross_total() {
        let response = QueryResponse::from(&queried_document(""));
        assert!(response.payments.is_empty());
        assert_eq!(response.outstanding, Some(dec!(25400)));
    }

    #[test]
    fn outstanding_needs_a_gross_total() {
        assert_eq!(outstanding(None, &[dec!(1)]), None);
        assert_eq!(outstanding(Some(dec!(100)), &[]), Some(dec!(100)));
        assert_eq!(
            outstanding(Some(dec!(100)), &[dec!(30), dec!(80)]),
            Some(dec!(-10))
        );
    }

    #[test]
    fn order_status_round_trips_with_flattened_states() {
        let mut status = OrderStatus::default();
        let mut invoice = DocumentStatus::new("SZ-2", DocumentState::Live);
        invoice.gross = Some(dec!(25400));
        invoice.net = Some(dec!(20000));
        invoice.payments = vec![dec!(10000)];
        invoice.referenced_proforma = Some("D-1".to_owned());
        invoice.e_invoice = Some(true);
        status.set(super::super::DocumentKind::Invoice, Some(invoice));
        status.proforma = Some(DocumentStatus::new(
            "D-1",
            DocumentState::Consumed {
                by: "SZ-2".to_owned(),
            },
        ));
        status.r#final = Some(DocumentStatus::new(
            "VS-1",
            DocumentState::Reversed {
                storno_number: Some("SS-9".to_owned()),
            },
        ));

        let json = round_trip(&status);
        assert_eq!(json["invoice"]["number"], "SZ-2");
        assert_eq!(json["invoice"]["state"], "live");
        assert_eq!(json["invoice"]["payments"], json!(["10000"]));
        assert_eq!(json["proforma"]["state"], "consumed");
        assert_eq!(json["proforma"]["by"], "SZ-2");
        assert_eq!(json["final"]["state"], "reversed");
        assert_eq!(json["final"]["storno_number"], "SS-9");
        assert_eq!(json["prepayment"], serde_json::Value::Null);
        assert_eq!(
            status.get(super::super::DocumentKind::Final),
            status.r#final.as_ref()
        );

        let empty: OrderStatus = serde_json::from_value(json!({})).expect("deserialize");
        assert_eq!(empty, OrderStatus::default());
        let bare: DocumentStatus =
            serde_json::from_value(json!({"number": "SZ-1", "state": "reversed"}))
                .expect("deserialize");
        assert_eq!(
            bare.state,
            DocumentState::Reversed {
                storno_number: None
            }
        );
    }

    /// `{scope, account: {id, mode, supplier_id}, namespace, credentials}`,
    /// the credentials tagged by `state`: `ok`, or `rejected` with szamlazz.hu's
    /// code and message.
    #[test]
    fn check_account_response_round_trips() {
        let mut account = Account::new("acme", "acme");
        account.mode = AccountMode::Test;
        account.supplier_id = Some(972_720);
        let mut response = CheckAccountResponse::new(
            Some("acme-events".to_owned()),
            CheckedAccount::from(&account),
            "acct",
            CredentialsCheck::Ok,
        );
        let json = round_trip(&response);
        assert_eq!(
            json,
            json!({
                "scope": "acme-events",
                "account": { "id": "acme", "mode": "test", "supplier_id": 972_720 },
                "namespace": "acct",
                "credentials": { "state": "ok" },
            })
        );

        response.scope = None;
        response.account.supplier_id = None;
        response.account.mode = AccountMode::Live;
        response.credentials = CredentialsCheck::Rejected {
            code: "3".to_owned(),
            message: "Sikertelen bejelentkezés.".to_owned(),
        };
        let json = round_trip(&response);
        assert_eq!(json["scope"], serde_json::Value::Null);
        assert_eq!(json["account"]["mode"], "live");
        assert_eq!(json["account"]["supplier_id"], serde_json::Value::Null);
        assert_eq!(
            json["credentials"],
            json!({ "state": "rejected", "code": "3", "message": "Sikertelen bejelentkezés." })
        );
    }
}
