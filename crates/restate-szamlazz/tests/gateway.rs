//! Wiremock tests of the `gateway` module: the lookup and create steps
//! (design §5 steps 3–4), storno validation (§6), deletion, credit entries,
//! credential rejections and transport failures, against synthetic
//! szamlazz.hu responses.

use jiff::civil::date;
use restate_szamlazz::account::{Account, Endpoint};
use restate_szamlazz::config::{AccountMode, Config};
use restate_szamlazz::contract::{
    BuyerInput, DocumentInput, IssuedKind, LineItemInput, PaymentEntry, PaymentMethod, Selector,
};
use restate_szamlazz::gateway::{
    CreateOutcome, CreateStepRequest, DeleteOutcome, DocumentRefs, Gateway,
    InvoiceDocumentExt as _, LookupOutcome, LookupRequest, QueryError, QueryOutcome,
    SetPaymentsOutcome, StornoLookupOutcome, StornoOutcome, StornoStepRequest, Unconfirmed,
};
use restate_szamlazz::{ExternalId, OrderKey};
use rust_decimal::dec;
use serde_json::json;
use szamlazz_agent::ops::invoice::InvoiceCreationResult;
use szamlazz_agent::{Credentials, InvoiceNumber};
use wiremock::matchers::{body_string_contains, method};
use wiremock::{Mock, MockBuilder, MockServer, ResponseTemplate};

const SUPPLIER: u64 = 972_720;

/// The szamlazz.hu codes that mean "the agent credentials are wrong": 3
/// invalid credentials, 135 browser session active, 136 login blocked, 164
/// multiple accounts. Every operation answers them as `CredentialsRejected`.
const CREDENTIAL_CODES: [&str; 4] = ["3", "135", "136", "164"];

// ----- fixtures --------------------------------------------------------------

/// A gateway for a test account pinned to `SUPPLIER`; every found document is
/// validated against those two pins.
fn gateway(server: &MockServer) -> Gateway {
    let config: Config = serde_json::from_value(json!({
        "account": {
            "slug": "acct",
            "agent_key": "key",
            "endpoint": server.uri(),
            "mode": "test",
            "supplier_id": SUPPLIER,
        },
    }))
    .expect("config");
    Gateway::new(&config).expect("gateway")
}

fn order() -> OrderKey {
    OrderKey::parse("ORD-1").expect("order")
}

fn external_id() -> ExternalId {
    ExternalId::new("acct:ORD-1:invoice")
}

fn storno_id() -> ExternalId {
    ExternalId::new("acct:ORD-1:storno:SZ-1")
}

fn document() -> DocumentInput {
    DocumentInput::new(
        BuyerInput::new("Kovács Bt.", "2030", "Érd", "Tárnoki út 23."),
        vec![LineItemInput::new(
            "Elado izé",
            dec!(1),
            "db",
            dec!(1000),
            "27",
        )],
        date(2026, 9, 3),
        date(2026, 9, 11),
        PaymentMethod::Transfer,
    )
}

/// A queried document, rendered as the `szamla` response XML.
struct Doc<'a> {
    number: &'a str,
    tipus: &'a str,
    order: Option<&'a str>,
    reversed: bool,
    referenced_proforma: Option<&'a str>,
    referenced_invoice: Option<&'a str>,
    test: bool,
    supplier_id: u64,
    payments: &'a [&'a str],
}

impl<'a> Doc<'a> {
    const fn new(number: &'a str, tipus: &'a str) -> Self {
        Self {
            number,
            tipus,
            order: Some("ORD-1"),
            reversed: false,
            referenced_proforma: None,
            referenced_invoice: None,
            test: true,
            supplier_id: SUPPLIER,
            payments: &[],
        }
    }

    /// A document of ours that carries `<sztornozott>true</sztornozott>`.
    const fn reversed(number: &'a str, tipus: &'a str) -> Self {
        Self {
            reversed: true,
            ..Self::new(number, tipus)
        }
    }

    fn xml(&self) -> String {
        let eszamla = if self.tipus == "D" { 0 } else { 2 };
        let opt = |tag: &str, value: Option<&str>| {
            value.map_or_else(String::new, |value| format!("<{tag}>{value}</{tag}>"))
        };
        let payments = if self.payments.is_empty() {
            String::new()
        } else {
            let mut entries = String::from("<kifizetesek>");
            for amount in self.payments {
                entries.push_str(
                    "<kifizetes><datum>2026-09-03</datum><jogcim>transfer</jogcim><osszeg>",
                );
                entries.push_str(amount);
                entries.push_str("</osszeg></kifizetes>");
            }
            entries.push_str("</kifizetesek>");
            entries
        };
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<szamla xmlns="http://www.szamlazz.hu/szamla">
  <szallito><id>{supplier}</id><nev>Seller</nev><cim><irsz>1111</irsz><telepules>Budapest</telepules><cim>Fő u. 1.</cim></cim></szallito>
  <alap><id>924307338</id><szamlaszam>{number}</szamlaszam><gazdEsemAzon>924307338</gazdEsemAzon><tipus>{tipus}</tipus><eszamla>{eszamla}</eszamla>{hivszamlaszam}{hivdijbekszam}<kelt>2026-09-03</kelt>{rendelesszam}<teszt>{test}</teszt>{sztornozott}</alap>
  <vevo><nev>Buyer</nev></vevo>
  <tetelek></tetelek>
  <osszegek><totalossz><netto>1000</netto><afa>270</afa><brutto>1270</brutto></totalossz></osszegek>
  {payments}
</szamla>"#,
            supplier = self.supplier_id,
            number = self.number,
            tipus = self.tipus,
            hivszamlaszam = opt("hivszamlaszam", self.referenced_invoice),
            hivdijbekszam = opt("hivdijbekszam", self.referenced_proforma),
            rendelesszam = opt("rendelesszam", self.order),
            test = self.test,
            sztornozott = if self.reversed {
                "<sztornozott>true</sztornozott>"
            } else {
                ""
            },
        )
    }

    fn response(&self) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_raw(self.xml(), "application/xml")
    }
}

/// The body-only code 7 of the XML query.
fn not_found() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(
        r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod><![CDATA[7]]></hibakod><hibauzenet><![CDATA[Hiányzó adat: számla xml (ismeretlen számlaszám, rendelésszám vagy külső azonosító).]]></hibauzenet></xmlszamlavalasz>"#,
        "application/xml",
    )
}

/// A successful create / storno / credit response (`xmlszamlavalasz`).
fn created(number: &str, net: &str, gross: &str) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("szlahu_szamlaszam", number)
        .insert_header("szlahu_id", "924307747")
        .insert_header("szlahu_nettovegosszeg", net)
        .insert_header("szlahu_bruttovegosszeg", gross)
        .insert_header("szlahu_kintlevoseg", gross)
        .set_body_raw(
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><szamlaszam>{number}</szamlaszam><szamlanetto>{net}</szamlanetto><szamlabrutto>{gross}</szamlabrutto><kintlevoseg>{gross}</kintlevoseg></xmlszamlavalasz>"#
            ),
            "application/xml",
        )
}

/// An error of an operation that reports in headers and body.
fn api_error(code: &str, message: &str) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("szlahu_error_code", code)
        .insert_header("szlahu_error", message)
        .set_body_raw(
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod>{code}</hibakod><hibauzenet>{message}</hibauzenet></xmlszamlavalasz>"#
            ),
            "application/xml",
        )
}

/// An error of an operation that reports in the body only.
fn body_error(code: &str, message: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod>{code}</hibakod><hibauzenet>{message}</hibauzenet></xmlszamlavalasz>"#
        ),
        "application/xml",
    )
}

