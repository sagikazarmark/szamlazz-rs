//! The unit tests' shared fixtures: [`Doc`], a queried document, and
//! [`LogCapture`], a `tracing` capture.
//!
//! [`Doc`] renders szamlazz.hu's `<szamla>` response XML and parses it into
//! an [`InvoiceDocument`] the way the gateway parses a query answer. The Számla
//! Agent crate's response types are `#[non_exhaustive]` on purpose, so a
//! unit test cannot construct one directly; the XML is the seam, and it is
//! what szamlazz.hu actually says (design §11 — tests state the answer
//! szamlazz.hu gives, not a second model of it). Every unit test that needs a
//! found document builds it here (#18); the two renderers below are the
//! deliberate exceptions.
//!
//! The wiremock integration tests (`tests/gateway.rs`, `tests/service.rs`)
//! carry their own `Doc` and do not share this one: a `#[cfg(test)]` module is
//! invisible to a `tests/` crate, and the alternative — a `test-support`
//! cargo feature enabled by a `[dev-dependencies]` self-reference — would make
//! a test fixture part of the crate's public feature set (docs.rs builds with
//! `all-features`, and a public feature is semver surface). The e2e `Doc`
//! also carries a harness-only `external_id` selector that is not part of any
//! document body. Two small renderers of one verified XML shape were judged
//! cheaper than that surface.
//!
//! `service::journal`'s `document()` is not a fixture of this kind and stays
//! where it is: it renders *every* element the `szamla` XML can carry, so
//! that a rename anywhere in the journaled types is caught, and its output is
//! pinned as JSON under `tests/journal/` — porting it here would churn the
//! pinned fixtures for no test.
//!
//! [`LogCapture`] is what the sentinel tests assert a warning through: what
//! it says, and that no agent key is in it.

use jiff::civil::{Date, date};
use szamlazz_agent::InvoiceNumber;
use szamlazz_agent::ops::query_pdf::InvoiceSelector;
use szamlazz_agent::ops::query_xml::{InvoiceDocument, QueryInvoiceXml};
use szamlazz_agent::wire::{AgentRequest as _, RawResponse};

/// The `szallito/id` of the documents [`Doc`] renders unless a test says
/// otherwise: the seller record's id as szamlazz.hu prints it in a query body
/// (972720 on the test account). Wire realism only — the worker holds no
/// account pin (ADR 0006, account-pin amendment), and tests that render
/// another value assert exactly that.
pub(crate) const SUPPLIER: u64 = 972_720;

/// The `telj` every document carries unless a test says otherwise: the
/// fulfillment date a storno of it must repeat (ADR 0007).
pub(crate) const ORIGINAL_TELJ: Date = date(2026, 7, 15);

/// A queried document, rendered as szamlazz.hu's `<szamla>` response XML.
///
/// [`Doc::new`] is a live test-account document of `ORD-1` from [`SUPPLIER`]
/// and [`Doc::default`] is its `SZ-1` invoice; override fields with
/// struct-update syntax and call [`Doc::parse`].
#[derive(Debug, Clone)]
pub(crate) struct Doc<'a> {
    /// `szamlaszam`.
    pub(crate) number: &'a str,
    /// `tipus` — `SZ`, `D`, `ES`, `VS`, `HS`, `SS`, …
    pub(crate) tipus: &'a str,
    /// `rendelesszam`; `None` renders no element — a document issued outside
    /// any order.
    pub(crate) order: Option<&'a str>,
    /// `teszt` — whether a test account issued the document. Parsed and
    /// projected by `query`, compared with nothing (ADR 0006, account-pin
    /// amendment).
    pub(crate) test: bool,
    /// `szallito/id` — the seller record's id in the `<szallito>` block.
    /// Parsed, compared with nothing (ADR 0006, account-pin amendment).
    pub(crate) supplier_id: u64,
    /// `<sztornozott>true</sztornozott>` — the document is reversed (as
    /// observed); `false` renders no element, as on a live document and on
    /// the storno invoice itself.
    pub(crate) reversed: bool,
    /// `hivszamlaszam` — the invoice a storno or a corrective references.
    pub(crate) referenced_invoice: Option<&'a str>,
    /// `hivdijbekszam` — the proforma an invoice or prepayment consumed.
    pub(crate) referenced_proforma: Option<&'a str>,
    /// `eszamla`; `None` follows `tipus` — `0` on a proforma, `2` (an
    /// e-invoice code) on anything else. szamlazz.hu reports `1` for a paper
    /// invoice and `3` for one created with `eszamla=true` (P73).
    pub(crate) eszamla: Option<i32>,
    /// `kelt`; `None` renders no element.
    pub(crate) issue_date: Option<Date>,
    /// `telj`; `None` renders no element — szamlazz.hu breaking its schema.
    pub(crate) fulfillment_date: Option<Date>,
    /// `osszegek/totalossz/netto`.
    pub(crate) net: &'a str,
    /// `osszegek/totalossz/afa`.
    pub(crate) vat: &'a str,
    /// `osszegek/totalossz/brutto` — what a document with no credit entries
    /// owes in full.
    pub(crate) gross: &'a str,
    /// `kifizetesek` — the credit entries registered against the document;
    /// empty renders no element.
    pub(crate) payments: &'a [CreditRecord<'a>],
    /// Further `<alap>` children, verbatim (`<fizh>…</fizh><devizanem>HUF</devizanem>`),
    /// for what no field covers; appended after the fields' elements, which
    /// the parser does not mind. Must not repeat an element a field renders.
    pub(crate) alap_extra: &'a str,
}

