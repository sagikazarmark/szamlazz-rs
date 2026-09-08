//! The unit tests' shared fixtures: [`Doc`], a queried document,
//! [`LogCapture`], a `tracing` capture, and [`open_gateway`], a gateway over
//! an HTTP client that loads no root certificates.
//!
//! [`Doc`] renders szamlazz.hu's `<szamla>` response XML, parses it into the
//! Számla Agent crate's [`InvoiceDocument`] and projects it onto the worker's
//! [`FoundDocument`] the way the gateway reads a query answer. Both the
//! agent's response types and the projection are `#[non_exhaustive]` on
//! purpose, so a unit test cannot construct one directly; the XML is the
//! seam, and it is what szamlazz.hu actually says (tests state the answer
//! szamlazz.hu gives, not a second model of it). Every unit test that needs a
//! found document builds it here; the two renderers below are the deliberate
//! exceptions.
//!
//! The wiremock integration tests (`tests/gateway.rs`, `tests/e2e/`)
//! carry their own `Doc` and do not share this one: a `#[cfg(test)]` module is
//! invisible to a `tests/` crate, and the alternative (a `test-support`
//! cargo feature enabled by a `[dev-dependencies]` self-reference) would make
//! a test fixture part of the crate's public feature set (docs.rs builds with
//! `all-features`, and a public feature is semver surface). The e2e `Doc`
//! also carries a harness-only `external_id` selector that is not part of any
//! document body. Two small renderers of one verified XML shape were judged
//! cheaper than that surface.
//!
//! `service::journal`'s `document()` is not a fixture of this kind and stays
//! where it is: it renders *every* element the `szamla` XML can carry, so
//! that a rename anywhere in the journaled types is caught, and its output is
//! pinned as JSON under `tests/journal/`; porting it here would churn the
//! pinned fixtures for no test.
//!
//! [`LogCapture`] is what the sentinel tests assert a warning through: what
//! it says, and that no agent key is in it.
//!
//! [`open_gateway`] is how every unit test opens a [`Gateway`]: as the
//! prologue does, but over [`http_client`], the default client's settings
//! (a cookie jar, the request timeout, no redirects) with **no root
//! certificates**. Building a default `reqwest::Client` parses the system CA
//! store (about 28 ms of CPU per client through the platform verifier, and a
//! failure on a host without a store), for tests whose every endpoint is plain
//! `http://` (a wiremock, `127.0.0.1:1`). The wiremock and e2e harnesses
//! build the same client for themselves; the e2e deployment's gateways are
//! the prologue's own `Gateway::open`.

use jiff::civil::{Date, date};
use szamlazz_agent::client::REQUEST_TIMEOUT;
use szamlazz_agent::ops::query_pdf::InvoiceSelector;
use szamlazz_agent::ops::query_xml::{InvoiceDocument, QueryInvoiceXml};
use szamlazz_agent::wire::{AgentRequest as _, RawResponse};
use szamlazz_agent::{Credentials, InvoiceNumber, reqwest};

use crate::account::Account;
use crate::gateway::{FoundDocument, Gateway};

/// The HTTP client [`open_gateway`] opens a gateway over: the default client
/// (see `szamlazz_agent::client`) minus the root certificates (see the module
/// docs).
pub(crate) fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .tls_certs_only(std::iter::empty())
        .cookie_store(true)
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("http client")
}

/// A gateway for `account` with `credentials`, opened as the prologue opens
/// one per execution, over a fresh [`http_client`].
pub(crate) fn open_gateway(account: Account, credentials: Credentials) -> Gateway {
    Gateway::open_with_http(account, credentials, http_client()).expect("gateway")
}

/// The `szallito/id` of the documents [`Doc`] renders unless a test says
/// otherwise: the seller record's id as szamlazz.hu prints it in a query body
/// (972720 on the test account). Wire realism only: the worker holds no
/// account pin, and tests that render another value assert exactly that.
pub(crate) const SUPPLIER: u64 = 972_720;

