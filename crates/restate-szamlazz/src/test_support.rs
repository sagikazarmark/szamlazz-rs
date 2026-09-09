//! The unit tests' shared fixtures: [`Doc`], a queried document, parsed into
//! the worker's projection; [`LogCapture`], a `tracing` capture; and
//! [`open_gateway`], a gateway over an HTTP client that loads no root
//! certificates.
//!
//! [`Doc`] is the one synthetic szamlazz.hu document of the crate's tests,
//! shared by path with the integration harnesses (`tests/common/mod.rs`: the
//! renderer, the response templates and the wiremock selector matchers; the
//! gateway's wiremock tests and the e2e suite declare the same module), so a
//! fact learned about szamlazz.hu's XML is edited in one place. Until #134
//! there were three renderers of one XML shape, and this module's docs argued
//! for it: a `#[cfg(test)]` module is invisible to a `tests/` crate, and the
//! alternative then considered (a `test-support` cargo feature enabled by a
//! `[dev-dependencies]` self-reference) would have made a test fixture part
//! of the crate's public feature set (docs.rs builds with `all-features`, and
//! a public feature is semver surface). The `#[path]` include below sidesteps
//! both: the file lives under `tests/`, where the integration binaries
//! declare it as an ordinary module, and this `cfg(test)` module compiles the
//! same source into the library's tests without any feature. What this
//! module adds is the unit tests' seam: the rendered XML is parsed into the
//! Számla Agent crate's [`InvoiceDocument`] ([`Doc::wire`]) and projected
//! onto the worker's [`FoundDocument`] ([`Doc::parse`]) the way the gateway
//! reads a query answer. The agent's response types are `#[non_exhaustive]`
//! on purpose, so a unit test cannot construct one directly, and the
//! projection is constructed through the wire by convention (ADR 0008; a
//! literal would state a second model of the document): the XML is the seam,
//! and it is what szamlazz.hu actually says (tests state the answer
//! szamlazz.hu gives).
//!
//! `service::journal`'s `wire_document()` is not a fixture of this kind and
//! stays where it is: it renders *every* element the `szamla` XML can carry,
//! so that the data guard's positive control carries every key the
//! projection drops.
//!
//! [`LogCapture`] is what the sentinel tests assert a warning through: what
//! it says, and that no agent key is in it.
//!
//! [`open_gateway`] is how every unit test opens a [`Gateway`]: as the
//! prologue does, but over the shared [`http_client`], the default client's
//! settings (a cookie jar, the request timeout, no redirects) with **no root
//! certificates**. Building a default `reqwest::Client` parses the system CA
//! store (about 28 ms of CPU per client through the platform verifier, and a
//! failure on a host without a store), for tests whose every endpoint is plain
//! `http://` (a wiremock, `127.0.0.1:1`). The integration harnesses open
//! theirs over the same client; the e2e deployment's gateways are the
//! prologue's own `Gateway::open`.

/// The shared fixtures, by path (see the module docs).
#[path = "../tests/common/mod.rs"]
mod common;

pub(crate) use common::{CreditRecord, Doc, ORIGINAL_TELJ, SUPPLIER, http_client};
use szamlazz_agent::ops::query_xml::{InvoiceDocument, QueryInvoiceXml};
use szamlazz_agent::wire::{AgentRequest as _, RawResponse};
use szamlazz_agent::{Credentials, InvoiceNumber, InvoiceSelector};

use crate::account::Account;
use crate::gateway::{FoundDocument, Gateway};

/// A gateway for `account` with `credentials`, opened as the prologue opens
/// one per execution, over a fresh [`http_client`].
pub(crate) fn open_gateway(account: Account, credentials: Credentials) -> Gateway {
    Gateway::open_with_http(account, credentials, http_client()).expect("gateway")
}

