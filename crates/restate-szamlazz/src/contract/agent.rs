//! Contract of the `Szamlazz.Agent` handlers that are not a storno:
//! `query`, `query_taxpayer`, `set_credit_entries` and `check_account`.
//!
//! `Szamlazz.Agent.storno` shares the storno contract with
//! `Szamlazz.Order.storno_invoice`; see [`storno`](super::storno).

use jiff::civil::Date;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use szamlazz_agent::ops::credit_entry::CreditEntry;
use szamlazz_agent::ops::taxpayer::{
    TaxpayerAddress as AgentTaxpayerAddress, TaxpayerInfo, TaxpayerPrefix,
};

use super::document::PaymentMethod;
use super::{InvoiceNumber, outstanding};
use crate::account::Account;
use crate::gateway::{FoundDocument, RecordedCreditEntry};

/// Input of `Szamlazz.Agent.query`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct QueryRequest {
    /// Which document to look up.
    pub selector: Selector,
}

impl QueryRequest {
    /// A query by `selector`.
    #[must_use]
    pub const fn new(selector: Selector) -> Self {
        Self { selector }
    }
}

/// A document selector for the query operation.
///
/// Serialises as `{"invoice_number": "…"}`, `{"order_number": "…"}` or
/// `{"external_id": "…"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Selector {
    /// By invoice number (`számlaszám`).
    InvoiceNumber(InvoiceNumber),
    /// By order number (`rendelésszám`); returns the last document issued
    /// under it.
    OrderNumber(String),
    /// By external id (`szamlaKulsoAzon`); not unique server-side, the last
    /// writer wins.
    ExternalId(String),
}

/// One registered credit entry as szamlazz.hu reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct CreditEntryRecord {
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
    /// Bank account the credit entry arrived on.
    #[serde(default)]
    pub bank_account: Option<String>,
}

impl CreditEntryRecord {
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

impl From<&RecordedCreditEntry> for CreditEntryRecord {
    fn from(entry: &RecordedCreditEntry) -> Self {
        let mut record = Self::new(entry.amount);
        record.date = Some(entry.date);
        record.title = Some(entry.title.clone());
        record.comment.clone_from(&entry.comment);
        record.bank_account.clone_from(&entry.bank_account);
        record
    }
}

/// Output of `Szamlazz.Agent.query`: a projection of the queried document,
/// read off the `FoundDocument` the handler's one step journaled.
/// Deliberately omits the seller block. The deploy-side go-live check is
/// `examples/verify_seller.rs`, using the actual deployed `Accounts` bundle
/// and a fresh Számla Agent client outside Restate's journal.
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
    pub credit_entries: Vec<CreditEntryRecord>,
    /// Outstanding amount: gross total minus the sum of credit entries.
    #[serde(default)]
    pub outstanding: Option<Decimal>,
    /// Issued from a test account (`teszt`), as szamlazz.hu reported it:
    /// compared with nothing by the worker. This projection omits the seller
    /// block, so the go-live check uses a direct Számla Agent query through
    /// the deployed resolver/store instead. `None` (`null`) is a document that does not
    /// say: the schema has the element mandatory, so it is szamlazz.hu
    /// breaking its schema, never a live document; a reader deciding "is this
    /// scope live?" must not read it as `false`.
    #[serde(default)]
    pub test: Option<bool>,
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
            credit_entries: Vec::new(),
            outstanding: None,
            test: None,
        }
    }
}

/// The projection of a found document: identity, references, dates, totals
/// and credit entries; no buyer or seller data (the journaled document carries neither).
/// `outstanding` is `gross − Σ credit entries`; `test` is `teszt` exactly as
/// reported, `None` included.
impl From<&FoundDocument> for QueryResponse {
    fn from(document: &FoundDocument) -> Self {
        let mut response = Self::new(&document.number, document.document_type.as_wire());
        response.reversed = document.reversed;
        response
            .referenced_invoice_number
            .clone_from(&document.referenced_invoice_number);
        response
            .referenced_proforma_number
            .clone_from(&document.referenced_proforma_number);
        response.order_number.clone_from(&document.order_number);
        response.issue_date = document.issue_date;
        response.fulfillment_date = document.fulfillment_date;
        response.due_date = document.due_date;
        response.currency.clone_from(&document.currency);
        response.net_total = Some(document.net_total);
        response.vat_total = Some(document.vat_total);
        response.gross_total = Some(document.gross_total);
        response.credit_entries = document
            .credit_entries
            .iter()
            .map(CreditEntryRecord::from)
            .collect();
        response.outstanding = outstanding(response.gross_total, &document.credit_entry_amounts());
        response.test = document.test;
        response
    }
}