/// A credit entry (`kifizetes`) on a [`Doc`].
#[derive(Debug, Clone)]
pub(crate) struct CreditRecord<'a> {
    /// `datum`.
    pub(crate) date: Date,
    /// `jogcim` — the credit entry's title, e.g. `átutalás`.
    pub(crate) title: &'a str,
    /// `osszeg`.
    pub(crate) amount: &'a str,
    /// `megjegyzes`.
    pub(crate) comment: Option<&'a str>,
    /// `bankszamlaszam`.
    pub(crate) bank_account: Option<&'a str>,
}

impl<'a> CreditRecord<'a> {
    /// A credit entry of `amount` on `date` under `title`, with no comment
    /// and no bank account.
    pub(crate) const fn new(date: Date, title: &'a str, amount: &'a str) -> Self {
        Self {
            date,
            title,
            amount,
            comment: None,
            bank_account: None,
        }
    }
}

impl<'a> Doc<'a> {
    /// A live test-account document of `ORD-1` from [`SUPPLIER`].
    pub(crate) const fn new(number: &'a str, tipus: &'a str) -> Self {
        Self {
            number,
            tipus,
            order: Some("ORD-1"),
            test: true,
            supplier_id: SUPPLIER,
            reversed: false,
            referenced_invoice: None,
            referenced_proforma: None,
            eszamla: None,
            issue_date: Some(date(2026, 9, 3)),
            fulfillment_date: Some(ORIGINAL_TELJ),
            net: "1000",
            vat: "270",
            gross: "1270",
            payments: &[],
            alap_extra: "",
        }
    }