fn op(action: &str) -> MockBuilder {
    Mock::given(method("POST")).and(body_string_contains(format!("name=\"{action}\"")))
}

fn external_id_query(id: &str) -> MockBuilder {
    op("action-szamla_agent_xml").and(body_string_contains(format!(
        "<szamlaKulsoAzon>{id}</szamlaKulsoAzon>"
    )))
}

fn order_query() -> MockBuilder {
    op("action-szamla_agent_xml").and(body_string_contains("<rendelesSzam>ORD-1</rendelesSzam>"))
}

fn number_query(number: &str) -> MockBuilder {
    op("action-szamla_agent_xml").and(body_string_contains(format!(
        "<szamlaszam>{number}</szamlaszam>"
    )))
}

fn create() -> MockBuilder {
    op("action-xmlagentxmlfile")
}

fn storno() -> MockBuilder {
    op("action-szamla_agent_st")
}

fn delete() -> MockBuilder {
    op("action-szamla_agent_dijbekero_torlese")
}

fn credit() -> MockBuilder {
    op("action-szamla_agent_kifiz")
}

struct Harness {
    server: MockServer,
    gateway: Gateway,
}

impl Harness {
    async fn start() -> Self {
        let server = MockServer::start().await;
        let gateway = gateway(&server);
        Self { server, gateway }
    }

    /// The lookup step for an invoice of `ORD-1`.
    async fn lookup(&self, our_numbers: &[String]) -> LookupOutcome {
        self.lookup_kind(IssuedKind::Invoice, &external_id(), our_numbers)
            .await
    }

    async fn lookup_kind(
        &self,
        kind: IssuedKind,
        external_id: &ExternalId,
        our_numbers: &[String],
    ) -> LookupOutcome {
        self.gateway
            .lookup(LookupRequest {
                external_id,
                kind,
                order: &order(),
                our_numbers,
            })
            .await
    }

    /// The create step for an invoice of `ORD-1`; `reversed` is the number
    /// the lookup step saw reversed under the id.
    async fn create(&self, reversed: Option<&str>) -> Result<CreateOutcome, Unconfirmed> {
        self.create_kind(IssuedKind::Invoice, &external_id(), reversed)
            .await
    }

    async fn create_kind(
        &self,
        kind: IssuedKind,
        external_id: &ExternalId,
        reversed: Option<&str>,
    ) -> Result<CreateOutcome, Unconfirmed> {
        let order = order();
        let refs = if kind == IssuedKind::Corrective {
            DocumentRefs {
                corrected: Some("SZ-1"),
                ..DocumentRefs::default()
            }
        } else {
            DocumentRefs::default()
        };
        let create = self
            .gateway
            .build_create(kind, &document(), &order, external_id, refs)
            .expect("build");
        self.gateway
            .create(CreateStepRequest {
                external_id,
                kind,
                order: &order,
                create: &create,
                reversed,
            })
            .await
    }

    async fn bodies(&self) -> Vec<String> {
        self.server
            .received_requests()
            .await
            .expect("requests")
            .iter()
            .map(|request| String::from_utf8_lossy(&request.body).into_owned())
            .collect()
    }
}

// ----- lookup ----------------------------------------------------------------

#[tokio::test]
async fn lookup_with_nothing_under_the_id_or_the_order_is_absent() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;
    order_query()
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;

    assert_eq!(h.lookup(&[]).await, LookupOutcome::Absent);
    assert_eq!(h.bodies().await.len(), 2, "the external id, then the hint");
}

#[tokio::test]
async fn lookup_finds_our_live_document_and_takes_no_hint() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(Doc::new("SZ-1", "SZ").response())
        .expect(1)
        .mount(&h.server)
        .await;
    order_query()
        .respond_with(Doc::new("SZ-77", "SZ").response())
        .expect(0)
        .mount(&h.server)
        .await;

    match h.lookup(&[]).await {
        LookupOutcome::Live(found) => {
            assert_eq!(found.number(), "SZ-1");
            assert_eq!(found.info.document_type, "SZ");
            assert!(found.is_live());
            assert_eq!(found.info.order_number.as_deref(), Some("ORD-1"));
            assert_eq!(found.totals.total.gross, dec!(1270));
            assert_eq!(found.totals.total.net, dec!(1000));
            assert_eq!(found.supplier.id, Some(SUPPLIER));
            assert_eq!(found.buyer.name, "Buyer", "the journaled document is whole");
        }
        other => panic!("expected Live, got {other:?}"),
    }
    assert_eq!(h.bodies().await.len(), 1, "settled without the hint");
}

