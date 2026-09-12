//! The worker's own projections of a szamlazz.hu document, the types the
//! document outcomes journal.
//!
//! A [`FoundDocument`] is what the services read off a queried `<szamla>`
//! (the Számla Agent crate's [`InvoiceDocument`]) and an [`IssuedDocument`]
//! what they read off a create or storno reply (the [`CreatedInvoice`] of a
//! `CreationOutcome::Issued` or of a storno). Both are crate-owned (the journal rule of the
//! [`gateway`](crate::gateway) module docs), so neither carries what the
//! worker never reads into an entry the Restate UI shows: the buyer block,
//! the seller block, the line items, the PDF. What a document *is* to the
//! worker (live, ours, a storno of a number, an e-invoice) is read here,
//! once.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use szamlazz_agent::ops::invoice::CreatedInvoice;
use szamlazz_agent::ops::query_xml::{self, InvoiceAppearance, InvoiceDocument};
use szamlazz_agent::{Date, DocumentType};

use crate::contract::IssuedKind;
use crate::identity::OrderKey;

/// A queried document as the worker reads it: the projection of the Számla
/// Agent crate's [`InvoiceDocument`] the document outcomes journal
/// ([`LookupOutcome`](super::LookupOutcome), [`CreateOutcome`](super::CreateOutcome),
/// [`QueryOutcome`](super::QueryOutcome)).
///
/// What the handlers read of a found document, and its szamlazz.hu id: the
/// identity (`szamlaszam`, `tipus`), the markers the checks below are made
/// on (`rendelesszam`, `sztornozott`, `hivszamlaszam`, `hivdijbekszam`,
/// `eszamla`, `teszt`), the dates the storno and `Szamlazz.Agent.query`
/// repeat (`kelt`, `telj`, `fizh`), the currency, the grand total and the
/// credit entries, plus `alap/id`, which pins a deletion's target across its
/// fresh query and lets an operator correlate it with szamlazz.hu (#127, #201). The
/// buyer block, the seller block, the line items and the PDF szamlazz.hu
/// returns with the document are not here: the worker never reads them, and
/// a journal entry is visible in the Restate UI for the retention period. The
/// external id the document was queried by is not here either: szamlazz.hu
/// never echoes `szamlaKulsoAzon`, and the handler holds it from the key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct FoundDocument {
    /// szamlazz.hu's internal document identifier (`alap/id`): a document
    /// identifier, not an account's or a seller's. Compared by the fresh
    /// deletion guard and carried so that a journal entry names the document the way
    /// szamlazz.hu's own records do (the same value a create reply's
    /// `szlahu_id` header carries, [`IssuedDocument::document_id`]). An
    /// `i64`, the agent crate's width for every XSD integer.
    pub document_id: i64,
    /// The document number (`szamlaszam`).
    pub number: String,
    /// The document type (`tipus`): `SZ` invoice, `D` proforma, `ES`
    /// prepayment invoice, `VS` final invoice, `HS` corrective, `SS` storno,
    /// `SL` delivery note, …; the agent crate's open [`DocumentType`], which
    /// journals as the two-letter token, so a code the crate learns later is
    /// read on replay.
    pub document_type: DocumentType,
    /// The order number the document carries (`rendelesszam`), trimmed as
    /// szamlazz.hu matches it; `None` when the element is absent, empty or
    /// whitespace only: a document issued outside any order. The one reading
    /// of the element: what `Szamlazz.Agent.storno` answers as
    /// `managed_by_order`'s `order_key`, and what [`Self::carries_order`]
    /// compares with the key.
    pub order_number: Option<String>,
    /// Whether the document has been reversed (`sztornozott`): `Some(true)`
    /// after a storno, by anyone; `None` on a live document and on a storno
    /// invoice itself, which carries no marker. [`Self::is_live`] is the
    /// reading.
    pub reversed: Option<bool>,
    /// The invoice this document references (`hivszamlaszam`): the reversed
    /// invoice of a storno, the corrected one of a corrective.
    pub referenced_invoice_number: Option<String>,
    /// The proforma this document converted (`hivdijbekszam`).
    pub referenced_proforma_number: Option<String>,
    /// The `eszamla` code as szamlazz.hu reports it: `0` not an invoice (a
    /// proforma), `1` paper, `2`/`3` e-invoice; the agent crate's
    /// [`InvoiceAppearance`] reads it, and [`Self::e_invoice`] is the reading
    /// the storno handlers take. An `i64`, the code's width in the agent
    /// crate.
    pub appearance: i64,
    /// The issue date (`kelt`).
    pub issue_date: Option<Date>,
    /// The fulfillment date (`telj`): what a storno of this document must
    /// repeat. szamlazz.hu's schema has the element mandatory; `None` is a
    /// document that does not say.
    pub fulfillment_date: Option<Date>,
    /// The payment due date (`fizh`).
    pub due_date: Option<Date>,
    /// The currency (`devizanem`).
    pub currency: Option<String>,
    /// Issued from a test account (`teszt`), as szamlazz.hu reported it:
    /// `None` is a document that does not say. Compared with nothing by the
    /// worker. Seller verification reads the full document outside the journal.
    pub test: Option<bool>,
    /// The net grand total (`osszegek/totalossz/netto`).
    #[serde(deserialize_with = "crate::contract::decimal::required")]
    #[serde(serialize_with = "rust_decimal::serde::str::serialize")]
    pub net_total: Decimal,
    /// The VAT grand total (`osszegek/totalossz/afa`).
    #[serde(deserialize_with = "crate::contract::decimal::required")]
    #[serde(serialize_with = "rust_decimal::serde::str::serialize")]
    pub vat_total: Decimal,
    /// The gross grand total (`osszegek/totalossz/brutto`): what a document
    /// with no credit entries owes in full.
    #[serde(deserialize_with = "crate::contract::decimal::required")]
    #[serde(serialize_with = "rust_decimal::serde::str::serialize")]
    pub gross_total: Decimal,
    /// The credit entries registered against the document (`kifizetesek`),
    /// in the order szamlazz.hu lists them.
    pub credit_entries: Vec<RecordedCreditEntry>,
}

