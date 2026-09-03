//! Wiremock tests of the `steps` module: the issue closure
//! (design §6 step 4), storno validation (§7), deletion, credit entries and
//! transport failures, against synthetic szamlazz.hu responses.

use std::sync::Arc;

use jiff::civil::date;
use restate_szamlazz::config::Config;
use restate_szamlazz::contract::{
    BuyerInput, DocumentInput, IssuedKind, LineItemInput, PaymentEntry, PaymentMethod, Selector,
};
use restate_szamlazz::steps::{
    DeleteOutcome, DocumentRefs, IssueOutcome, IssueRequest, QueryError, QueryOutcome,
    SetPaymentsOutcome, Steps, StornoAttempt, StornoOutcome,
};
use restate_szamlazz::{ExternalId, OrderKey};
use rust_decimal::dec;
use serde_json::json;
use wiremock::matchers::{body_string_contains, method};
use wiremock::{Mock, MockBuilder, MockServer, ResponseTemplate};

const SUPPLIER: u64 = 972_720;

// ----- fixtures --------------------------------------------------------------

fn steps(server: &MockServer) -> Steps {
    let config: Config = serde_json::from_value(json!({
        "account": {
            "slug": "acct",
            "agent_key": "key",
            "fp_secret": "fp",
            "endpoint": server.uri(),
            "mode": "test",
            "supplier_id": SUPPLIER,
        },
    }))
    .expect("config");
    Steps::new(Arc::new(config)).expect("steps")
}

fn order() -> OrderKey {
    OrderKey::parse("ORD-1").expect("order")
}

fn external_id() -> ExternalId {
    ExternalId::new("acct:ORD-1:invoice:0")
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
    steps: Steps,
}

impl Harness {
    async fn start() -> Self {
        let server = MockServer::start().await;
        let steps = steps(&server);
        Self { server, steps }
    }

    async fn issue(
        &self,
        check_hint: bool,
        our_numbers: &[String],
        our_proforma: Option<&str>,
    ) -> IssueOutcome {
        let order = order();
        let external_id = external_id();
        let create = self
            .steps
            .build_create(
                IssuedKind::Invoice,
                &document(),
                &order,
                &external_id,
                DocumentRefs {
                    proforma: our_proforma,
                    ..DocumentRefs::default()
                },
            )
            .expect("build");
        self.steps
            .issue(IssueRequest {
                external_id: &external_id,
                kind: IssuedKind::Invoice,
                order: &order,
                create: &create,
                check_hint,
                our_numbers,
                our_proforma,
                expect_supplier_id: Some(SUPPLIER),
                expect_test: true,
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

// ----- issue -----------------------------------------------------------------

#[tokio::test]
async fn pre_query_hit_is_found_and_nothing_is_created() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice:0")
        .respond_with(Doc::new("SZ-1", "SZ").response())
        .expect(1)
        .mount(&h.server)
        .await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.server)
        .await;

    match h.issue(true, &[], None).await {
        IssueOutcome::Found(found) => {
            assert_eq!(found.number, "SZ-1");
            assert_eq!(found.document_type, "SZ");
            assert_eq!(found.reversed, None);
            assert!(found.is_live());
            assert_eq!(found.order_number.as_deref(), Some("ORD-1"));
            assert_eq!(found.gross, Some(dec!(1270)));
            assert_eq!(found.net, Some(dec!(1000)));
            assert!(found.test);
            assert_eq!(found.e_invoice, Some(true));
            assert_eq!(found.supplier_id, Some(SUPPLIER));
            assert_eq!(found.document_id, Some(924_307_338));
            assert!(!found.adopted);
        }
        other => panic!("expected Found, got {other:?}"),
    }
    assert_eq!(h.bodies().await.len(), 1, "no hint query after a hit");
}

#[tokio::test]
async fn pre_query_miss_then_create_is_issued() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice:0")
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;
    create()
        .respond_with(created("SZ-2", "1000", "1270"))
        .expect(1)
        .mount(&h.server)
        .await;

    let outcome = h.issue(false, &[], None).await;
    match outcome {
        IssueOutcome::Issued(issued) => {
            assert_eq!(issued.number, "SZ-2");
            assert_eq!(issued.net, Some(dec!(1000)));
            assert_eq!(issued.gross, Some(dec!(1270)));
            assert_eq!(issued.outstanding, Some(dec!(1270)));
            assert_eq!(issued.document_id, Some(924_307_747));
            assert!(!issued.notification_delivery_failed);
        }
        other => panic!("expected Issued, got {other:?}"),
    }
    let bodies = h.bodies().await;
    assert_eq!(bodies.len(), 2, "no hint query when check_hint is false");
    let create_body = &bodies[1];
    assert!(create_body.contains("<szamlaKulsoAzon>acct:ORD-1:invoice:0</szamlaKulsoAzon>"));
    assert!(create_body.contains("<rendelesSzam>ORD-1</rendelesSzam>"));
    assert!(create_body.contains("<szamlaLetoltes>false</szamlaLetoltes>"));
    assert!(create_body.contains("<nev>Kovács Bt.</nev>"));
}

#[tokio::test]
async fn duplicate_order_number_then_requery_hit_is_found() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice:0")
        .respond_with(not_found())
        .up_to_n_times(1)
        .expect(1)
        .mount(&h.server)
        .await;
    external_id_query("acct:ORD-1:invoice:0")
        .respond_with(Doc::new("SZ-3", "SZ").response())
        .expect(1)
        .mount(&h.server)
        .await;
    create()
        .respond_with(api_error(
            "152",
            "M%C3%A1r+l%C3%A9tez%C5%91+rendel%C3%A9ssz%C3%A1m",
        ))
        .expect(1)
        .mount(&h.server)
        .await;