#[tokio::test]
async fn lookup_of_an_invalid_document_under_our_id_is_a_collision() {
    let other_order = Doc {
        order: Some("ORD-2"),
        ..Doc::new("SZ-9", "SZ")
    };
    let other_kind = Doc::new("D-9", "D");
    let live_account = Doc {
        test: false,
        ..Doc::new("SZ-9", "SZ")
    };
    let other_supplier = Doc {
        supplier_id: 1,
        ..Doc::new("SZ-9", "SZ")
    };
    for (label, doc) in [
        ("order", other_order),
        ("kind", other_kind),
        ("test", live_account),
        ("supplier", other_supplier),
    ] {
        let h = Harness::start().await;
        external_id_query("acct:ORD-1:invoice")
            .respond_with(doc.response())
            .mount(&h.server)
            .await;
        order_query()
            .respond_with(not_found())
            .expect(0)
            .mount(&h.server)
            .await;
        match h.lookup(&[]).await {
            LookupOutcome::Collision(found) => assert_eq!(found.number(), doc.number, "{label}"),
            other => panic!("{label}: expected Collision, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn lookup_of_our_reversed_document_names_its_storno_from_the_hint() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(Doc::reversed("SZ-1", "SZ").response())
        .expect(1)
        .mount(&h.server)
        .await;
    order_query()
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-1"),
                ..Doc::new("SS-1", "SS")
            }
            .response(),
        )
        .expect(1)
        .mount(&h.server)
        .await;

    match h.lookup(&[]).await {
        LookupOutcome::Reversed {
            document,
            storno_number,
        } => {
            assert_eq!(document.number(), "SZ-1");
            assert!(!document.is_live());
            assert_eq!(storno_number.as_deref(), Some("SS-1"));
        }
        other => panic!("expected Reversed, got {other:?}"),
    }
}

#[tokio::test]
async fn lookup_of_our_reversed_document_has_no_storno_number_when_the_hint_is_not_its_storno() {
    // The newest document under the order is the reversed document itself
    // (nothing newer exists) or the storno of another document: no storno
    // number is known, and neither is foreign.
    let same = Doc::reversed("SZ-1", "SZ");
    let other_storno = Doc {
        referenced_invoice: Some("SZ-0"),
        ..Doc::new("SS-0", "SS")
    };
    for (label, hint) in [("same", same), ("other storno", other_storno)] {
        let h = Harness::start().await;
        external_id_query("acct:ORD-1:invoice")
            .respond_with(Doc::reversed("SZ-1", "SZ").response())
            .mount(&h.server)
            .await;
        order_query()
            .respond_with(hint.response())
            .mount(&h.server)
            .await;

        match h.lookup(&[]).await {
            LookupOutcome::Reversed {
                document,
                storno_number,
            } => {
                assert_eq!(document.number(), "SZ-1", "{label}");
                assert_eq!(storno_number, None, "{label}");
            }
            other => panic!("{label}: expected Reversed, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn lookup_reports_a_live_invoice_under_the_order_that_is_not_ours_as_foreign() {
    // Plain foreign; a conversion of our proforma not reachable under our id
    // (not issued by this service, so nothing is adopted); and a foreign
    // document beside our own reversed one, where no create — reissue or
    // not — may proceed.
    let plain = (not_found(), Doc::new("SZ-77", "SZ"), "SZ-77");
    let conversion = (
        not_found(),
        Doc {
            referenced_proforma: Some("D-1"),
            ..Doc::new("SZ-78", "SZ")
        },
        "SZ-78",
    );
    let beside_reversed = (
        Doc::reversed("SZ-1", "SZ").response(),
        Doc::new("ES-79", "ES"),
        "ES-79",
    );
    for (label, (under_id, hint, expected)) in [
        ("plain", plain),
        ("conversion", conversion),
        ("beside reversed", beside_reversed),
    ] {
        let h = Harness::start().await;
        external_id_query("acct:ORD-1:invoice")
            .respond_with(under_id)
            .mount(&h.server)
            .await;
        order_query()
            .respond_with(hint.response())
            .expect(1)
            .mount(&h.server)
            .await;

        match h.lookup(&["D-1".to_owned()]).await {
            LookupOutcome::Foreign(found) => assert_eq!(found.number(), expected, "{label}"),
            other => panic!("{label}: expected Foreign, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn lookup_hint_ignores_our_documents_non_invoices_and_its_own_failure() {
    let ours = Doc::new("D-1", "D");
    let ours_by_number = Doc::new("SZ-1", "SZ");
    let reversed = Doc::reversed("SZ-5", "SZ");
    let storno = Doc {
        referenced_invoice: Some("SZ-5"),
        ..Doc::new("SS-5", "SS")
    };
    let proforma = Doc::new("D-9", "D");
    let cases = [
        ("our proforma", ours.response()),
        ("our number", ours_by_number.response()),
        ("reversed", reversed.response()),
        ("storno", storno.response()),
        ("proforma", proforma.response()),
        ("hint miss", not_found()),
        ("hint error", body_error("57", "malformed")),
    ];
    for (label, hint) in cases {
        let h = Harness::start().await;
        external_id_query("acct:ORD-1:invoice")
            .respond_with(not_found())
            .mount(&h.server)
            .await;
        order_query()
            .respond_with(hint)
            .expect(1)
            .mount(&h.server)
            .await;

        assert_eq!(
            h.lookup(&["D-1".to_owned(), "SZ-1".to_owned()]).await,
            LookupOutcome::Absent,
            "{label}"
        );
    }
}

#[tokio::test]
async fn lookup_reports_a_failed_query_as_transport() {
    // A bare 500 with an empty body parses as `UnexpectedBody` in the agent
    // crate (a `Parse` error), which this layer reports as `Transport`. The
    // external id first; then the hint, whose own transport failure is not
    // conclusive either.
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(ResponseTemplate::new(500))
        .mount(&h.server)
        .await;
    assert!(matches!(
        h.lookup(&[]).await,
        LookupOutcome::Transport(message) if message.contains("empty response")
    ));

    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .mount(&h.server)
        .await;
    order_query()
        .respond_with(ResponseTemplate::new(500))
        .mount(&h.server)
        .await;
    assert!(matches!(h.lookup(&[]).await, LookupOutcome::Transport(_)));
}

#[tokio::test]
async fn lookup_of_a_corrective_takes_no_hint() {
    // The live base invoice under the order is expected, not foreign.
    let h = Harness::start().await;
    let corrective_id = ExternalId::new("acct:ORD-1:corrective:c1");
    external_id_query(corrective_id.as_str())
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;
    order_query()
        .respond_with(Doc::new("SZ-1", "SZ").response())
        .expect(0)
        .mount(&h.server)
        .await;

    assert_eq!(
        h.lookup_kind(IssuedKind::Corrective, &corrective_id, &[])
            .await,
        LookupOutcome::Absent
    );
    assert_eq!(h.bodies().await.len(), 1);
}

#[tokio::test]
async fn corrective_with_a_live_base_under_the_order_is_issued() {
    // Lookup, then create: the base invoice is the newest document under the
    // order throughout and is never queried; the corrective is issued.
    let h = Harness::start().await;
    let corrective_id = ExternalId::new("acct:ORD-1:corrective:c1");
    external_id_query(corrective_id.as_str())
        .respond_with(not_found())
        .expect(2)
        .mount(&h.server)
        .await;
    order_query()
        .respond_with(Doc::new("SZ-1", "SZ").response())
        .expect(0)
        .mount(&h.server)
        .await;
    create()
        .and(body_string_contains(
            "<helyesbitoszamla>true</helyesbitoszamla>",
        ))
        .and(body_string_contains(
            "<helyesbitettSzamlaszam>SZ-1</helyesbitettSzamlaszam>",
        ))
        .respond_with(created("HS-1", "-1000", "-1270"))
        .expect(1)
        .mount(&h.server)
        .await;

    assert_eq!(
        h.lookup_kind(IssuedKind::Corrective, &corrective_id, &[])
            .await,
        LookupOutcome::Absent
    );
    match h
        .create_kind(IssuedKind::Corrective, &corrective_id, None)
        .await
    {
        Ok(CreateOutcome::Issued(issued)) => assert_eq!(number_of(&issued), Some("HS-1")),
        other => panic!("expected Issued, got {other:?}"),
    }
}

// ----- create ----------------------------------------------------------------

/// The issued number of a create result.
fn number_of(issued: &InvoiceCreationResult) -> Option<&str> {
    issued.invoice_number.as_ref().map(InvoiceNumber::as_str)
}

#[tokio::test]
async fn create_with_nothing_under_the_id_sends_the_create_and_is_issued() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;
    create()
        .respond_with(created("SZ-2", "1000", "1270"))
        .expect(1)
        .mount(&h.server)
        .await;

    match h.create(None).await {
        Ok(CreateOutcome::Issued(issued)) => {
            assert_eq!(number_of(&issued), Some("SZ-2"));
            assert_eq!(issued.net_total, Some(dec!(1000)));
            assert_eq!(issued.gross_total, Some(dec!(1270)));
            assert_eq!(issued.outstanding, Some(dec!(1270)));
            assert_eq!(issued.document_id, Some(924_307_747));
            assert!(!issued.notification_delivery_failed);
        }
        other => panic!("expected Issued, got {other:?}"),
    }
    let bodies = h.bodies().await;
    assert_eq!(bodies.len(), 2, "the leading query, then the create");
    let create_body = &bodies[1];
    assert!(create_body.contains("<szamlaKulsoAzon>acct:ORD-1:invoice</szamlaKulsoAzon>"));
    assert!(create_body.contains("<rendelesSzam>ORD-1</rendelesSzam>"));
    assert!(create_body.contains("<szamlaLetoltes>false</szamlaLetoltes>"));
    assert!(create_body.contains("<nev>Kovács Bt.</nev>"));
}

#[tokio::test]
async fn create_re_executed_after_a_lost_reply_finds_the_document_and_sends_nothing() {
    // The step, driven twice: the first execution's reply is lost (500), its
    // immediate re-query still sees nothing, so it is unconfirmed; the second
    // execution's leading query finds the document that landed and sends no
    // create — also when the lookup step had seen a reversed document
    // (`reissue: true`) that this live one is not.
    for (label, reversed) in [("plain", None), ("reissue", Some("SZ-0"))] {
        let h = Harness::start().await;
        external_id_query("acct:ORD-1:invoice")
            .respond_with(not_found())
            .up_to_n_times(2)
            .mount(&h.server)
            .await;
        external_id_query("acct:ORD-1:invoice")
            .respond_with(Doc::new("SZ-1", "SZ").response())
            .mount(&h.server)
            .await;
        create()
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&h.server)
            .await;

        assert!(
            matches!(h.create(reversed).await, Err(Unconfirmed::Transport(_))),
            "{label}: first execution"
        );
        match h.create(reversed).await {
            Ok(CreateOutcome::Found(found)) => assert_eq!(found.number(), "SZ-1", "{label}"),
            other => panic!("{label}: expected Found, got {other:?}"),
        }
        assert_eq!(
            h.bodies().await.len(),
            4,
            "{label}: query, create, re-query; query"
        );
    }
}

#[tokio::test]
async fn create_past_the_reversed_document_the_lookup_saw_sends_the_create() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(Doc::reversed("SZ-1", "SZ").response())
        .expect(1)
        .mount(&h.server)
        .await;
    create()
        .respond_with(created("SZ-2", "1000", "1270"))
        .expect(1)
        .mount(&h.server)
        .await;

    match h.create(Some("SZ-1")).await {
        Ok(CreateOutcome::Issued(issued)) => assert_eq!(number_of(&issued), Some("SZ-2")),
        other => panic!("expected Issued, got {other:?}"),
    }
}

#[tokio::test]
async fn create_never_sends_when_the_leading_query_is_not_a_clean_miss() {
    // A collision under the id settles the step; a failed query leaves it
    // unconfirmed. Neither sends a create.
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(
            Doc {
                order: Some("ORD-2"),
                ..Doc::new("SZ-9", "SZ")
            }
            .response(),
        )
        .mount(&h.server)
        .await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.server)
        .await;
    match h.create(None).await {
        Ok(CreateOutcome::Collision(found)) => assert_eq!(found.number(), "SZ-9"),
        other => panic!("expected Collision, got {other:?}"),
    }

    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(ResponseTemplate::new(500))
        .mount(&h.server)
        .await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.server)
        .await;
    assert!(matches!(
        h.create(None).await,
        Err(Unconfirmed::Transport(message)) if message.contains("empty response")
    ));
}

#[tokio::test]
async fn create_rejection_is_settled_without_a_re_query() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;
    create()
        .respond_with(api_error("259", "net"))
        .mount(&h.server)
        .await;

    assert_eq!(
        h.create(None).await,
        Ok(CreateOutcome::Rejected {
            code: "259".to_owned(),
            message: "net".to_owned(),
        })
    );
}

/// A success without a number: `xmlszamlavalasz` without `<szamlaszam>`,
/// which the agent crate refuses to parse for a create.
fn created_without_a_number() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(
        r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres></xmlszamlavalasz>"#,
        "application/xml",
    )
}