/// Input of `Szamlazz.Agent.query_taxpayer`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct QueryTaxpayerRequest {
    /// The Hungarian tax number to look up: the bare eight-digit stem
    /// (`12345678`) or the full `NNNNNNNN-N-NN` form (`12345678-2-42`).
    /// Nothing else (no whitespace, no other separator) is accepted; the
    /// handler refuses anything else as `invalid_input`.
    pub tax_number: String,
}

impl QueryTaxpayerRequest {
    /// A request for `tax_number`, in either accepted form.
    #[must_use]
    pub fn new(tax_number: impl Into<String>) -> Self {
        Self {
            tax_number: tax_number.into(),
        }
    }

    /// The eight-digit prefix (törzsszám) NAV is asked about, derived from
    /// either accepted form of `tax_number`.
    ///
    /// # Errors
    ///
    /// [`InvalidTaxNumber`] when `tax_number` is neither the bare eight-digit
    /// stem nor the full `NNNNNNNN-N-NN` form; the message names the input
    /// and the accepted forms.
    pub fn prefix(&self) -> Result<TaxpayerPrefix, InvalidTaxNumber> {
        let invalid = || InvalidTaxNumber(self.tax_number.clone());
        let digits = |value: &str, len: usize| {
            value.len() == len && value.bytes().all(|byte| byte.is_ascii_digit())
        };
        // The stem is whatever precedes the first `-`; `TaxpayerPrefix` is
        // the one judge of the stem (eight ASCII digits). The rest, when
        // present, must be exactly `-N-NN`: the VAT code and the area code.
        let (stem, suffix) = match self.tax_number.split_once('-') {
            None => (self.tax_number.as_str(), None),
            Some((stem, rest)) => (stem, Some(rest)),
        };
        if let Some(rest) = suffix
            && !rest
                .split_once('-')
                .is_some_and(|(vat, area)| digits(vat, 1) && digits(area, 2))
        {
            return Err(invalid());
        }
        stem.parse().map_err(|_| invalid())
    }
}

/// A tax number that is neither the eight-digit stem nor the full
/// `NNNNNNNN-N-NN` form.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "invalid tax number {0:?}: expected the eight-digit stem (12345678) or the full Hungarian tax number (12345678-2-42)"
)]
pub struct InvalidTaxNumber(String);

/// Output of `Szamlazz.Agent.query_taxpayer`: a taxpayer as NAV registers
/// it, looked up through szamlazz.hu's `xmltaxpayer` operation.
///
/// `valid: false` is a normal answer (NAV knows no taxpayer under the
/// prefix), not a fault; the optional fields are then absent.
///
/// A crate-owned projection of the Számla Agent crate's `TaxpayerInfo`, not
/// the agent type as it is: it is what the handler's read step journals, so
/// what an entry holds is decided here and the agent crate's serde layout
/// never rides in the journal (ADR 0009: crate-owned, under no compatibility
/// rule). Every field but `valid` is optional and defaults, so NAV's
/// `valid: false` and its simple addresses decode as they come. Not cached by
/// the worker (szamlazz.hu is the source
/// of truth); a caller that looks a buyer up repeatedly caches this response
/// itself, with a TTL on the order of a day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct QueryTaxpayerResponse {
    /// Whether NAV says the prefix belongs to a valid taxpayer
    /// (`taxpayerValidity`).
    pub valid: bool,
    /// The registered name (`taxpayerName`), when valid.
    #[serde(default)]
    pub name: Option<String>,
    /// The eight-digit tax number stem (`taxpayerId`), when provided.
    #[serde(default)]
    pub tax_number: Option<String>,
    /// The VAT code digit (`vatCode`), when provided.
    #[serde(default)]
    pub vat_code: Option<String>,
    /// The registered addresses (`taxpayerAddressItem` entries).
    #[serde(default)]
    pub addresses: Vec<TaxpayerAddress>,
}