    match h.issue(false, &[], None).await {
        IssueOutcome::Found(found) => assert_eq!(found.number, "SZ-3"),
        other => panic!("expected Found, got {other:?}"),
    }
}

#[tokio::test]
async fn duplicate_order_number_then_requery_miss_is_duplicate() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice:0")
        .respond_with(not_found())
        .expect(2)
        .mount(&h.server)
        .await;
    create()
        .respond_with(api_error(
            "152",
            "M%C3%A1r+l%C3%A9tez%C5%91+rendel%C3%A9ssz%C3%A1m",
        ))
        .expect(1)
        .mount(&h.server)
        .await;

    assert_eq!(
        h.issue(false, &[], None).await,
        IssueOutcome::DuplicateOrderNumber {
            code: "152".to_owned(),
            message: "Már létező rendelésszám".to_owned(),
        }
    );
}

#[tokio::test]
async fn invalid_documents_under_our_external_id_are_collisions() {
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
        external_id_query("acct:ORD-1:invoice:0")
            .respond_with(doc.response())
            .mount(&h.server)
            .await;
        create()
            .respond_with(created("SZ-X", "1000", "1270"))
            .expect(0)
            .mount(&h.server)
            .await;
        match h.issue(true, &[], None).await {
            IssueOutcome::Collision(found) => assert_eq!(found.number, doc.number, "{label}"),
            other => panic!("{label}: expected Collision, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn live_invoice_under_the_order_is_foreign() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice:0")
        .respond_with(not_found())
        .mount(&h.server)
        .await;
    order_query()
        .respond_with(Doc::new("SZ-77", "SZ").response())
        .expect(1)
        .mount(&h.server)
        .await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.server)
        .await;

    match h.issue(true, &["SZ-1".to_owned()], None).await {
        IssueOutcome::Foreign(found) => {
            assert_eq!(found.number, "SZ-77");
            assert_eq!(found.document_type, "SZ");
            assert!(!found.adopted);
        }
        other => panic!("expected Foreign, got {other:?}"),
    }
}