#[tokio::test]
async fn create_with_an_open_outcome_re_queries_once_and_is_unconfirmed_when_nothing_landed() {
    let cases: [(&str, ResponseTemplate, Unconfirmed); 4] = [
        (
            "maintenance",
            api_error("1", "maintenance"),
            Unconfirmed::Open {
                code: Some("1".to_owned()),
                message: "maintenance".to_owned(),
            },
        ),
        (
            "signing",
            api_error("55", "signing"),
            Unconfirmed::Open {
                code: Some("55".to_owned()),
                message: "signing".to_owned(),
            },
        ),
        (
            "down",
            ResponseTemplate::new(503).insert_header("szlahu_down", "maintenance"),
            Unconfirmed::Open {
                code: None,
                message: "maintenance".to_owned(),
            },
        ),
        (
            "no number",
            created_without_a_number(),
            Unconfirmed::Transport("missing szamlaszam in response".to_owned()),
        ),
    ];
    for (label, response, expected) in cases {
        let h = Harness::start().await;
        external_id_query("acct:ORD-1:invoice")
            .respond_with(not_found())
            .expect(2)
            .mount(&h.server)
            .await;
        create().respond_with(response).mount(&h.server).await;

        assert_eq!(h.create(None).await, Err(expected), "{label}");
    }
}

#[tokio::test]
async fn create_with_an_open_outcome_is_found_when_the_re_query_sees_the_document() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(Doc::new("SZ-4", "SZ").response())
        .mount(&h.server)
        .await;
    create()
        .respond_with(api_error("55", "signing"))
        .mount(&h.server)
        .await;

    match h.create(None).await {
        Ok(CreateOutcome::Found(found)) => assert_eq!(found.number(), "SZ-4"),
        other => panic!("expected Found, got {other:?}"),
    }
}

// ----- create: the duplicate order number (71/152) --------------------------

const DUPLICATE_MESSAGE: &str = "M%C3%A1r+l%C3%A9tez%C5%91+rendel%C3%A9ssz%C3%A1m";

/// The leading query misses, the create answers 152, and the re-query sees
/// `under_id`.
async fn duplicate_harness(under_id: ResponseTemplate) -> Harness {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(under_id)
        .mount(&h.server)
        .await;
    create()
        .respond_with(api_error("152", DUPLICATE_MESSAGE))
        .expect(1)
        .mount(&h.server)
        .await;
    h
}

#[tokio::test]
async fn duplicate_order_number_with_our_live_document_under_the_id_is_reconciled() {
    let h = duplicate_harness(Doc::new("SZ-3", "SZ").response()).await;
    order_query()
        .respond_with(not_found())
        .expect(0)
        .mount(&h.server)
        .await;

    match h.create(None).await {
        Ok(CreateOutcome::Reconciled(found)) => assert_eq!(found.number(), "SZ-3"),
        other => panic!("expected Reconciled, got {other:?}"),
    }
}

#[tokio::test]
async fn duplicate_order_number_with_an_invalid_document_under_the_id_is_a_collision() {
    let h = duplicate_harness(
        Doc {
            order: Some("ORD-2"),
            ..Doc::new("SZ-9", "SZ")
        }
        .response(),
    )
    .await;

    match h.create(None).await {
        Ok(CreateOutcome::Collision(found)) => assert_eq!(found.number(), "SZ-9"),
        other => panic!("expected Collision, got {other:?}"),
    }
}

#[tokio::test]
async fn duplicate_order_number_names_the_existing_document_when_our_kind_is_newest() {
    // Nothing under the id, or only our reversed document (a reissue): the
    // live duplicate is not ours; the order-number query names it when it is
    // a live document of the kind being issued.
    let absent = (not_found(), None);
    let reversed = (Doc::reversed("SZ-1", "SZ").response(), Some("SZ-1"));
    for (label, (under_id, reversed)) in [("absent", absent), ("reversed", reversed)] {
        let h = duplicate_harness(under_id).await;
        order_query()
            .respond_with(Doc::new("SZ-77", "SZ").response())
            .expect(1)
            .mount(&h.server)
            .await;

        assert_eq!(
            h.create(reversed).await,
            Ok(CreateOutcome::DuplicateOrderNumber {
                code: "152".to_owned(),
                message: "Már létező rendelésszám".to_owned(),
                existing_number: Some("SZ-77".to_owned()),
            }),
            "{label}"
        );
    }
}

#[tokio::test]
async fn duplicate_order_number_has_no_existing_number_when_another_kind_is_newest() {
    // The newest document under the order is a proforma, a storno, or a
    // reversed document of our kind: none of them is the live duplicate.
    let proforma = Doc::new("D-1", "D");
    let storno = Doc {
        referenced_invoice: Some("SZ-0"),
        ..Doc::new("SS-0", "SS")
    };
    let reversed_of_our_kind = Doc::reversed("SZ-0", "SZ");
    for (label, newest) in [
        ("proforma", proforma),
        ("storno", storno),
        ("reversed", reversed_of_our_kind),
    ] {
        let h = duplicate_harness(not_found()).await;
        order_query()
            .respond_with(newest.response())
            .mount(&h.server)
            .await;

        assert_eq!(
            h.create(None).await,
            Ok(CreateOutcome::DuplicateOrderNumber {
                code: "152".to_owned(),
                message: "Már létező rendelésszám".to_owned(),
                existing_number: None,
            }),
            "{label}"
        );
    }
}