    /// The document as szamlazz.hu's `<szamla>` response body.
    pub(crate) fn xml(&self) -> String {
        let opt = |tag: &str, value: Option<&str>| {
            value.map_or_else(String::new, |value| format!("<{tag}>{value}</{tag}>"))
        };
        let eszamla = self
            .eszamla
            .unwrap_or(if self.tipus == "D" { 0 } else { 2 });
        let kelt = self.issue_date.map(|date| date.to_string());
        let telj = self.fulfillment_date.map(|date| date.to_string());
        let payments = if self.payments.is_empty() {
            String::new()
        } else {
            let entries = self.payments.iter().fold(String::new(), |xml, entry| {
                format!(
                    "{xml}<kifizetes><datum>{}</datum><jogcim>{}</jogcim><osszeg>{}</osszeg>{}{}</kifizetes>",
                    entry.date,
                    entry.title,
                    entry.amount,
                    opt("megjegyzes", entry.comment),
                    opt("bankszamlaszam", entry.bank_account),
                )
            });
            format!("<kifizetesek>{entries}</kifizetesek>")
        };
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<szamla xmlns="http://www.szamlazz.hu/szamla">
  <szallito><id>{supplier}</id><nev>Seller</nev><cim><irsz>1111</irsz><telepules>Budapest</telepules><cim>Fő u. 1.</cim></cim></szallito>
  <alap><id>924307338</id><szamlaszam>{number}</szamlaszam><tipus>{tipus}</tipus><eszamla>{eszamla}</eszamla>{hivszamlaszam}{hivdijbekszam}{kelt}{telj}{rendelesszam}<teszt>{test}</teszt>{sztornozott}{alap_extra}</alap>
  <vevo><nev>Buyer</nev></vevo>
  <tetelek></tetelek>
  <osszegek><totalossz><netto>{net}</netto><afa>{vat}</afa><brutto>{gross}</brutto></totalossz></osszegek>
  {payments}
</szamla>"#,
            supplier = self.supplier_id,
            number = self.number,
            tipus = self.tipus,
            hivszamlaszam = opt("hivszamlaszam", self.referenced_invoice),
            hivdijbekszam = opt("hivdijbekszam", self.referenced_proforma),
            kelt = opt("kelt", kelt.as_deref()),
            telj = opt("telj", telj.as_deref()),
            rendelesszam = opt("rendelesszam", self.order),
            test = self.test,
            sztornozott = if self.reversed {
                "<sztornozott>true</sztornozott>"
            } else {
                ""
            },
            alap_extra = self.alap_extra,
            net = self.net,
            vat = self.vat,
            gross = self.gross,
        )
    }

    /// The document parsed as the gateway parses a query answer.
    pub(crate) fn parse(&self) -> InvoiceDocument {
        QueryInvoiceXml::new(InvoiceSelector::InvoiceNumber(InvoiceNumber::new(
            self.number,
        )))
        .parse(&RawResponse::new::<&str, &str>([], self.xml().into_bytes()))
        .expect("the rendered szamla XML parses")
    }

    /// [`Doc::parse`] boxed, as the gateway outcomes carry a found document.
    pub(crate) fn boxed(&self) -> Box<InvoiceDocument> {
        Box::new(self.parse())
    }
}

impl Default for Doc<'_> {
    fn default() -> Self {
        Self::new("SZ-1", "SZ")
    }
}