/// A credit entry as szamlazz.hu records it against a document (`kifizetes`):
/// the projection of the agent crate's
/// [`RecordedCreditEntry`](query_xml::RecordedCreditEntry) with what
/// `Szamlazz.Agent.query` shows of each entry. Crate-owned and journaled, like
/// [`FoundDocument`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RecordedCreditEntry {
    /// The payment date (`datum`).
    pub date: Date,
    /// The title (`jogcim`): the payment method it was settled by, as its
    /// wire token.
    pub title: String,
    /// The amount (`osszeg`), in the document's currency.
    #[serde(deserialize_with = "crate::contract::decimal::required")]
    #[serde(serialize_with = "rust_decimal::serde::str::serialize")]
    pub amount: Decimal,
    /// The free-text comment (`megjegyzes`).
    pub comment: Option<String>,
    /// The bank account the payment arrived on (`bankszamlaszam`).
    pub bank_account: Option<String>,
}

impl FoundDocument {
    /// Validate identity before a queried document can become worker evidence.
    /// Vendor numbers retain their full spelling and are not subject to caller
    /// input bounds. Diagnostics never copy response text into the journal.
    pub(super) fn validate_identity(&self, expected: Option<&str>) -> Result<(), &'static str> {
        if self.number.trim().is_empty() {
            return Err("query: missing document number");
        }
        if expected.is_some_and(|number| number != self.number) {
            return Err("query: document number differs from the requested number");
        }
        Ok(())
    }

    /// Whether the document is live: `reversed != Some(true)`.
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.reversed != Some(true)
    }

    /// Whether the document is the storno invoice (`SS`) reversing `number`.
    #[must_use]
    pub fn is_storno_of(&self, number: &str) -> bool {
        !self.number.trim().is_empty()
            && self.number != number
            && self.document_type == DocumentType::Storno
            && self.referenced_invoice_number.as_deref() == Some(number)
    }

    /// Whether the corrective names the intended original invoice.
    pub(crate) fn is_corrective_of(&self, number: &str) -> bool {
        self.document_type == DocumentType::Corrective
            && self.referenced_invoice_number.as_deref() == Some(number)
    }

    /// Whether the document is a legal invoice of the kinds an order carries:
    /// `SZ`, `ES` or `VS`. What the order-number hint treats as a *Foreign
    /// document* when it is live and not ours; stornos, correctives,
    /// proformas and delivery notes are not.
    #[must_use]
    pub fn is_invoice_family(&self) -> bool {
        matches!(
            self.document_type,
            DocumentType::Invoice | DocumentType::Prepayment | DocumentType::Final
        )
    }

    /// Whether szamlazz.hu can reverse the document: an `SZ`, `ES`, `VS` or
    /// `HS`. A proforma or a delivery note is echoed unchanged by a storno, a
    /// storno invoice is refused (14), and a code the agent crate does not
    /// know is not sent for.
    #[must_use]
    pub fn is_stornoable(&self) -> bool {
        matches!(
            self.document_type,
            DocumentType::Invoice
                | DocumentType::Prepayment
                | DocumentType::Final
                | DocumentType::Corrective
        )
    }

    /// Whether it is an e-invoice; `None` for non-invoices (proformas) and
    /// unknown `eszamla` codes, where the account default applies.
    ///
    /// What the storno handlers send as the storno's `eszamla`. szamlazz.hu
    /// does not require a storno's form to match its original's: a mismatch
    /// is accepted silently and the storno document takes the request's flag
    /// (P73), so this derivation, not the server, is what keeps a reversal
    /// in its original's form. `1` is paper and `2`/`3` are e-invoice codes,
    /// as the vendor annotation says and the test account confirmed (`3` for
    /// an invoice created with `eszamla=true`); the code set is the agent
    /// crate's [`InvoiceAppearance`], read off the journaled code, so a code
    /// the crate learns later is read on replay.
    #[must_use]
    pub fn e_invoice(&self) -> Option<bool> {
        match InvoiceAppearance::from(self.appearance) {
            InvoiceAppearance::Paper => Some(false),
            InvoiceAppearance::Electronic(_) => Some(true),
            _ => None,
        }
    }

    /// The registered credit entry amounts, in the order szamlazz.hu lists
    /// them.
    #[must_use]
    pub fn credit_entry_amounts(&self) -> Vec<Decimal> {
        self.credit_entries
            .iter()
            .map(|entry| entry.amount)
            .collect()
    }

    /// Whether the document carries `order` as its
    /// [order number](Self::order_number). What makes a document found by
    /// number this order's to act on or link.
    #[must_use]
    pub fn carries_order(&self, order: &OrderKey) -> bool {
        self.order_number.as_deref() == Some(order.as_str())
    }

    /// Whether the document is ours: it [carries
    /// `order`](Self::carries_order) and the `tipus` of `kind`. Nothing about
    /// the account: the worker holds no account pin (`teszt` is carried,
    /// never compared).
    #[must_use]
    pub fn is_ours(&self, order: &OrderKey, kind: IssuedKind) -> bool {
        self.carries_order(order) && self.document_type == kind.document_type()
    }
}