#[tokio::test]
async fn conversion_of_our_proforma_is_adopted() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice:0")
        .respond_with(not_found())
        .mount(&h.server)
        .await;
    order_query()
        .respond_with(
            Doc {
                referenced_proforma: Some("D-1"),
                ..Doc::new("SZ-78", "SZ")
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

    match h.issue(true, &["D-1".to_owned()], Some("D-1")).await {
        IssueOutcome::Found(found) => {
            assert_eq!(found.number, "SZ-78");
            assert_eq!(found.referenced_proforma.as_deref(), Some("D-1"));
            assert!(found.adopted);
        }
        other => panic!("expected adopted Found, got {other:?}"),
    }
}

#[tokio::test]
async fn hint_ignores_our_reversed_and_non_invoice_documents() {
    let ours = Doc::new("SZ-1", "SZ");
    let reversed = Doc {
        reversed: true,
        ..Doc::new("SZ-5", "SZ")
    };
    let storno = Doc {
        referenced_invoice: Some("SZ-5"),
        ..Doc::new("SS-5", "SS")
    };
    let proforma = Doc::new("D-1", "D");
    for (label, doc) in [
        ("ours", ours),
        ("reversed", reversed),
        ("storno", storno),
        ("proforma", proforma),
    ] {
        let h = Harness::start().await;
        external_id_query("acct:ORD-1:invoice:0")
            .respond_with(not_found())
            .mount(&h.server)
            .await;
        order_query()
            .respond_with(doc.response())
            .expect(1)
            .mount(&h.server)
            .await;
        create()
            .respond_with(created("SZ-6", "1000", "1270"))
            .expect(1)
            .mount(&h.server)
            .await;
        match h.issue(true, &["SZ-1".to_owned()], None).await {
            IssueOutcome::Issued(issued) => assert_eq!(issued.number, "SZ-6", "{label}"),
            other => panic!("{label}: expected Issued, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn hint_miss_or_error_continues_to_create() {
    for response in [not_found(), body_error("3", "login")] {
        let h = Harness::start().await;
        external_id_query("acct:ORD-1:invoice:0")
            .respond_with(not_found())
            .mount(&h.server)
            .await;
        order_query()
            .respond_with(response)
            .expect(1)
            .mount(&h.server)
            .await;
        create()
            .respond_with(created("SZ-6", "1000", "1270"))
            .expect(1)
            .mount(&h.server)
            .await;
        assert!(matches!(
            h.issue(true, &[], None).await,
            IssueOutcome::Issued(_)
        ));
    }
}

#[tokio::test]
async fn create_rejections_and_unknowns_are_classified() {
    let cases: [(ResponseTemplate, IssueOutcome); 3] = [
        (
            api_error("259", "net"),
            IssueOutcome::Rejected {
                code: "259".to_owned(),
                message: "net".to_owned(),
            },
        ),
        (
            api_error("1", "maintenance"),
            IssueOutcome::Unknown {
                code: Some("1".to_owned()),
                message: "maintenance".to_owned(),
            },
        ),
        (
            ResponseTemplate::new(503).insert_header("szlahu_down", "maintenance"),
            IssueOutcome::Unknown {
                code: None,
                message: "maintenance".to_owned(),
            },
        ),
    ];
    for (response, expected) in cases {
        let h = Harness::start().await;
        external_id_query("acct:ORD-1:invoice:0")
            .respond_with(not_found())
            .mount(&h.server)
            .await;
        create()
            .respond_with(response)
            .expect(1)
            .mount(&h.server)
            .await;
        assert_eq!(h.issue(false, &[], None).await, expected);
    }
}

#[tokio::test]
async fn failed_pre_query_never_creates() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice:0")
        .respond_with(ResponseTemplate::new(500))
        .mount(&h.server)
        .await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.server)
        .await;

    // A bare 500 with an empty body parses as `UnexpectedBody` in the agent
    // crate (a `Parse` error), which this layer reports as `Transport`.
    assert!(matches!(
        h.issue(true, &[], None).await,
        IssueOutcome::Transport(message) if message.contains("empty response")
    ));
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

    match h.steps.verify("SZ-1").await {
        QueryOutcome::Found(found) => {
            assert_eq!(found.number, "SZ-1");
            assert_eq!(found.payments, vec![dec!(500), dec!(770)]);
        }
        other => panic!("expected Found, got {other:?}"),
    }
    assert_eq!(h.steps.verify("SZ-404").await, QueryOutcome::NotFound);
    assert!(matches!(
        h.steps.verify("SZ-500").await,
        QueryOutcome::Transport(_)
    ));
    match h.steps.hint(&order()).await {
        QueryOutcome::Found(found) => {
            assert_eq!(found.document_type, "SS");
            assert_eq!(found.referenced_invoice.as_deref(), Some("SZ-1"));
        }
        other => panic!("expected Found, got {other:?}"),
    }
    let document = h
        .steps
        .query_document(&Selector::InvoiceNumber("SZ-1".to_owned()))
        .await
        .expect("document");
    assert_eq!(document.info.invoice_number.as_str(), "SZ-1");
    assert_eq!(document.payments.len(), 2);
    assert_eq!(
        h.steps
            .query_document(&Selector::InvoiceNumber("SZ-404".to_owned()))
            .await,
        Err(QueryError::NotFound)
    );
    // No mock matches the external id query: wiremock answers 404 with an
    // empty body, which the agent crate reports as a parse failure.
    assert!(matches!(
        h.steps
            .query(&Selector::ExternalId("acct:ORD-1:invoice:0".to_owned()))
            .await,
        QueryOutcome::Transport(_)
    ));
}

// ----- storno ----------------------------------------------------------------

fn storno_attempt(external_id: &ExternalId) -> StornoAttempt<'_> {
    StornoAttempt {
        invoice_number: "SZ-1",
        external_id,
        comment: Some("wrong buyer"),
        e_invoice: true,
    }
}

#[tokio::test]
async fn storno_reversed_is_validated() {
    let h = Harness::start().await;
    let storno_id = external_id().storno_of();
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

    assert_eq!(
        h.steps.storno(storno_attempt(&storno_id)).await,
        StornoOutcome::Reversed {
            storno_number: "SS-1".to_owned(),
            gross: Some(dec!(-1270)),
            document_id: Some(924_307_747),
        }
    );
    let body = &h.bodies().await[1];
    assert!(body.contains("<szamlaszam>SZ-1</szamlaszam>"));
    assert!(body.contains("<szamlaKulsoAzon>acct:ORD-1:invoice:0:storno</szamlaKulsoAzon>"));
    assert!(body.contains("<megjegyzes>wrong buyer</megjegyzes>"));
    assert!(body.contains("<eszamla>true</eszamla>"));
    assert!(!body.contains("<keltDatum>"), "352 otherwise");
}

#[tokio::test]
async fn storno_echo_is_not_stornoable() {
    let h = Harness::start().await;
    let storno_id = external_id().storno_of();
    external_id_query(storno_id.as_str())
        .respond_with(not_found())
        .mount(&h.server)
        .await;
    storno()
        .respond_with(created("SZ-1", "1000", "1270"))
        .mount(&h.server)
        .await;

    assert_eq!(
        h.steps.storno(storno_attempt(&storno_id)).await,
        StornoOutcome::NotStornoable
    );
}

#[tokio::test]
async fn storno_rejections_are_typed() {
    for (code, message) in [("14", "storno of storno"), ("221", "has corrective")] {
        let h = Harness::start().await;
        let storno_id = external_id().storno_of();
        external_id_query(storno_id.as_str())
            .respond_with(not_found())
            .mount(&h.server)
            .await;
        storno()
            .respond_with(api_error(code, message))
            .mount(&h.server)
            .await;
        assert_eq!(
            h.steps.storno(storno_attempt(&storno_id)).await,
            StornoOutcome::Rejected {
                code: code.to_owned(),
                message: message.to_owned(),
            }
        );
    }
}

#[tokio::test]
async fn storno_pre_query_hit_is_already_reversed() {
    let h = Harness::start().await;
    let storno_id = external_id().storno_of();
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
        h.steps.storno(storno_attempt(&storno_id)).await,
        StornoOutcome::AlreadyReversed {
            storno_number: "SS-1".to_owned(),
        }
    );
}