#[tokio::test]
async fn duplicate_order_number_with_nothing_under_the_order_is_unconfirmed() {
    let h = duplicate_harness(not_found()).await;
    order_query()
        .respond_with(not_found())
        .mount(&h.server)
        .await;

    assert_eq!(
        h.create(None).await,
        Err(Unconfirmed::Contradiction {
            code: "152".to_owned(),
            message: "Már létező rendelésszám".to_owned(),
        })
    );
}

#[tokio::test]
async fn duplicate_order_number_on_a_corrective_is_rejected_without_an_order_query() {
    // Correctives are exempt from the order-number check (verified), so a
    // 71/152 the re-query cannot resolve is an ordinary rejection, and the
    // live base invoice under the order is never consulted; a re-query that
    // finds the corrective is still reconciled.
    let corrective_id = ExternalId::new("acct:ORD-1:corrective:c1");

    let h = Harness::start().await;
    external_id_query(corrective_id.as_str())
        .respond_with(not_found())
        .expect(2)
        .mount(&h.server)
        .await;
    order_query()
        .respond_with(Doc::new("SZ-1", "SZ").response())
        .expect(0)
        .mount(&h.server)
        .await;
    create()
        .respond_with(api_error("71", "duplicate"))
        .mount(&h.server)
        .await;
    assert_eq!(
        h.create_kind(IssuedKind::Corrective, &corrective_id, None)
            .await,
        Ok(CreateOutcome::Rejected {
            code: "71".to_owned(),
            message: "duplicate".to_owned(),
        })
    );

    let h = Harness::start().await;
    external_id_query(corrective_id.as_str())
        .respond_with(not_found())
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    external_id_query(corrective_id.as_str())
        .respond_with(Doc::new("HS-1", "HS").response())
        .mount(&h.server)
        .await;
    create()
        .respond_with(api_error("71", "duplicate"))
        .mount(&h.server)
        .await;
    match h
        .create_kind(IssuedKind::Corrective, &corrective_id, None)
        .await
    {
        Ok(CreateOutcome::Reconciled(found)) => assert_eq!(found.number(), "HS-1"),
        other => panic!("expected Reconciled, got {other:?}"),
    }
}

// ----- credentials: lookup and create -----------------------------------------

#[tokio::test]
async fn credential_codes_on_the_lookup_external_id_query_are_credentials_rejected() {
    for code in CREDENTIAL_CODES {
        let h = Harness::start().await;
        external_id_query("acct:ORD-1:invoice")
            .respond_with(body_error(code, "login"))
            .expect(1)
            .mount(&h.server)
            .await;
        order_query()
            .respond_with(Doc::new("SZ-77", "SZ").response())
            .expect(0)
            .mount(&h.server)
            .await;

        assert_eq!(
            h.lookup(&[]).await,
            LookupOutcome::CredentialsRejected {
                code: code.to_owned(),
                message: "login".to_owned(),
            },
            "{code}"
        );
        assert_eq!(
            h.bodies().await.len(),
            1,
            "{code}: no hint after the rejection"
        );
    }
}

#[tokio::test]
async fn credential_codes_on_the_lookup_hint_are_credentials_rejected() {
    // Unlike a miss or another API error, a credential rejection on the hint
    // is conclusive: the lookup does not report `Absent` and nothing proceeds
    // to the create step.
    for code in CREDENTIAL_CODES {
        let h = Harness::start().await;
        external_id_query("acct:ORD-1:invoice")
            .respond_with(not_found())
            .mount(&h.server)
            .await;
        order_query()
            .respond_with(body_error(code, "login"))
            .expect(1)
            .mount(&h.server)
            .await;

        assert_eq!(
            h.lookup(&[]).await,
            LookupOutcome::CredentialsRejected {
                code: code.to_owned(),
                message: "login".to_owned(),
            },
            "{code}"
        );
    }
}

#[tokio::test]
async fn credential_codes_on_the_create_leading_query_never_send() {
    for code in CREDENTIAL_CODES {
        let h = Harness::start().await;
        external_id_query("acct:ORD-1:invoice")
            .respond_with(body_error(code, "login"))
            .expect(1)
            .mount(&h.server)
            .await;
        create()
            .respond_with(created("SZ-X", "1000", "1270"))
            .expect(0)
            .mount(&h.server)
            .await;

        assert_eq!(
            h.create(None).await,
            Ok(CreateOutcome::CredentialsRejected {
                code: code.to_owned(),
                message: "login".to_owned(),
            }),
            "{code}"
        );
    }
}

#[tokio::test]
async fn credential_codes_on_the_create_send_are_settled_without_a_re_query() {
    // Settled data, not `Unconfirmed`: re-executing the step with the same
    // key would only repeat the answer, so the run retry policy must not be
    // spent on it.
    for code in CREDENTIAL_CODES {
        let h = Harness::start().await;
        external_id_query("acct:ORD-1:invoice")
            .respond_with(not_found())
            .expect(1)
            .mount(&h.server)
            .await;
        create()
            .respond_with(api_error(code, "login"))
            .expect(1)
            .mount(&h.server)
            .await;

        assert_eq!(
            h.create(None).await,
            Ok(CreateOutcome::CredentialsRejected {
                code: code.to_owned(),
                message: "login".to_owned(),
            }),
            "{code}"
        );
    }
}

// ----- queries ---------------------------------------------------------------

#[tokio::test]
async fn verify_query_and_hint() {
    let h = Harness::start().await;
    number_query("SZ-1")
        .respond_with(
            Doc {
                payments: &["500", "770"],
                ..Doc::new("SZ-1", "SZ")
            }
            .response(),
        )
        .mount(&h.server)
        .await;
    number_query("SZ-404")
        .respond_with(not_found())
        .mount(&h.server)
        .await;
    number_query("SZ-500")
        .respond_with(ResponseTemplate::new(500))
        .mount(&h.server)
        .await;
    order_query()
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-1"),
                ..Doc::new("SS-1", "SS")
            }
            .response(),
        )
        .mount(&h.server)
        .await;

    match h.gateway.verify("SZ-1").await {
        QueryOutcome::Found(found) => {
            assert_eq!(found.number(), "SZ-1");
            assert_eq!(found.payment_amounts(), vec![dec!(500), dec!(770)]);
        }
        other => panic!("expected Found, got {other:?}"),
    }
    assert_eq!(h.gateway.verify("SZ-404").await, QueryOutcome::NotFound);
    assert!(matches!(
        h.gateway.verify("SZ-500").await,
        QueryOutcome::Transport(_)
    ));
    match h.gateway.hint(&order()).await {
        QueryOutcome::Found(found) => {
            assert_eq!(found.info.document_type, "SS");
            assert!(found.is_storno_of("SZ-1"));
        }
        other => panic!("expected Found, got {other:?}"),
    }
    let document = h
        .gateway
        .query_document(&Selector::InvoiceNumber("SZ-1".to_owned()))
        .await
        .expect("document");
    assert_eq!(document.info.invoice_number.as_str(), "SZ-1");
    assert_eq!(document.payments.len(), 2);
    assert_eq!(
        h.gateway
            .query_document(&Selector::InvoiceNumber("SZ-404".to_owned()))
            .await,
        Err(QueryError::NotFound)
    );
    // No mock matches the external id query: wiremock answers 404 with an
    // empty body, which the agent crate reports as a parse failure.
    assert!(matches!(
        h.gateway
            .query(&Selector::ExternalId("acct:ORD-1:invoice".to_owned()))
            .await,
        QueryOutcome::Transport(_)
    ));
}