/// The projection of a queried `<szamla>`: the reads above, and the one
/// normalisation the worker makes on the wire, the order number trimmed and
/// an empty one read as none.
impl From<InvoiceDocument> for FoundDocument {
    fn from(document: InvoiceDocument) -> Self {
        let info = document.info;
        Self {
            document_id: info.id,
            number: info.invoice_number.as_str().to_owned(),
            document_type: info.document_type,
            order_number: info
                .order_number
                .as_deref()
                .map(str::trim)
                .filter(|order| !order.is_empty())
                .map(str::to_owned),
            reversed: info.reversed,
            referenced_invoice_number: info
                .referenced_invoice_number
                .map(|number| number.as_str().to_owned()),
            referenced_proforma_number: info
                .referenced_proforma_number
                .map(|number| number.as_str().to_owned()),
            appearance: info.appearance.code(),
            issue_date: info.issue_date,
            fulfillment_date: info.fulfillment_date,
            due_date: info.due_date,
            currency: info.currency.map(|currency| currency.as_str().to_owned()),
            test: info.test,
            net_total: document.totals.total.net,
            vat_total: document.totals.total.vat,
            gross_total: document.totals.total.gross,
            credit_entries: document
                .credit_entries
                .into_iter()
                .map(RecordedCreditEntry::from)
                .collect(),
        }
    }
}