impl From<TaxpayerInfo> for QueryTaxpayerResponse {
    fn from(info: TaxpayerInfo) -> Self {
        Self {
            valid: info.valid,
            name: info.name,
            tax_number: info.tax_number,
            vat_code: info.vat_code,
            addresses: info.addresses.into_iter().map(Into::into).collect(),
        }
    }
}

/// A registered address of a taxpayer (`taxpayerAddressItem`), as NAV
/// structures it. Every field is optional: NAV's detailed addresses fill the
/// structured fields, its simple addresses only `additional_address_detail`.
///
/// Crate-owned and journaled, like `QueryTaxpayerResponse`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(default)]
#[non_exhaustive]
pub struct TaxpayerAddress {
    /// The address type (`taxpayerAddressType`), e.g. `HQ` or `SITE`.
    pub kind: Option<String>,
    /// Country code (`countryCode`).
    pub country_code: Option<String>,
    /// Region (`region`).
    pub region: Option<String>,
    /// Postal code (`postalCode`).
    pub postal_code: Option<String>,
    /// City (`city`).
    pub city: Option<String>,
    /// Street name (`streetName`).
    pub street_name: Option<String>,
    /// The public place category (`publicPlaceCategory`), e.g. `UTCA`.
    pub public_place_category: Option<String>,
    /// House number (`number`).
    pub number: Option<String>,
    /// Building (`building`).
    pub building: Option<String>,
    /// Staircase (`staircase`).
    pub staircase: Option<String>,
    /// Floor (`floor`).
    pub floor: Option<String>,
    /// Door (`door`).
    pub door: Option<String>,
    /// Lot number (`lotNumber`).
    pub lot_number: Option<String>,
    /// The free-form detail of a simple address
    /// (`additionalAddressDetail`).
    pub additional_address_detail: Option<String>,
}

impl From<AgentTaxpayerAddress> for TaxpayerAddress {
    fn from(address: AgentTaxpayerAddress) -> Self {
        Self {
            kind: address.kind,
            country_code: address.country_code,
            region: address.region,
            postal_code: address.postal_code,
            city: address.city,
            street_name: address.street_name,
            public_place_category: address.public_place_category,
            number: address.number,
            building: address.building,
            staircase: address.staircase,
            floor: address.floor,
            door: address.door,
            lot_number: address.lot_number,
            additional_address_detail: address.additional_address_detail,
        }
    }
}

/// Input of `Szamlazz.Agent.set_credit_entries`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct SetCreditEntriesRequest {
    /// The invoice to register credit entries on.
    pub invoice_number: InvoiceNumber,
    /// The credit entries (`jóváírások`); szamlazz.hu accepts at most five,
    /// and a replacing request (`additive: false`) needs at least one: with
    /// none it would clear the invoice's credit entries, and is refused as
    /// `invalid_input` with nothing sent.
    pub entries: Vec<CreditEntryInput>,
    /// Add to the existing entries instead of replacing them.
    ///
    /// **At-least-once.** A replacement sends a full snapshot, so a stale
    /// retry can overwrite newer entries. Additive entries are appended by every send that
    /// reaches szamlazz.hu, and the handler cannot tell a lost reply from a
    /// lost request: an `outcome_unknown` fault, or the handler's one retry
    /// after a crash, may have landed the entries already. A caller that sees
    /// `outcome_unknown` on an additive call queries the invoice
    /// (`Szamlazz.Agent.query`) before sending only entries still missing,
    /// with a new `Idempotency-Key`. A replacing call likewise queries first
    /// and sends the current intended snapshot if replacement is still wanted.
    /// Cancellation of the one-shot write is the same structured
    /// `outcome_unknown`: it cannot prove that nothing landed.
    #[serde(default)]
    pub additive: bool,
}

impl SetCreditEntriesRequest {
    /// A replacing request: `entries` become the invoice's credit entries.
    /// Set [`additive`](Self::additive) to append instead.
    #[must_use]
    pub fn new(invoice_number: InvoiceNumber, entries: Vec<CreditEntryInput>) -> Self {
        Self {
            invoice_number,
            entries,
            additive: false,
        }
    }
}