// ----- credentials -------------------------------------------------------------

#[tokio::test]
async fn credential_codes_on_a_query_are_credentials_rejected() {
    for code in CREDENTIAL_CODES {
        let h = Harness::start().await;
        op("action-szamla_agent_xml")
            .respond_with(body_error(code, "login"))
            .mount(&h.server)
            .await;

        let expected = QueryOutcome::CredentialsRejected {
            code: code.to_owned(),
            message: "login".to_owned(),
        };
        assert_eq!(h.gateway.verify("SZ-1").await, expected, "verify {code}");
        assert_eq!(h.gateway.hint(&order()).await, expected, "hint {code}");
        assert_eq!(
            h.gateway
                .query(&Selector::ExternalId("acct:ORD-1:invoice".to_owned()))
                .await,
            expected,
            "query {code}"
        );
        assert_eq!(
            h.gateway
                .query_document(&Selector::InvoiceNumber("SZ-1".to_owned()))
                .await,
            Err(QueryError::CredentialsRejected {
                code: code.to_owned(),
                message: "login".to_owned(),
            }),
            "query_document {code}"
        );
    }
}

// ----- storno ----------------------------------------------------------------

fn storno_request(external_id: &ExternalId) -> StornoStepRequest<'_> {
    StornoStepRequest {
        invoice_number: "SZ-1",
        external_id,
        comment: Some("wrong buyer"),
        e_invoice: true,
    }
}

#[tokio::test]
async fn storno_lookup_finds_our_storno_under_the_id() {
    let h = Harness::start().await;
    let storno_id = storno_id();
    external_id_query(storno_id.as_str())
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-1"),
                ..Doc::new("SS-1", "SS")
            }
            .response(),
        )
        .expect(1)
        .mount(&h.server)
        .await;

    assert_eq!(
        h.gateway.lookup_storno(&storno_id, "SZ-1").await,
        StornoLookupOutcome::AlreadyReversed {
            storno_number: "SS-1".to_owned(),
        }
    );
    assert_eq!(h.bodies().await.len(), 1, "read-only: one query");
}

#[tokio::test]
async fn storno_lookup_is_absent_on_a_miss_or_another_holder() {
    // Code 7, and a holder that is not the storno of `SZ-1` (a storno is
    // idempotent server-side, so proceeding past a stray holder is safe).
    let h = Harness::start().await;
    let storno_id = storno_id();
    external_id_query(storno_id.as_str())
        .respond_with(not_found())
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.lookup_storno(&storno_id, "SZ-1").await,
        StornoLookupOutcome::Absent
    );

    let h = Harness::start().await;
    external_id_query(storno_id.as_str())
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-9"),
                ..Doc::new("SS-9", "SS")
            }
            .response(),
        )
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.lookup_storno(&storno_id, "SZ-1").await,
        StornoLookupOutcome::Absent
    );
}

#[tokio::test]
async fn storno_lookup_reports_rejected_credentials_and_a_failed_query() {
    for code in CREDENTIAL_CODES {
        let h = Harness::start().await;
        let storno_id = storno_id();
        external_id_query(storno_id.as_str())
            .respond_with(body_error(code, "login"))
            .mount(&h.server)
            .await;
        assert_eq!(
            h.gateway.lookup_storno(&storno_id, "SZ-1").await,
            StornoLookupOutcome::CredentialsRejected {
                code: code.to_owned(),
                message: "login".to_owned(),
            },
            "{code}"
        );
    }

    let h = Harness::start().await;
    let storno_id = storno_id();
    external_id_query(storno_id.as_str())
        .respond_with(ResponseTemplate::new(500))
        .mount(&h.server)
        .await;
    assert!(matches!(
        h.gateway.lookup_storno(&storno_id, "SZ-1").await,
        StornoLookupOutcome::Transport(_)
    ));
}

#[tokio::test]
async fn storno_reversed_is_validated() {
    let h = Harness::start().await;
    let storno_id = storno_id();
    external_id_query(storno_id.as_str())
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;
    storno()
        .respond_with(created("SS-1", "-1000", "-1270"))
        .expect(1)
        .mount(&h.server)
        .await;

    match h.gateway.storno(storno_request(&storno_id)).await {
        Ok(StornoOutcome::Reversed(storno)) => {
            assert_eq!(storno.invoice_number.as_str(), "SS-1");
            assert_eq!(storno.gross_total, Some(dec!(-1270)));
            assert_eq!(storno.document_id, Some(924_307_747));
        }
        other => panic!("expected Reversed, got {other:?}"),
    }
    let body = &h.bodies().await[1];
    assert!(body.contains("<szamlaszam>SZ-1</szamlaszam>"));
    assert!(body.contains("<szamlaKulsoAzon>acct:ORD-1:storno:SZ-1</szamlaKulsoAzon>"));
    assert!(body.contains("<megjegyzes>wrong buyer</megjegyzes>"));
    assert!(body.contains("<eszamla>true</eszamla>"));
    assert!(!body.contains("<keltDatum>"), "352 otherwise");
}

#[tokio::test]
async fn storno_echo_is_not_stornoable() {
    let h = Harness::start().await;
    let storno_id = storno_id();
    external_id_query(storno_id.as_str())
        .respond_with(not_found())
        .mount(&h.server)
        .await;
    storno()
        .respond_with(created("SZ-1", "1000", "1270"))
        .mount(&h.server)
        .await;

    assert_eq!(
        h.gateway.storno(storno_request(&storno_id)).await,
        Ok(StornoOutcome::NotStornoable)
    );
}

#[tokio::test]
async fn storno_rejections_are_typed() {
    for (code, message) in [("14", "storno of storno"), ("221", "has corrective")] {
        let h = Harness::start().await;
        let storno_id = storno_id();
        external_id_query(storno_id.as_str())
            .respond_with(not_found())
            .mount(&h.server)
            .await;
        storno()
            .respond_with(api_error(code, message))
            .mount(&h.server)
            .await;
        assert_eq!(
            h.gateway.storno(storno_request(&storno_id)).await,
            Ok(StornoOutcome::Rejected {
                code: code.to_owned(),
                message: message.to_owned(),
            })
        );
    }
}

#[tokio::test]
async fn storno_leading_query_hit_is_already_reversed() {
    let h = Harness::start().await;
    let storno_id = storno_id();
    external_id_query(storno_id.as_str())
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-1"),
                ..Doc::new("SS-1", "SS")
            }
            .response(),
        )
        .mount(&h.server)
        .await;
    storno()
        .respond_with(created("SS-2", "-1000", "-1270"))
        .expect(0)
        .mount(&h.server)
        .await;

    assert_eq!(
        h.gateway.storno(storno_request(&storno_id)).await,
        Ok(StornoOutcome::AlreadyReversed {
            storno_number: "SS-1".to_owned(),
        })
    );
}