/// The `telj` every document carries unless a test says otherwise: the
/// fulfillment date a storno of it must repeat.
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
    /// `tipus`: `SZ`, `D`, `ES`, `VS`, `HS`, `SS`, …
    pub(crate) tipus: &'a str,
    /// `rendelesszam`; `None` renders no element: a document issued outside
    /// any order.
    pub(crate) order: Option<&'a str>,
    /// `teszt`: whether a test account issued the document. Parsed and
    /// projected by `query`, compared with nothing. `None` renders no element:
    /// szamlazz.hu breaking its schema, a document that does not say (the
    /// agent crate reports it as `None`).
    pub(crate) test: Option<bool>,
    /// `szallito/id`: the seller record's id in the `<szallito>` block.
    /// Parsed, compared with nothing.
    pub(crate) supplier_id: u64,
    /// `<sztornozott>true</sztornozott>`: the document is reversed (as
    /// observed); `false` renders no element, as on a live document and on
    /// the storno invoice itself.
    pub(crate) reversed: bool,
    /// `hivszamlaszam`: the invoice a storno or a corrective references.
    pub(crate) referenced_invoice: Option<&'a str>,
    /// `hivdijbekszam`: the proforma an invoice or prepayment consumed.
    pub(crate) referenced_proforma: Option<&'a str>,
    /// `eszamla`; `None` follows `tipus`: `0` on a proforma, `2` (an
    /// e-invoice code) on anything else. szamlazz.hu reports `1` for a paper
    /// invoice and `3` for one created with `eszamla=true` (P73).
    pub(crate) eszamla: Option<i32>,
    /// `kelt`; `None` renders no element.
    pub(crate) issue_date: Option<Date>,
    /// `telj`; `None` renders no element: szamlazz.hu breaking its schema.
    pub(crate) fulfillment_date: Option<Date>,
    /// `osszegek/totalossz/netto`.
    pub(crate) net: &'a str,
    /// `osszegek/totalossz/afa`.
    pub(crate) vat: &'a str,
    /// `osszegek/totalossz/brutto`: what a document with no credit entries
    /// owes in full.
    pub(crate) gross: &'a str,
    /// `kifizetesek`: the credit entries registered against the document;
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
    /// `jogcim`: the credit entry's title, e.g. `átutalás`.
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
            test: Some(true),
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
        let teszt = self.test.map(|test| test.to_string());
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
  <alap><id>924307338</id><szamlaszam>{number}</szamlaszam><tipus>{tipus}</tipus><eszamla>{eszamla}</eszamla>{hivszamlaszam}{hivdijbekszam}{kelt}{telj}{rendelesszam}{teszt}{sztornozott}{alap_extra}</alap>
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
            teszt = opt("teszt", teszt.as_deref()),
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

    /// The document as the Számla Agent crate parses a query answer, before
    /// the worker's projection: for a test that needs to put a value the
    /// renderer cannot into the wire type (a `rendelesszam` the parser would
    /// have trimmed) and read what [`FoundDocument::from`] makes of it.
    pub(crate) fn wire(&self) -> InvoiceDocument {
        QueryInvoiceXml::new(InvoiceSelector::InvoiceNumber(InvoiceNumber::new(
            self.number,
        )))
        .parse(&RawResponse::new::<&str, &str>([], self.xml().into_bytes()))
        .expect("the rendered szamla XML parses")
    }

    /// The document as the gateway reads a query answer: parsed by the
    /// Számla Agent crate ([`Doc::wire`]) and projected onto the worker's
    /// [`FoundDocument`].
    pub(crate) fn parse(&self) -> FoundDocument {
        FoundDocument::from(self.wire())
    }

    /// [`Doc::parse`] boxed, as the gateway outcomes carry a found document.
    pub(crate) fn boxed(&self) -> Box<FoundDocument> {
        Box::new(self.parse())
    }
}

impl Default for Doc<'_> {
    fn default() -> Self {
        Self::new("SZ-1", "SZ")
    }
}

/// Captures every `tracing` event the current thread emits, formatted, so a
/// test can assert what a warning says, and what it does not (an agent key).
///
/// [`LogCapture::subscribe`] installs a `TRACE`-level subscriber as the
/// thread's default for the returned guard's lifetime. tracing caches a
/// callsite's interest on its first hit, and a first hit from a parallel test
/// thread (which has no subscriber) would cache it as disabled; the caller
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