impl From<query_xml::RecordedCreditEntry> for RecordedCreditEntry {
    fn from(entry: query_xml::RecordedCreditEntry) -> Self {
        Self {
            date: entry.date,
            title: entry.title.as_wire().to_owned(),
            amount: entry.amount,
            comment: entry.comment,
            bank_account: entry.bank_account,
        }
    }
}

/// A document szamlazz.hu issued in answer to a create or a storno, as the
/// worker reads the reply: the projection of the Számla Agent crate's
/// [`CreatedInvoice`] (the `Issued` arm of a create's `CreationOutcome`, and
/// a storno's reply) the write outcomes journal
/// ([`CreateOutcome::Issued`](super::CreateOutcome::Issued),
/// [`StornoOutcome::Reversed`](super::StornoOutcome::Reversed)).
///
/// Always numbered, since a [`CreatedInvoice`] is: a create reply without a
/// number (a PDF preview, which the worker never asks for) is the outcome's
/// other arm, and the create step re-queries instead of reporting it. The PDF
/// the reply may carry is not here. Crate-owned and journaled, like
/// [`FoundDocument`] (ADR 0009: no compatibility rule).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct IssuedDocument {
    /// The assigned document number (`szamlaszam`).
    pub number: String,
    /// szamlazz.hu's internal document identifier (the `szlahu_id` header):
    /// the same value a query returns as [`FoundDocument::document_id`];
    /// `None` when the header is absent or not a number. Read by no handler;
    /// carried for the same correlation.
    pub document_id: Option<i64>,
    /// The net total (`szamlanetto`).
    #[serde(default, deserialize_with = "crate::contract::decimal::optional")]
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
    pub net_total: Option<Decimal>,
    /// The gross total (`szamlabrutto`).
    #[serde(default, deserialize_with = "crate::contract::decimal::optional")]
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
    pub gross_total: Option<Decimal>,
    /// The outstanding amount (`kintlevoseg`).
    #[serde(default, deserialize_with = "crate::contract::decimal::optional")]
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
    pub outstanding: Option<Decimal>,
    /// The buyer-facing account URL (`vevoifiokurl`).
    pub customer_account_url: Option<String>,
    /// Whether szamlazz.hu issued the document but could not deliver its
    /// notification (code 56): issued all the same, with a warning.
    pub notification_delivery_failed: bool,
}