/// One credit entry (`jóváírás`) as the caller sends it: the input side of
/// `CreditEntryRecord`, with the same words (`title`, `comment`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreditEntryInput {
    /// The date of the credit entry.
    pub date: Date,
    /// The entry's title (`jogcim`): the payment method it was settled by.
    pub title: PaymentMethod,
    /// Amount in the invoice currency.
    pub amount: Decimal,
    /// Free-text comment (`megjegyzes`).
    #[serde(default)]
    pub comment: Option<String>,
}

impl CreditEntryInput {
    /// An entry of `amount` settled by `title` on `date`, without a comment.
    #[must_use]
    pub const fn new(date: Date, title: PaymentMethod, amount: Decimal) -> Self {
        Self {
            date,
            title,
            amount,
            comment: None,
        }
    }
}

impl From<&CreditEntryInput> for CreditEntry {
    fn from(entry: &CreditEntryInput) -> Self {
        let mut credit = Self::new(entry.date, entry.title.clone().into(), entry.amount);
        credit.description.clone_from(&entry.comment);
        credit
    }
}

/// Output of `Szamlazz.Agent.set_credit_entries`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct SetCreditEntriesResponse {
    /// The invoice the entries were registered on.
    pub invoice_number: String,
    /// Outstanding amount after the update (`kintlévőség`).
    #[serde(default)]
    pub outstanding: Option<Decimal>,
    /// Gross total of the invoice.
    #[serde(default)]
    pub gross_total: Option<Decimal>,
}

impl SetCreditEntriesResponse {
    /// A response for `invoice_number` without totals.
    #[must_use]
    pub fn new(invoice_number: impl Into<String>) -> Self {
        Self {
            invoice_number: invoice_number.into(),
            outstanding: None,
            gross_total: None,
        }
    }
}

/// Output of `Szamlazz.Agent.check_account`: what the deploy pipeline needs
/// to prove, per scope, that the scope reaches the worker, resolves to the
/// configured account and its credentials work, without issuing anything.
///
/// Credential acceptance is the only szamlazz.hu-verified fact here; the
/// account field echoes the *configured* account's id. *Which* szamlazz.hu
/// account the key opens (and whether it is a test account) is not in the
/// answer and is checked nowhere in the worker: a not-found probe has no
/// document to read, and no operation answers "which account am I?". That is
/// the operator's go-live check: `examples/verify_seller.rs` uses the actual
/// deployed `Accounts` resolver/store, a fresh Számla Agent client per scope
/// and independent expectations for a known document's `test`, seller name
/// and tax number, outside Restate's journal. The Restate `query` deliberately
/// omits the seller block. Repeat the check after key rotation.
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
/// resolver's id, never the agent key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct CheckedAccount {
    /// The account's id as the resolver knows it.
    pub id: String,
}

impl CheckedAccount {
    /// The identity `id`.
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

impl From<&Account> for CheckedAccount {
    fn from(account: &Account) -> Self {
        Self::new(account.id.to_string())
    }
}

/// Whether szamlazz.hu accepted the account's credentials on the probe query.
///
/// Tagged by `state`: `ok`, or `rejected` with the szamlazz.hu code (3, 135,
/// 136 or 164) and message. Data, not a fault: the probe's purpose is to
/// report it. Unknown states preserve their token and payload fields without
/// inferring whether credentials were accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
    /// A newer state and its payload, preserved without interpreting it.
    #[serde(untagged)]
    Other {
        /// The unknown state token.
        state: String,
        /// The state's payload (excluding `state`).
        #[serde(flatten)]
        fields: serde_json::Map<String, serde_json::Value>,
    },
}

impl CredentialsCheck {
    /// Known state tokens; a rejection's payload is carried by its variant.
    pub const KNOWN: [&'static str; 2] = ["ok", "rejected"];

    /// The state token, including an unknown token verbatim.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Ok => "ok",
            Self::Rejected { .. } => "rejected",
            Self::Other { state, .. } => state,
        }
    }
}