#[tokio::test]
async fn storno_unknown_and_transport() {
    let h = Harness::start().await;
    let storno_id = external_id().storno_of();
    external_id_query(storno_id.as_str())
        .respond_with(not_found())
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
        h.steps.storno(storno_attempt(&storno_id)).await,
        StornoOutcome::Unknown {
            code: Some("55".to_owned()),
            message: "signing".to_owned(),
        }
    );
    assert!(matches!(
        h.steps.storno(storno_attempt(&storno_id)).await,
        StornoOutcome::Transport(_)
    ));
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

    assert_eq!(h.steps.delete_proforma("D-1").await, DeleteOutcome::Deleted);
    assert_eq!(
        h.steps.delete_proforma("D-2").await,
        DeleteOutcome::AlreadyGone
    );
    assert_eq!(
        h.steps.delete_proforma("D-3").await,
        DeleteOutcome::Rejected {
            code: "3".to_owned(),
            message: "login".to_owned(),
        }
    );
    assert!(matches!(
        h.steps.delete_proforma("D-4").await,
        DeleteOutcome::Transport(_)
    ));
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
        .steps
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
        h.steps
            .set_payments("SZ-2", std::slice::from_ref(&entry), false)
            .await,
        SetPaymentsOutcome::Rejected {
            code: "463".to_owned(),
            message: "reversed".to_owned(),
        }
    );
    assert!(matches!(
        h.steps
            .set_payments("SZ-3", std::slice::from_ref(&entry), false)
            .await,
        SetPaymentsOutcome::Transport(_)
    ));
    let six = vec![entry; 6];
    assert!(matches!(
        h.steps.set_payments("SZ-9", &six, false).await,
        SetPaymentsOutcome::Rejected { code, .. } if code == "request"
    ));
    assert_eq!(
        h.bodies().await.len(),
        3,
        "six entries never reach the wire"
    );
}