#[tokio::test]
async fn credential_codes_on_the_storno_are_credentials_rejected() {
    // Both `Szamlazz.Order.storno_invoice` and `Szamlazz.Agent.storno` run
    // this step; the leading query and the send each report the rejection.
    for code in CREDENTIAL_CODES {
        let h = Harness::start().await;
        let storno_id = storno_id();
        external_id_query(storno_id.as_str())
            .respond_with(body_error(code, "login"))
            .expect(1)
            .mount(&h.server)
            .await;
        storno()
            .respond_with(created("SS-1", "-1000", "-1270"))
            .expect(0)
            .mount(&h.server)
            .await;
        assert_eq!(
            h.gateway.storno(storno_request(&storno_id)).await,
            Ok(StornoOutcome::CredentialsRejected {
                code: code.to_owned(),
                message: "login".to_owned(),
            }),
            "leading query {code}"
        );

        let h = Harness::start().await;
        external_id_query(storno_id.as_str())
            .respond_with(not_found())
            .mount(&h.server)
            .await;
        storno()
            .respond_with(api_error(code, "login"))
            .expect(1)
            .mount(&h.server)
            .await;
        assert_eq!(
            h.gateway.storno(storno_request(&storno_id)).await,
            Ok(StornoOutcome::CredentialsRejected {
                code: code.to_owned(),
                message: "login".to_owned(),
            }),
            "send {code}"
        );
    }
}

#[tokio::test]
async fn storno_with_a_lost_reply_re_queries_once_and_is_unconfirmed_when_nothing_landed() {
    // An open code (55) and a lost reply (500): each is re-queried once,
    // immediately; nothing under the storno id leaves the step unconfirmed,
    // so the run retry policy re-executes it.
    let h = Harness::start().await;
    let storno_id = storno_id();
    external_id_query(storno_id.as_str())
        .respond_with(not_found())
        .expect(4)
        .mount(&h.server)
        .await;
    storno()
        .respond_with(api_error("55", "signing"))
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    storno()
        .respond_with(ResponseTemplate::new(500))
        .mount(&h.server)
        .await;

    assert_eq!(
        h.gateway.storno(storno_request(&storno_id)).await,
        Err(Unconfirmed::Open {
            code: Some("55".to_owned()),
            message: "signing".to_owned(),
        })
    );
    assert!(matches!(
        h.gateway.storno(storno_request(&storno_id)).await,
        Err(Unconfirmed::Transport(_))
    ));
}

#[tokio::test]
async fn storno_re_executed_after_a_lost_reply_finds_the_storno_and_sends_nothing() {
    // The step, driven twice: the first execution's reply is lost, its
    // immediate re-query still sees nothing; the second execution's leading
    // query finds the storno that landed and sends nothing.
    let h = Harness::start().await;
    let storno_id = storno_id();
    external_id_query(storno_id.as_str())
        .respond_with(not_found())
        .up_to_n_times(2)
        .mount(&h.server)
        .await;
    external_id_query(storno_id.as_str())
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-1"),
                ..Doc::new("SS-1", "SS")
            }
            .response(),
        )
        .mount(&h.server)
        .await;
    storno()
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&h.server)
        .await;

    assert!(matches!(
        h.gateway.storno(storno_request(&storno_id)).await,
        Err(Unconfirmed::Transport(_))
    ));
    assert_eq!(
        h.gateway.storno(storno_request(&storno_id)).await,
        Ok(StornoOutcome::AlreadyReversed {
            storno_number: "SS-1".to_owned(),
        })
    );
    assert_eq!(h.bodies().await.len(), 4, "query, storno, re-query; query");
}

#[tokio::test]
async fn storno_lost_reply_whose_re_query_finds_the_storno_is_reversed() {
    // The reply is lost but the storno landed: the immediate re-query finds
    // the `SS` and settles the step without a second send.
    let h = Harness::start().await;
    let storno_id = storno_id();
    external_id_query(storno_id.as_str())
        .respond_with(not_found())
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    external_id_query(storno_id.as_str())
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-1"),
                ..Doc::new("SS-1", "SS")
            }
            .response(),
        )
        .mount(&h.server)
        .await;
    storno()
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&h.server)
        .await;

    assert_eq!(
        h.gateway.storno(storno_request(&storno_id)).await,
        Ok(StornoOutcome::AlreadyReversed {
            storno_number: "SS-1".to_owned(),
        })
    );
}

// ----- delete / credit -------------------------------------------------------