impl Doc<'_> {
    /// The document as the Számla Agent crate parses a query answer, before
    /// the worker's projection: what the builder's own tests read (each field
    /// as the parser sees it), and the seam for a test that needs a value the
    /// renderer cannot put on the wire (a `rendelesszam` the parser would
    /// have trimmed; [`Doc::assigned_order`]) to read what
    /// [`FoundDocument::from`] makes of it.
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

    /// [`Doc::parse`] with `order` assigned to the parsed `rendelesszam`
    /// **after** the agent crate's parser, so the projection's own reading of
    /// the element (trim, empty as none) is exercised with the parser's
    /// normalisation out of the way: the renderer cannot put an untrimmed or
    /// empty element on the wire and have it arrive as such.
    pub(crate) fn assigned_order(&self, order: Option<&str>) -> FoundDocument {
        let mut wire = self.wire();
        wire.info.order_number = order.map(str::to_owned);
        FoundDocument::from(wire)
    }

    /// [`Doc::parse`] boxed, as the gateway outcomes carry a found document.
    pub(crate) fn boxed(&self) -> Box<FoundDocument> {
        Box::new(self.parse())
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
    use jiff::civil::date;
    use rust_decimal::dec;

    use super::*;

    /// The default document is what the worker's checks and derivations read:
    /// a live test-account `SZ-1` of `ORD-1` from [`SUPPLIER`], an e-invoice
    /// with the fulfillment date of [`ORIGINAL_TELJ`], reversed by nobody,
    /// referencing nothing, with no credit entries.
    #[test]
    fn the_default_document_is_a_live_test_invoice_of_ord_1() {
        let document = Doc::default().wire();
        assert_eq!(document.info.invoice_number.as_str(), "SZ-1");
        assert_eq!(
            document.info.document_type,
            szamlazz_agent::DocumentType::Invoice
        );
        assert_eq!(document.info.order_number.as_deref(), Some("ORD-1"));
        assert_eq!(document.info.test, Some(true));
        assert_eq!(document.supplier.id, Some(SUPPLIER));
        assert_eq!(document.info.fulfillment_date, Some(ORIGINAL_TELJ));
        assert_eq!(
            document.info.appearance.code(),
            1,
            "paper, the default create's code"
        );
        assert_eq!(document.info.reversed, None);
        assert_eq!(document.info.referenced_invoice_number, None);
        assert_eq!(document.info.referenced_proforma_number, None);
        assert!(document.credit_entries.is_empty());
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

        let unmanaged = Doc::unmanaged("SZ-1", "SZ").wire();
        assert_eq!(unmanaged.info.order_number, None);
        let of_order = Doc::of("SZ-1", "SZ", "E2E-1").wire();
        assert_eq!(of_order.info.order_number.as_deref(), Some("E2E-1"));

        let no_teszt = Doc {
            test: None,
            ..Doc::default()
        };
        assert!(!no_teszt.xml().contains("<teszt>"), "renders no element");
        assert_eq!(no_teszt.wire().info.test, None);

        let storno = Doc {
            referenced_invoice: Some("SZ-1"),
            ..Doc::new("SS-1", "SS")
        }
        .wire();
        assert_eq!(
            storno.info.document_type,
            szamlazz_agent::DocumentType::Storno
        );
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

    /// `eszamla` follows `tipus` (`0` on a proforma, `1`, paper, on anything
    /// else: the code szamlazz.hu reports for a default create, #73) unless a
    /// test sets the code itself.
    #[test]
    fn eszamla_follows_the_kind_unless_set() {
        assert_eq!(Doc::new("D-1", "D").wire().info.appearance.code(), 0);
        assert_eq!(Doc::new("ES-1", "ES").wire().info.appearance.code(), 1);
        let electronic = Doc {
            eszamla: Some(3),
            ..Doc::default()
        };
        assert_eq!(electronic.wire().info.appearance.code(), 3);
    }

    /// What `get` and the `Szamlazz.Agent.query` projection read beyond the
    /// checks: `kelt`, the totals, the credit entries (`kifizetesek`, each with
    /// its date, title, amount and the optional comment and bank account),
    /// and any further `<alap>` child a test needs verbatim.
    #[test]
    fn dates_totals_credit_entries_and_extra_alap_children_render() {
        let document = Doc {
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
        .wire();

        assert_eq!(document.info.issue_date, Some(date(2026, 7, 4)));
        assert_eq!(document.info.fulfillment_date, Some(date(2026, 7, 4)));
        assert_eq!(document.totals.total.net, dec!(20000));
        assert_eq!(document.totals.total.vat, dec!(5400));
        assert_eq!(document.totals.total.gross, dec!(25400));
        assert_eq!(document.info.due_date, Some(date(2026, 7, 12)));
        assert_eq!(document.info.currency, Some(szamlazz_agent::Currency::HUF));

        let [first, second] = document.credit_entries.as_slice() else {
            panic!("two credit entries, got {:?}", document.credit_entries);
        };
        assert_eq!(first.date, date(2026, 7, 10));
        assert_eq!(first.title, szamlazz_agent::PaymentMethod::Transfer);
        assert_eq!(first.amount, dec!(10000));
        assert_eq!(first.comment.as_deref(), Some("first"));
        assert_eq!(first.bank_account.as_deref(), Some("1234-5678"));
        assert_eq!(second.date, date(2026, 7, 11));
        assert_eq!(second.title, szamlazz_agent::PaymentMethod::Card);
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