impl From<CreatedInvoice> for IssuedDocument {
    fn from(created: CreatedInvoice) -> Self {
        Self {
            number: created.invoice_number.as_str().to_owned(),
            document_id: created.document_id,
            net_total: created.net_total,
            gross_total: created.gross_total,
            outstanding: created.outstanding,
            customer_account_url: created.customer_account_url,
            notification_delivery_failed: created.notification_delivery_failed,
        }
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use rust_decimal::dec;

    use super::*;
    use crate::contract::IssuedKind;
    use crate::identity::OrderKey;
    use crate::test_support::{CreditRecord, Doc, ORIGINAL_TELJ};

    /// The projection of a queried document is what the handlers read and
    /// nothing else: the identity (`alap/id`, `szamlaszam`, `tipus`), the
    /// markers (`rendelesszam`, `sztornozott`, `hivszamlaszam`,
    /// `hivdijbekszam`, `eszamla`, `teszt`), the dates the storno and the
    /// `query` projection repeat (`kelt`, `telj`, `fizh`), the currency, the
    /// grand total and the credit entries with what `query` shows of each.
    #[test]
    fn a_found_document_is_the_projection_of_the_queried_szamla() {
        let found = Doc {
            referenced_invoice: Some("SZ-0"),
            referenced_proforma: Some("D-1"),
            eszamla: Some(3),
            issue_date: Some(date(2026, 7, 4)),
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

        assert_eq!(found.document_id, 924_307_338);
        assert_eq!(found.number, "SZ-1");
        assert_eq!(found.document_type, DocumentType::Invoice);
        assert_eq!(found.order_number.as_deref(), Some("ORD-1"));
        assert_eq!(found.reversed, None, "a live document carries no marker");
        assert_eq!(found.referenced_invoice_number.as_deref(), Some("SZ-0"));
        assert_eq!(found.referenced_proforma_number.as_deref(), Some("D-1"));
        assert_eq!(found.appearance, 3);
        assert_eq!(found.issue_date, Some(date(2026, 7, 4)));
        assert_eq!(found.fulfillment_date, Some(ORIGINAL_TELJ));
        assert_eq!(found.due_date, Some(date(2026, 7, 12)));
        assert_eq!(found.currency.as_deref(), Some("HUF"));
        assert_eq!(found.test, Some(true));
        assert_eq!(found.net_total, dec!(20000));
        assert_eq!(found.vat_total, dec!(5400));
        assert_eq!(found.gross_total, dec!(25400));

        let [first, second] = found.credit_entries.as_slice() else {
            panic!("two credit entries, got {:?}", found.credit_entries);
        };
        assert_eq!(first.date, date(2026, 7, 10));
        assert_eq!(first.title, "átutalás");
        assert_eq!(first.amount, dec!(10000));
        assert_eq!(first.comment.as_deref(), Some("first"));
        assert_eq!(first.bank_account.as_deref(), Some("1234-5678"));
        assert_eq!(second.date, date(2026, 7, 11));
        assert_eq!(second.title, "bankkártya");
        assert_eq!(second.amount, dec!(5000));
        assert_eq!(second.comment, None);
        assert_eq!(second.bank_account, None);

        // What szamlazz.hu leaves out is `None`, not invented.
        let bare = Doc {
            reversed: true,
            test: None,
            fulfillment_date: None,
            issue_date: None,
            ..Doc::default()
        }
        .parse();
        assert_eq!(bare.reversed, Some(true));
        assert_eq!(bare.test, None);
        assert_eq!(bare.fulfillment_date, None);
        assert_eq!(bare.issue_date, None);
        assert_eq!(bare.due_date, None);
        assert_eq!(bare.currency, None);
        assert_eq!(bare.referenced_invoice_number, None);
        assert_eq!(bare.referenced_proforma_number, None);
        assert!(bare.credit_entries.is_empty());
    }

    /// The projection serialises flat, under exactly the field names above,
    /// and nothing of the buyer, the seller, the line items or the PDF is in
    /// it: what the journal holds for the retention period. (The field order
    /// is serde's and asserted by nothing.)
    #[test]
    fn a_found_document_serialises_without_buyer_seller_items_or_pdf() {
        let json = serde_json::to_value(Doc::default().parse()).expect("serialises");
        let keys: std::collections::BTreeSet<&str> = json
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            std::collections::BTreeSet::from([
                "document_id",
                "number",
                "document_type",
                "order_number",
                "reversed",
                "referenced_invoice_number",
                "referenced_proforma_number",
                "appearance",
                "issue_date",
                "fulfillment_date",
                "due_date",
                "currency",
                "test",
                "net_total",
                "vat_total",
                "gross_total",
                "credit_entries",
            ])
        );
        assert_eq!(json["appearance"], 1, "the code as an integer");
        assert_eq!(json["document_type"], "SZ", "the tipus as its token");
        assert_eq!(json["net_total"], "1000", "decimals as strings");
        let back: FoundDocument = serde_json::from_value(json).expect("decodes");
        assert_eq!(back, Doc::default().parse());
    }

    /// The checks the services make on a found document, read off the
    /// projection: live unless `sztornozott` says otherwise; ours when it
    /// carries the order and the kind's `tipus`; the storno of a number when
    /// it is an `SS` referencing it; an e-invoice by the `eszamla` code as
    /// the agent crate reads it (`1` paper, `2`/`3` e-invoice, `0` not an
    /// invoice); the credit entry amounts in szamlazz.hu's order.
    #[test]
    fn the_projection_reads_the_checks_off_a_queried_document() {
        let order = OrderKey::parse("ORD-1").expect("order");
        let live = Doc {
            credit_entries: &[
                CreditRecord::new(date(2026, 7, 4), "transfer", "500"),
                CreditRecord::new(date(2026, 7, 5), "transfer", "770"),
            ],
            ..Doc::new("SZ-1", "SZ")
        }
        .parse();
        assert!(live.is_live());
        assert_eq!(live.e_invoice(), Some(false));
        assert_eq!(live.credit_entry_amounts(), [dec!(500), dec!(770)]);
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
        let corrective = Doc {
            referenced_invoice: Some("SZ-1"),
            ..Doc::new("HS-1", "HS")
        }
        .parse();
        assert!(
            !corrective.is_storno_of("SZ-1"),
            "a corrective references its base and reverses nothing"
        );

        // The two sets the services decide on, read off the type: the
        // invoice family (what the hint treats as foreign when live and not
        // ours) and what a storno can reverse.
        for (tipus, family, stornoable) in [
            ("SZ", true, true),
            ("ES", true, true),
            ("VS", true, true),
            ("HS", false, true),
            ("D", false, false),
            ("SL", false, false),
            ("SS", false, false),
            ("XX", false, false),
        ] {
            let document = Doc::new("N-1", tipus).parse();
            assert_eq!(document.is_invoice_family(), family, "{tipus}");
            assert_eq!(document.is_stornoable(), stornoable, "{tipus}");
        }
        // An unknown code journals and reads back as itself.
        let unknown = Doc::new("N-1", "XX").parse();
        assert_eq!(unknown.document_type, DocumentType::Other("XX".to_owned()));
        let json = serde_json::to_value(&unknown).expect("serialises");
        assert_eq!(json["document_type"], "XX");

        for (code, e_invoice) in [(1, Some(false)), (2, Some(true)), (3, Some(true))] {
            let document = Doc {
                eszamla: Some(code),
                ..Doc::default()
            }
            .parse();
            assert_eq!(document.e_invoice(), e_invoice, "eszamla {code}");
        }
        let proforma = Doc::new("D-1", "D").parse();
        assert_eq!(proforma.appearance, 0);
        assert_eq!(proforma.e_invoice(), None, "eszamla 0 is not an invoice");
        assert!(proforma.is_ours(&order, IssuedKind::Proforma));
        let unknown = Doc {
            eszamla: Some(9),
            ..Doc::default()
        }
        .parse();
        assert_eq!(unknown.e_invoice(), None, "an unknown code decides nothing");

        // No account pin: `teszt` is carried, compared with nothing.
        let other_account = Doc {
            test: Some(false),
            ..Doc::new("SZ-1", "SZ")
        }
        .parse();
        assert!(other_account.is_ours(&order, IssuedKind::Invoice));
    }

    /// The order number a document carries is `rendelesszam` trimmed, as
    /// szamlazz.hu matches it, and nothing when the element is absent, empty
    /// or whitespace only: a document issued outside any order. The one
    /// reading of the element, made once, in the projection: `carries_order`
    /// is that reading compared with the key. The agent crate's parser trims
    /// the element and reads an empty one as `None` already, so the rendered
    /// cases prove the pair end to end and the assigned ones prove the
    /// projection's own reading, which does not lean on the parser's.
    #[test]
    fn the_order_number_is_the_trimmed_rendelesszam_or_none() {
        let order = OrderKey::parse("ORD-1").expect("order");

        let plain = Doc::default().parse();
        assert_eq!(plain.order_number.as_deref(), Some("ORD-1"));
        assert!(plain.carries_order(&order));

        let padded = Doc {
            order: Some("  ORD-1 "),
            ..Doc::default()
        }
        .parse();
        assert_eq!(padded.order_number.as_deref(), Some("ORD-1"), "trimmed");
        assert!(padded.carries_order(&order));

        for outside_any_order in [None, Some(""), Some("   "), Some("\t\n")] {
            let document = Doc {
                order: outside_any_order,
                ..Doc::default()
            }
            .parse();
            assert_eq!(
                document.order_number, None,
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

        // The projection's own reading of the parsed value, with the parser's
        // normalisation out of the way.
        for (raw, read) in [
            (Some("ORD-1"), Some("ORD-1")),
            (Some("  ORD-1 "), Some("ORD-1")),
            (Some(""), None),
            (Some("   "), None),
            (Some("\t\n"), None),
            (None, None),
        ] {
            let found = Doc::default().assigned_order(raw);
            assert_eq!(found.order_number.as_deref(), read, "order_number {raw:?}");
            assert_eq!(
                found.carries_order(&order),
                read.is_some(),
                "order_number {raw:?}"
            );
        }
    }
}