#[tokio::test]
async fn delete_proforma_outcomes() {
    let h = Harness::start().await;
    let ok = r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamladbkdelvalasz xmlns="http://www.szamlazz.hu/xmlszamladbkdelvalasz"><sikeres>true</sikeres></xmlszamladbkdelvalasz>"#;
    let gone = r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamladbkdelvalasz xmlns="http://www.szamlazz.hu/xmlszamladbkdelvalasz"><sikeres>false</sikeres><hibakod>335</hibakod><hibauzenet>Nincs ilyen díjbekérő</hibauzenet></xmlszamladbkdelvalasz>"#;
    delete()
        .and(body_string_contains("<szamlaszam>D-1</szamlaszam>"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(ok, "application/xml"))
        .mount(&h.server)
        .await;
    delete()
        .and(body_string_contains("<szamlaszam>D-2</szamlaszam>"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("szlahu_error_code", "335")
                .insert_header("szlahu_error", "Nincs+ilyen")
                .set_body_raw(gone, "application/xml"),
        )
        .mount(&h.server)
        .await;
    delete()
        .and(body_string_contains("<szamlaszam>D-3</szamlaszam>"))
        .respond_with(api_error("3", "login"))
        .mount(&h.server)
        .await;
    delete()
        .and(body_string_contains("<szamlaszam>D-4</szamlaszam>"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&h.server)
        .await;
    delete()
        .and(body_string_contains("<szamlaszam>D-5</szamlaszam>"))
        .respond_with(api_error("57", "malformed"))
        .mount(&h.server)
        .await;

    assert_eq!(
        h.gateway.delete_proforma("D-1").await,
        DeleteOutcome::Deleted
    );
    assert_eq!(
        h.gateway.delete_proforma("D-2").await,
        DeleteOutcome::AlreadyGone
    );
    assert_eq!(
        h.gateway.delete_proforma("D-3").await,
        DeleteOutcome::CredentialsRejected {
            code: "3".to_owned(),
            message: "login".to_owned(),
        }
    );
    assert!(matches!(
        h.gateway.delete_proforma("D-4").await,
        DeleteOutcome::Transport(_)
    ));
    assert_eq!(
        h.gateway.delete_proforma("D-5").await,
        DeleteOutcome::Rejected {
            code: "57".to_owned(),
            message: "malformed".to_owned(),
        }
    );
}

#[tokio::test]
async fn credential_codes_on_the_delete_are_credentials_rejected() {
    for code in CREDENTIAL_CODES {
        let h = Harness::start().await;
        delete()
            .respond_with(api_error(code, "login"))
            .expect(1)
            .mount(&h.server)
            .await;
        assert_eq!(
            h.gateway.delete_proforma("D-1").await,
            DeleteOutcome::CredentialsRejected {
                code: code.to_owned(),
                message: "login".to_owned(),
            },
            "{code}"
        );
    }
}

#[tokio::test]
async fn credential_codes_on_set_payments_are_credentials_rejected() {
    let entry = PaymentEntry {
        date: date(2026, 9, 3),
        method: PaymentMethod::Card,
        amount: dec!(1000),
        description: None,
    };
    for code in CREDENTIAL_CODES {
        let h = Harness::start().await;
        credit()
            .respond_with(body_error(code, "login"))
            .expect(1)
            .mount(&h.server)
            .await;
        assert_eq!(
            h.gateway
                .set_payments("SZ-1", std::slice::from_ref(&entry), false)
                .await,
            SetPaymentsOutcome::CredentialsRejected {
                code: code.to_owned(),
                message: "login".to_owned(),
            },
            "{code}"
        );
    }
}

#[tokio::test]
async fn set_payments_outcomes() {
    let h = Harness::start().await;
    credit()
        .and(body_string_contains("<szamlaszam>SZ-1</szamlaszam>"))
        .respond_with(created("SZ-1", "1000", "1270").insert_header("szlahu_kintlevoseg", "270"))
        .mount(&h.server)
        .await;
    credit()
        .and(body_string_contains("<szamlaszam>SZ-2</szamlaszam>"))
        .respond_with(body_error("463", "reversed"))
        .mount(&h.server)
        .await;
    credit()
        .and(body_string_contains("<szamlaszam>SZ-3</szamlaszam>"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&h.server)
        .await;
    let entry = PaymentEntry {
        date: date(2026, 9, 3),
        method: PaymentMethod::Card,
        amount: dec!(1000),
        description: Some("card".to_owned()),
    };

    match h
        .gateway
        .set_payments("SZ-1", std::slice::from_ref(&entry), true)
        .await
    {
        SetPaymentsOutcome::Done { outstanding, gross } => {
            // The body's <kintlevoseg> takes precedence over the header.
            assert_eq!(outstanding, Some(dec!(1270)));
            assert_eq!(gross, Some(dec!(1270)));
        }
        other => panic!("expected Done, got {other:?}"),
    }
    let body = &h.bodies().await[0];
    assert!(body.contains("<additiv>true</additiv>"));
    assert!(body.contains("<jogcim>bankkártya</jogcim>"));
    assert!(body.contains("<osszeg>1000</osszeg>"));
    assert!(body.contains("<leiras>card</leiras>"));

    assert_eq!(
        h.gateway
            .set_payments("SZ-2", std::slice::from_ref(&entry), false)
            .await,
        SetPaymentsOutcome::Rejected {
            code: "463".to_owned(),
            message: "reversed".to_owned(),
        }
    );
    assert!(matches!(
        h.gateway
            .set_payments("SZ-3", std::slice::from_ref(&entry), false)
            .await,
        SetPaymentsOutcome::Transport(_)
    ));
    let six = vec![entry; 6];
    assert!(matches!(
        h.gateway.set_payments("SZ-9", &six, false).await,
        SetPaymentsOutcome::Rejected { code, .. } if code == "request"
    ));
    assert_eq!(
        h.bodies().await.len(),
        3,
        "six entries never reach the wire"
    );
}

// ----- Gateway::open ---------------------------------------------------------

/// An [`Account`] in `mode`, pinned to `SUPPLIER`, on `server`, and the
/// gateway opened for it with `key`.
fn open(server: &MockServer, id: &str, key: &str, mode: AccountMode) -> Gateway {
    let mut account = Account::new(id, id);
    account.mode = mode;
    account.supplier_id = Some(SUPPLIER);
    account.endpoint = Endpoint::parse(&server.uri()).expect("endpoint");
    Gateway::open(account, Credentials::agent_key(key)).expect("gateway")
}

fn agent_key_on_the_wire(body: &str) -> Option<&str> {
    let start = body.find("<szamlaagentkulcs>")? + "<szamlaagentkulcs>".len();
    let end = body[start..].find("</szamlaagentkulcs>")? + start;
    Some(&body[start..end])
}

#[tokio::test]
async fn a_gateway_opened_from_an_account_sends_that_accounts_key() {
    let server = MockServer::start().await;
    external_id_query("acme:ORD-1:invoice")
        .respond_with(not_found())
        .mount(&server)
        .await;
    let gateway = open(&server, "acme", "key-acme", AccountMode::Test);

    let outcome = gateway
        .query(&Selector::ExternalId("acme:ORD-1:invoice".to_owned()))
        .await;
    assert!(matches!(outcome, QueryOutcome::NotFound), "{outcome:?}");

    let sent = server.received_requests().await.expect("requests");
    assert_eq!(sent.len(), 1);
    let body = String::from_utf8_lossy(&sent[0].body);
    assert_eq!(agent_key_on_the_wire(&body), Some("key-acme"));
    assert_eq!(gateway.account().id.as_str(), "acme");
}

/// Two accounts on one szamlazz.hu: each gateway's requests carry its own
/// key, and a session cookie szamlazz.hu sets for the first never travels
/// with the second — a fresh client per gateway. The first gateway's second
/// request *does* carry the cookie, proving the cookie store is live and the
/// test would catch a shared client.
#[tokio::test]
async fn two_gateways_opened_from_two_accounts_share_no_key_and_no_session() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(not_found().insert_header("set-cookie", "JSESSIONID=session-of-acme; Path=/"))
        .mount(&server)
        .await;
    let acme = open(&server, "acme", "key-acme", AccountMode::Test);
    let beta = open(&server, "beta", "key-beta", AccountMode::Test);

    let selector = Selector::OrderNumber("ORD-1".to_owned());
    assert!(matches!(
        acme.query(&selector).await,
        QueryOutcome::NotFound
    ));
    assert!(matches!(
        beta.query(&selector).await,
        QueryOutcome::NotFound
    ));
    assert!(matches!(
        acme.query(&selector).await,
        QueryOutcome::NotFound
    ));

    let sent = server.received_requests().await.expect("requests");
    assert_eq!(sent.len(), 3);
    let cookie = |i: usize| {
        sent[i]
            .headers
            .get("cookie")
            .map(|value| value.to_str().expect("ascii").to_owned())
    };
    let key = |i: usize| {
        agent_key_on_the_wire(&String::from_utf8_lossy(&sent[i].body)).map(str::to_owned)
    };

    assert_eq!(key(0).as_deref(), Some("key-acme"));
    assert_eq!(cookie(0), None, "acme's first request: no session yet");
    assert_eq!(key(1).as_deref(), Some("key-beta"));
    assert_eq!(cookie(1), None, "beta never saw acme's Set-Cookie");
    assert_eq!(key(2).as_deref(), Some("key-acme"));
    assert_eq!(
        cookie(2).as_deref(),
        Some("JSESSIONID=session-of-acme"),
        "acme's own client keeps its own session"
    );
}

/// The account's mode is always validated: a document under our external id
/// that says `teszt=false` is not ours on a test account (and vice versa),
/// so the lookup step reports a collision instead of adopting it.
#[tokio::test]
async fn opened_gateway_validates_the_accounts_mode_against_teszt() {
    for (mode, teszt, label) in [
        (AccountMode::Test, false, "test account, live document"),
        (AccountMode::Live, true, "live account, test document"),
    ] {
        let server = MockServer::start().await;
        let external_id = ExternalId::new("acme:ORD-1:invoice");
        external_id_query("acme:ORD-1:invoice")
            .respond_with(
                Doc {
                    test: teszt,
                    ..Doc::new("SZ-9", "SZ")
                }
                .response(),
            )
            .mount(&server)
            .await;
        order_query()
            .respond_with(not_found())
            .expect(0)
            .mount(&server)
            .await;
        let gateway = open(&server, "acme", "key-acme", mode);

        let outcome = gateway
            .lookup(LookupRequest {
                external_id: &external_id,
                kind: IssuedKind::Invoice,
                order: &order(),
                our_numbers: &[],
            })
            .await;
        match outcome {
            LookupOutcome::Collision(found) => {
                assert_eq!(found.number(), "SZ-9", "{label}");
                assert!(
                    !found.is_ours(
                        &order(),
                        IssuedKind::Invoice,
                        mode.is_test(),
                        Some(SUPPLIER)
                    ),
                    "{label}"
                );
            }
            other => panic!("{label}: expected Collision, got {other:?}"),
        }
    }
}