/// Captures every `tracing` event the current thread emits, formatted, so a
/// test can assert what a warning says — and what it does not (an agent key).
///
/// [`LogCapture::subscribe`] installs a `TRACE`-level subscriber as the
/// thread's default for the returned guard's lifetime. tracing caches a
/// callsite's interest on its first hit, and a first hit from a parallel test
/// thread — which has no subscriber — would cache it as disabled; the caller
/// hits the callsite it is about once *after* subscribing (a warm-up event it
/// can tell apart), then calls [`LogCapture::rebuild_interest`] so the cache
/// is re-evaluated against this thread's subscriber.
#[derive(Clone, Default)]
pub(crate) struct LogCapture(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl LogCapture {
    /// Installs the capturing subscriber as this thread's default.
    pub(crate) fn subscribe(&self) -> tracing::subscriber::DefaultGuard {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(self.clone())
            .with_ansi(false)
            .finish();
        tracing::subscriber::set_default(subscriber)
    }

    /// Re-evaluates every registered callsite's interest; see the type docs.
    pub(crate) fn rebuild_interest() {
        tracing::callsite::rebuild_interest_cache();
    }

    /// Everything captured so far.
    pub(crate) fn logs(&self) -> String {
        String::from_utf8(self.0.lock().expect("capture").clone()).expect("utf-8")
    }
}

impl std::io::Write for LogCapture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("capture").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogCapture {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// The builder's own tests, at the [`Doc::parse`] seam: what each field
/// renders as, read back through the Számla Agent parser.
mod tests {
    use rust_decimal::dec;

    use super::*;

    /// The default document is what the worker's pins and derivations read:
    /// a live test-account `SZ-1` of `ORD-1` from [`SUPPLIER`], an e-invoice
    /// with the fulfillment date of [`ORIGINAL_TELJ`], reversed by nobody,
    /// referencing nothing, with no credit entries.
    #[test]
    fn the_default_document_is_a_live_test_invoice_of_ord_1() {
        let document = Doc::default().parse();
        assert_eq!(document.info.invoice_number.as_str(), "SZ-1");
        assert_eq!(document.info.document_type, "SZ");
        assert_eq!(document.info.order_number.as_deref(), Some("ORD-1"));
        assert!(document.info.test);
        assert_eq!(document.supplier.id, Some(SUPPLIER));
        assert_eq!(document.info.fulfillment_date, Some(ORIGINAL_TELJ));
        assert_eq!(document.info.e_invoice.code(), 2);
        assert_eq!(document.info.reversed, None);
        assert_eq!(document.info.referenced_invoice_number, None);
        assert_eq!(document.info.referenced_proforma_number, None);
        assert!(document.payments.is_empty());
    }

    /// Each marker the worker reads renders from its field — `rendelesszam`,
    /// `sztornozott`, and the `hivszamlaszam` / `hivdijbekszam` references of
    /// a storno and of the invoice that consumed a proforma — and so do the
    /// two it parses but compares with nothing, `teszt` and `szallito/id`.
    #[test]
    fn the_markers_render_from_their_fields() {
        let other = Doc {
            order: Some("ORD-2"),
            test: false,
            supplier_id: 1,
            reversed: true,
            ..Doc::new("SZ-9", "SZ")
        }
        .parse();
        assert_eq!(other.info.invoice_number.as_str(), "SZ-9");
        assert_eq!(other.info.order_number.as_deref(), Some("ORD-2"));
        assert!(!other.info.test);
        assert_eq!(other.supplier.id, Some(1));
        assert_eq!(other.info.reversed, Some(true));

        let unmanaged = Doc {
            order: None,
            ..Doc::default()
        }
        .parse();
        assert_eq!(unmanaged.info.order_number, None);

        let storno = Doc {
            referenced_invoice: Some("SZ-1"),
            ..Doc::new("SS-1", "SS")
        }
        .parse();
        assert_eq!(storno.info.document_type, "SS");
        assert_eq!(
            storno
                .info
                .referenced_invoice_number
                .as_ref()
                .map(InvoiceNumber::as_str),
            Some("SZ-1")
        );
        assert_eq!(storno.info.reversed, None, "a storno carries no marker");

        let consumer = Doc {
            referenced_proforma: Some("D-1"),
            ..Doc::default()
        }
        .parse();
        assert_eq!(
            consumer
                .info
                .referenced_proforma_number
                .as_ref()
                .map(InvoiceNumber::as_str),
            Some("D-1")
        );
    }

    /// `eszamla` follows `tipus` — `0` on a proforma, `2` (an e-invoice code)
    /// on anything else — unless a test sets the code itself.
    #[test]
    fn eszamla_follows_the_kind_unless_set() {
        assert_eq!(Doc::new("D-1", "D").parse().info.e_invoice.code(), 0);
        assert_eq!(Doc::new("ES-1", "ES").parse().info.e_invoice.code(), 2);
        let paper = Doc {
            eszamla: Some(1),
            ..Doc::default()
        };
        assert_eq!(paper.parse().info.e_invoice.code(), 1);
    }

    /// What `get` and the `Szamlazz.Agent.query` projection read beyond the
    /// pins: `kelt`, the totals, the credit entries (`kifizetesek`, each with
    /// its date, title, amount and the optional comment and bank account), and
    /// any further `<alap>` child a test needs verbatim.
    #[test]
    fn dates_totals_credit_entries_and_extra_alap_children_render() {
        let document = Doc {
            issue_date: Some(date(2026, 7, 4)),
            fulfillment_date: Some(date(2026, 7, 4)),
            net: "20000",
            vat: "5400",
            gross: "25400",
            payments: &[
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

        assert_eq!(document.info.issue_date, Some(date(2026, 7, 4)));
        assert_eq!(document.info.fulfillment_date, Some(date(2026, 7, 4)));
        assert_eq!(document.totals.total.net, dec!(20000));
        assert_eq!(document.totals.total.vat, dec!(5400));
        assert_eq!(document.totals.total.gross, dec!(25400));
        assert_eq!(document.info.due_date, Some(date(2026, 7, 12)));
        assert_eq!(document.info.currency.as_deref(), Some("HUF"));

        let [first, second] = document.payments.as_slice() else {
            panic!("two credit entries, got {:?}", document.payments);
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

        // `telj` absent, and `telj` empty — szamlazz.hu breaking its schema
        // either way; both parse as no fulfillment date.
        let without_telj = Doc {
            fulfillment_date: None,
            ..Doc::default()
        }
        .parse();
        assert_eq!(without_telj.info.fulfillment_date, None);
        let empty_telj = Doc {
            fulfillment_date: None,
            alap_extra: "<telj></telj>",
            ..Doc::default()
        }
        .parse();
        assert_eq!(empty_telj.info.fulfillment_date, None);
    }
}