/// The builder's own tests, at the [`Doc::wire`] seam: what each field
/// renders as, read back through the Számla Agent parser (the projection's
/// reading of it is `gateway::document`'s own test).
mod tests {
    use rust_decimal::dec;

    use super::*;

    /// The default document is what the worker's pins and derivations read:
    /// a live test-account `SZ-1` of `ORD-1` from [`SUPPLIER`], an e-invoice
    /// with the fulfillment date of [`ORIGINAL_TELJ`], reversed by nobody,
    /// referencing nothing, with no credit entries.
    #[test]
    fn the_default_document_is_a_live_test_invoice_of_ord_1() {
        let document = Doc::default().wire();
        assert_eq!(document.info.invoice_number.as_str(), "SZ-1");
        assert_eq!(document.info.document_type, "SZ");
        assert_eq!(document.info.order_number.as_deref(), Some("ORD-1"));
        assert_eq!(document.info.test, Some(true));
        assert_eq!(document.supplier.id, Some(SUPPLIER));
        assert_eq!(document.info.fulfillment_date, Some(ORIGINAL_TELJ));
        assert_eq!(document.info.e_invoice.code(), 2);
        assert_eq!(document.info.reversed, None);
        assert_eq!(document.info.referenced_invoice_number, None);
        assert_eq!(document.info.referenced_proforma_number, None);
        assert!(document.payments.is_empty());
    }

    /// Each marker the worker reads renders from its field (`rendelesszam`,
    /// `sztornozott`, and the `hivszamlaszam` / `hivdijbekszam` references of
    /// a storno and of the invoice that consumed a proforma), and so do the
    /// two it parses but compares with nothing, `teszt` and `szallito/id`.
    #[test]
    fn the_markers_render_from_their_fields() {
        let other = Doc {
            order: Some("ORD-2"),
            test: Some(false),
            supplier_id: 1,
            reversed: true,
            ..Doc::new("SZ-9", "SZ")
        }
        .wire();
        assert_eq!(other.info.invoice_number.as_str(), "SZ-9");
        assert_eq!(other.info.order_number.as_deref(), Some("ORD-2"));
        assert_eq!(other.info.test, Some(false));
        assert_eq!(other.supplier.id, Some(1));
        assert_eq!(other.info.reversed, Some(true));

        let unmanaged = Doc {
            order: None,
            ..Doc::default()
        }
        .wire();
        assert_eq!(unmanaged.info.order_number, None);

        let unknown_mode = Doc {
            test: None,
            ..Doc::default()
        };
        assert!(
            !unknown_mode.xml().contains("<teszt>"),
            "renders no element"
        );
        assert_eq!(unknown_mode.wire().info.test, None);

        let storno = Doc {
            referenced_invoice: Some("SZ-1"),
            ..Doc::new("SS-1", "SS")
        }
        .wire();
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
        .wire();
        assert_eq!(
            consumer
                .info
                .referenced_proforma_number
                .as_ref()
                .map(InvoiceNumber::as_str),
            Some("D-1")
        );
    }

    /// `eszamla` follows `tipus` (`0` on a proforma, `2` (an e-invoice code)
    /// on anything else) unless a test sets the code itself.
    #[test]
    fn eszamla_follows_the_kind_unless_set() {
        assert_eq!(Doc::new("D-1", "D").wire().info.e_invoice.code(), 0);
        assert_eq!(Doc::new("ES-1", "ES").wire().info.e_invoice.code(), 2);
        let paper = Doc {
            eszamla: Some(1),
            ..Doc::default()
        };
        assert_eq!(paper.wire().info.e_invoice.code(), 1);
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
        .wire();

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

        // `telj` absent, and `telj` empty: szamlazz.hu breaking its schema
        // either way; both parse as no fulfillment date.
        let without_telj = Doc {
            fulfillment_date: None,
            ..Doc::default()
        }
        .wire();
        assert_eq!(without_telj.info.fulfillment_date, None);
        let empty_telj = Doc {
            fulfillment_date: None,
            alap_extra: "<telj></telj>",
            ..Doc::default()
        }
        .wire();
        assert_eq!(empty_telj.info.fulfillment_date, None);
    }
}