impl<'de> Deserialize<'de> for CredentialsCheck {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            state: String,
            #[serde(flatten)]
            fields: serde_json::Map<String, serde_json::Value>,
        }
        let Wire { state, mut fields } = Wire::deserialize(deserializer)?;
        match state.as_str() {
            "ok" => Ok(Self::Ok),
            "rejected" => Ok(Self::Rejected {
                code: serde_json::from_value(
                    fields
                        .remove("code")
                        .ok_or_else(|| serde::de::Error::missing_field("code"))?,
                )
                .map_err(serde::de::Error::custom)?,
                message: serde_json::from_value(
                    fields
                        .remove("message")
                        .ok_or_else(|| serde::de::Error::missing_field("message"))?,
                )
                .map_err(serde::de::Error::custom)?,
            }),
            _ => Ok(Self::Other { state, fields }),
        }
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for CredentialsCheck {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "CredentialsCheck".into()
    }

    fn schema_id() -> std::borrow::Cow<'static, str> {
        concat!(module_path!(), "::CredentialsCheck").into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "object",
            "description": "Credential acceptance: ok, or rejected with required code and message strings. Unknown state strings and their payload fields are preserved without classifying credential acceptance.",
            "required": ["state"],
            "properties": { "state": { "type": "string" } },
            "if": { "properties": { "state": { "const": "rejected" } } },
            "then": {
                "required": ["code", "message"],
                "properties": {
                    "code": { "type": "string" },
                    "message": { "type": "string" }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use rust_decimal::dec;
    use serde_json::json;
    use szamlazz_agent::wire::{AgentRequest as _, RawResponse};

    use super::*;
    use crate::contract::document::tests::{refuses_unknown_field, round_trip};
    use crate::test_support::{CreditRecord, Doc};

    #[test]
    fn unknown_credentials_states_preserve_the_full_probe_response() {
        let wire = json!({
            "scope": "acme",
            "account": {"id": "account-1"},
            "namespace": "acct",
            "credentials": {
                "state": " future_state/é ",
                "code": {"new": "shape"},
                "details": [null, true, 17, "é"]
            }
        });
        let decoded: CheckAccountResponse =
            serde_json::from_value(wire.clone()).expect("open probe");
        let CredentialsCheck::Other { state, fields } = &decoded.credentials else {
            panic!(
                "unknown credentials must not be classified: {:?}",
                decoded.credentials
            );
        };
        assert_eq!(state, " future_state/é ");
        assert_eq!(decoded.credentials.as_str(), state);
        assert_eq!(fields["details"], wire["credentials"]["details"]);
        assert_eq!(serde_json::to_value(decoded).expect("json"), wire);
    }

    #[test]
    fn known_credentials_payloads_stay_validated() {
        for wire in [
            json!({"state": "rejected"}),
            json!({"state": "rejected", "code": "3"}),
            json!({"state": "rejected", "code": 3, "message": "no"}),
            json!({"state": "rejected", "code": "3", "message": null}),
            json!({"state": 17}),
            json!({}),
        ] {
            assert!(serde_json::from_value::<CredentialsCheck>(wire).is_err());
        }
        for state in [
            CredentialsCheck::Ok,
            CredentialsCheck::Rejected {
                code: "3".to_owned(),
                message: "no".to_owned(),
            },
        ] {
            assert!(CredentialsCheck::KNOWN.contains(&state.as_str()));
            round_trip(&state);
        }
    }

    #[test]
    fn query_request_selectors() {
        let cases = [
            (
                Selector::InvoiceNumber("SZ-1".parse().expect("valid number")),
                json!({"invoice_number": "SZ-1"}),
            ),
            (
                Selector::OrderNumber("ORD-1".to_owned()),
                json!({"order_number": "ORD-1"}),
            ),
            (
                Selector::ExternalId("acct:ORD-1:invoice".to_owned()),
                json!({"external_id": "acct:ORD-1:invoice"}),
            ),
        ];
        for (selector, expected) in cases {
            let request = QueryRequest { selector };
            let json = round_trip(&request);
            assert_eq!(json["selector"], expected);
        }
    }

    #[test]
    fn set_credit_entries_request_round_trips() {
        let request = SetCreditEntriesRequest {
            invoice_number: "SZ-1".parse().expect("valid number"),
            entries: vec![CreditEntryInput {
                date: date(2026, 7, 10),
                title: PaymentMethod::Card,
                amount: dec!(25400),
                comment: Some("card".to_owned()),
            }],
            additive: false,
        };
        let json = round_trip(&request);
        assert_eq!(json["entries"][0]["title"], "card");
        assert_eq!(json["entries"][0]["amount"], "25400");
        let minimal: SetCreditEntriesRequest = serde_json::from_value(json!({
            "invoice_number": "SZ-1",
            "entries": [{"date": "2026-07-10", "title": "cash", "amount": 100}],
        }))
        .expect("deserialize");
        assert!(!minimal.additive);
        assert_eq!(minimal.entries[0].amount, dec!(100));
    }

    /// A misspelt field is refused, never a silent default: `aditive` as
    /// `additive: false` would *replace* the invoice's credit entries.
    #[test]
    fn request_types_refuse_unknown_fields() {
        let entry = json!({"date": "2026-07-10", "title": "cash", "amount": 100});

        refuses_unknown_field::<QueryRequest>(
            json!({"selector": {"invoice_number": "SZ-1"}, "invoice_number": "SZ-1"}),
            "invoice_number",
        );

        refuses_unknown_field::<QueryTaxpayerRequest>(
            json!({"tax_number": "12345678", "tax_numer": "12345678"}),
            "tax_numer",
        );

        refuses_unknown_field::<SetCreditEntriesRequest>(
            json!({"invoice_number": "SZ-1", "entries": [entry], "aditive": true}),
            "aditive",
        );
        refuses_unknown_field::<SetCreditEntriesRequest>(
            json!({
                "invoice_number": "SZ-1",
                "entries": [{"date": "2026-07-10", "title": "cash", "amount": 100, "note": "x"}],
            }),
            "note",
        );
        refuses_unknown_field::<CreditEntryInput>(
            json!({"date": "2026-07-10", "title": "cash", "amount": 100, "commnet": "x"}),
            "commnet",
        );
    }

    /// The externally tagged enum is closed already: a second key beside the
    /// variant is refused by serde itself.
    #[test]
    fn selector_refuses_a_second_key() {
        let error = serde_json::from_value::<QueryRequest>(json!({
            "selector": {"invoice_number": "SZ-1", "order_number": "ORD-1"},
        }))
        .expect_err("two selectors at once are refused");
        assert!(error.to_string().contains("single key"), "{error}");
    }

    /// The bodies the READMEs and the e2e scenarios send still deserialize:
    /// nothing documented carries a field the contract does not know.
    #[test]
    fn documented_bodies_deserialize() {
        // tests/e2e/: the literal bodies of the e2e scenarios.
        serde_json::from_value::<QueryRequest>(json!({"selector": {"invoice_number": "SZ-12"}}))
            .expect("a query body");
        // The crate README's `QueryTaxpayerRequest` forms and the e2e
        // taxpayer scenario: the full tax number and the bare stem.
        for tax_number in ["12345678-2-42", "12345678"] {
            let request =
                serde_json::from_value::<QueryTaxpayerRequest>(json!({"tax_number": tax_number}))
                    .expect("a taxpayer body");
            assert_eq!(request.prefix().expect("prefix").as_str(), "12345678");
        }
    }

    /// The bare eight-digit stem and the full `NNNNNNNN-N-NN` tax number
    /// derive the same prefix: the caller sends whatever it has.
    #[test]
    fn query_taxpayer_request_derives_the_prefix_from_either_form() {
        for tax_number in ["12345678", "12345678-2-42"] {
            let request = QueryTaxpayerRequest::new(tax_number);
            let prefix = request.prefix().expect(tax_number);
            assert_eq!(prefix.as_str(), "12345678", "{tax_number}");
            let json = round_trip(&request);
            assert_eq!(json, json!({"tax_number": tax_number}));
        }
    }

    /// Exactly two forms are accepted; the handler is not lenient, or an
    /// agent implementing a caller would invent its own leniency. Every
    /// refusal names the input and the accepted forms.
    #[test]
    fn query_taxpayer_request_refuses_every_other_form() {
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
            let error = QueryTaxpayerRequest::new(tax_number)
                .prefix()
                .expect_err(tax_number);
            let message = error.to_string();
            assert!(
                message.contains(&format!("{tax_number:?}")),
                "{tax_number:?}: {message}"
            );
            assert!(
                message.contains("12345678-2-42"),
                "{tax_number:?} names the accepted forms: {message}"
            );
        }
    }

    #[test]
    fn credit_entry_input_converts_to_agent() {
        let entry = CreditEntryInput {
            date: date(2026, 7, 10),
            title: PaymentMethod::Card,
            amount: dec!(25400),
            comment: Some("card".to_owned()),
        };
        let credit = CreditEntry::from(&entry);
        assert_eq!(credit.date, date(2026, 7, 10));
        assert_eq!(credit.title, szamlazz_agent::PaymentMethod::Card);
        assert_eq!(credit.amount, dec!(25400));
        assert_eq!(credit.description.as_deref(), Some("card"));

        let bare = CreditEntryInput {
            title: PaymentMethod::Other("Bitcoin".to_owned()),
            comment: None,
            ..entry
        };
        let credit = CreditEntry::from(&bare);
        assert_eq!(credit.title.as_wire(), "Bitcoin");
        assert_eq!(credit.description, None);
    }

    #[test]
    fn set_credit_entries_response_round_trips() {
        let mut credit_entries = SetCreditEntriesResponse::new("SZ-1");
        credit_entries.outstanding = Some(dec!(0));
        credit_entries.gross_total = Some(dec!(25400));
        round_trip(&credit_entries);
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
        let mut entry = CreditEntryRecord::new(dec!(10000));
        entry.date = Some(date(2026, 7, 10));
        entry.title = Some("átutalás".to_owned());
        response.credit_entries = vec![entry];
        response.outstanding = Some(dec!(15400));
        response.test = Some(true);
        let json = round_trip(&response);
        assert_eq!(json["document_type"], "SZ");
        assert_eq!(json["test"], true);
        assert!(
            json.get("supplier_id").is_none(),
            "the seller block is not projected: {json}"
        );
        assert_eq!(json["credit_entries"][0]["amount"], "10000");

        let minimal: QueryResponse =
            serde_json::from_value(json!({"invoice_number": "D-1", "document_type": "D"}))
                .expect("deserialize");
        assert_eq!(minimal, QueryResponse::new("D-1", "D"));
    }

    /// A queried `SZ-1` of `ORD-1` that consumed proforma `D-1` and settles
    /// prepayment `ES-1`, with a due date and a currency, owing 25400 before
    /// its two credit entries.
    #[test]
    fn query_response_projects_a_queried_document() {
        let document = Doc {
            referenced_invoice: Some("ES-1"),
            referenced_proforma: Some("D-1"),
            issue_date: Some(date(2026, 7, 4)),
            fulfillment_date: Some(date(2026, 7, 4)),
            net: "20000",
            vat: "5400",
            gross: "25400",
            credit_entries: &[
                CreditRecord {
                    comment: Some("first"),
                    bank_account: Some("1234-5678"),
                    ..CreditRecord::new(date(2026, 7, 10), "átutalás", "10000")
                },
                CreditRecord::new(date(2026, 7, 11), "bankkártya", "5000"),
            ],
            alap_extra: "<fizh>2026-07-12</fizh><devizanem>HUF</devizanem>",
            ..Doc::default()
        }
        .parse();
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
        let mut first = CreditEntryRecord::new(dec!(10000));
        first.date = Some(date(2026, 7, 10));
        first.title = Some("átutalás".to_owned());
        first.comment = Some("first".to_owned());
        first.bank_account = Some("1234-5678".to_owned());
        let mut second = CreditEntryRecord::new(dec!(5000));
        second.date = Some(date(2026, 7, 11));
        second.title = Some("bankkártya".to_owned());
        expected.credit_entries = vec![first, second];
        expected.outstanding = Some(dec!(10400));
        expected.test = Some(true);
        assert_eq!(response, expected);
        assert_eq!(
            CreditEntryRecord::from(&document.credit_entries[0]),
            expected.credit_entries[0]
        );
    }

    #[test]
    fn query_response_without_credit_entries_owes_the_gross_total() {
        let document = Doc {
            net: "20000",
            vat: "5400",
            gross: "25400",
            ..Doc::default()
        }
        .parse();
        let response = QueryResponse::from(&document);
        assert!(response.credit_entries.is_empty());
        assert_eq!(response.outstanding, Some(dec!(25400)));
    }

    /// `{scope, account: {id}, namespace, credentials}`, the credentials
    /// tagged by `state`: `ok`, or `rejected` with szamlazz.hu's code and
    /// message.
    #[test]
    fn check_account_response_round_trips() {
        let account = Account::new("acme", "acme");
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
                "account": { "id": "acme" },
                "namespace": "acct",
                "credentials": { "state": "ok" },
            })
        );

        response.scope = None;
        response.credentials = CredentialsCheck::Rejected {
            code: "3".to_owned(),
            message: "Sikertelen bejelentkezés.".to_owned(),
        };
        let json = round_trip(&response);
        assert_eq!(json["scope"], serde_json::Value::Null);
        assert_eq!(json["account"], json!({ "id": "acme" }));
        assert_eq!(
            json["credentials"],
            json!({ "state": "rejected", "code": "3", "message": "Sikertelen bejelentkezés." })
        );
    }

    /// The taxpayer response is a projection of the agent crate's
    /// `TaxpayerInfo`, field for field, and every field but `valid` defaults:
    /// NAV's `valid: false` (no taxpayer data) and an address NAV gives no
    /// detail for decode as they come.
    #[test]
    fn query_taxpayer_response_projects_the_agent_info_and_defaults_what_nav_omits() {
        use szamlazz_agent::ops::taxpayer::QueryTaxpayer;

        let body = br#"<QueryTaxpayerResponse xmlns="http://schemas.nav.gov.hu/OSA/2.0/api"><result><funcCode>OK</funcCode></result>
            <taxpayerValidity>true</taxpayerValidity><taxpayerData><taxpayerName>SYNTHETIC SOFTWARE KFT.</taxpayerName>
            <taxNumberDetail><taxpayerId>12345678</taxpayerId><vatCode>2</vatCode></taxNumberDetail>
            <taxpayerAddressList><taxpayerAddressItem><taxpayerAddressType>SITE</taxpayerAddressType><taxpayerAddress>
            <countryCode>HU</countryCode><region>Pest</region><postalCode>1111</postalCode>
            <city>Budapest</city><streetName>Fo</streetName><publicPlaceCategory>UTCA</publicPlaceCategory>
            <number>1</number><building>A</building><staircase>2</staircase><floor>3</floor>
            <door>4</door><lotNumber>123/4</lotNumber><additionalAddressDetail>Main road 1.</additionalAddressDetail>
            </taxpayerAddress></taxpayerAddressItem></taxpayerAddressList></taxpayerData>
            </QueryTaxpayerResponse>"#;
        let info = QueryTaxpayer::new("12345678")
            .expect("prefix")
            .parse(&RawResponse::new::<&str, &str>([], body.to_vec()))
            .expect("parse");
        let response = QueryTaxpayerResponse::from(info);

        let json = round_trip(&response);
        assert_eq!(
            json,
            json!({
                "valid": true,
                "name": "SYNTHETIC SOFTWARE KFT.",
                "tax_number": "12345678",
                "vat_code": "2",
                "addresses": [{
                    "kind": "SITE",
                    "country_code": "HU",
                    "region": "Pest",
                    "postal_code": "1111",
                    "city": "Budapest",
                    "street_name": "Fo",
                    "public_place_category": "UTCA",
                    "number": "1",
                    "building": "A",
                    "staircase": "2",
                    "floor": "3",
                    "door": "4",
                    "lot_number": "123/4",
                    "additional_address_detail": "Main road 1.",
                }],
            })
        );

        // The minimal shape (NAV's `valid: false`: no taxpayer data) decodes.
        let minimal: QueryTaxpayerResponse =
            serde_json::from_value(json!({"valid": false})).expect("deserialize");
        assert!(!minimal.valid);
        assert_eq!(minimal.name, None);
        assert!(minimal.addresses.is_empty());
        let bare_address: QueryTaxpayerResponse =
            serde_json::from_value(json!({"valid": true, "addresses": [{}]})).expect("deserialize");
        assert_eq!(bare_address.addresses, [TaxpayerAddress::default()]);
    }
}
