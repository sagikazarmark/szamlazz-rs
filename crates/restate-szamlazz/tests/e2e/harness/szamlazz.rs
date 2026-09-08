//! What szamlazz.hu holds, as wiremock stubs (mirroring `tests/gateway.rs`):
//! the document fixture ([`Doc`]) rendered as one `<szamla>` body, the
//! selector matchers (by number, order number, external id; the create,
//! storno, credit and delete operations), the response templates (code 7,
//! a created document, an API code, …) and the document-centric helpers
//! ([`holds`] and its siblings) that mount one body on every selector the
//! document is reachable by, so the stubs cannot disagree. The
//! helpers' own tests, against wiremock alone, close the file.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use jiff::civil::{Date, date};
use serde_json::{Value, json};
use szamlazz_agent::reqwest;
use tokio::sync::watch;
use wiremock::matchers::{body_string_contains, method};
use wiremock::{Mock, MockBuilder, MockServer, Request, ResponseTemplate};

use crate::harness::plain_http;

/// The `szallito/id` the rendered documents carry: the seller record's id as
/// szamlazz.hu prints it in a query body (972720 on the test account). Wire
/// realism only: the worker holds no account pin; the scenario that renders
/// [`SUPPLIER_B`] asserts exactly that.
const SUPPLIER: u64 = 972_720;

/// Another seller record's id: what a document of another szamlazz.hu account
/// would carry. Compared with nothing.
pub(crate) const SUPPLIER_B: u64 = 972_721;

/// The `telj` every document of the run carries unless a scenario says
/// otherwise: the fulfillment date a storno of it must repeat.
const ORIGINAL_TELJ: Date = date(2026, 7, 15);

pub(crate) struct Doc<'a> {
    pub(crate) number: &'a str,
    pub(crate) tipus: &'a str,
    pub(crate) order: Option<&'a str>,
    pub(crate) reversed: bool,
    pub(crate) referenced_invoice: Option<&'a str>,
    pub(crate) referenced_proforma: Option<&'a str>,
    /// `teszt`: whether a test account issued the document. Projected by
    /// `query`, compared with nothing.
    pub(crate) test: bool,
    /// `szallito/id`: the seller record's id in the `<szallito>` block.
    /// Parsed, compared with nothing.
    pub(crate) supplier_id: u64,
    /// The external id the document sits under, when the test states it:
    /// szamlazz.hu never echoes it, so it is not in the body, but it is a
    /// selector the document is reachable by ([`holds`]).
    pub(crate) external_id: Option<&'a str>,
    /// `telj`; `None` renders no element: szamlazz.hu breaking its schema.
    pub(crate) fulfillment_date: Option<Date>,
    /// `eszamla`; `None` follows `tipus`: `0` on a proforma, `2` (an
    /// e-invoice code) on anything else. szamlazz.hu reports `1` for a paper
    /// invoice and `3` for one created with `eszamla=true` (P73).
    pub(crate) eszamla: Option<i32>,
    /// The registered credit entries (`kifizetesek`), by amount; empty
    /// renders no element. What makes a proforma *paid* for
    /// `delete_proforma`.
    pub(crate) payments: &'a [&'a str],
}

impl<'a> Doc<'a> {
    /// A live test-account document of `order`.
    pub(crate) const fn new(number: &'a str, tipus: &'a str, order: &'a str) -> Self {
        Self {
            order: Some(order),
            ..Self::unmanaged(number, tipus)
        }
    }

    /// A live test-account document carrying no order number: issued outside
    /// the worker, reachable by number only.
    pub(crate) const fn unmanaged(number: &'a str, tipus: &'a str) -> Self {
        Self {
            number,
            tipus,
            order: None,
            reversed: false,
            referenced_invoice: None,
            referenced_proforma: None,
            test: true,
            supplier_id: SUPPLIER,
            external_id: None,
            fulfillment_date: Some(ORIGINAL_TELJ),
            eszamla: None,
            payments: &[],
        }
    }

    pub(crate) fn response(&self) -> ResponseTemplate {
        let opt = |tag: &str, value: Option<&str>| {
            value.map_or_else(String::new, |value| format!("<{tag}>{value}</{tag}>"))
        };
        let eszamla = self
            .eszamla
            .unwrap_or(if self.tipus == "D" { 0 } else { 2 });
        let telj = self.fulfillment_date.map(|date| date.to_string());
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
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<szamla xmlns="http://www.szamlazz.hu/szamla">
  <szallito><id>{supplier_id}</id><nev>Seller</nev><cim><irsz>1111</irsz><telepules>Budapest</telepules><cim>Fő u. 1.</cim></cim></szallito>
  <alap><id>924307338</id><szamlaszam>{number}</szamlaszam><tipus>{tipus}</tipus><eszamla>{eszamla}</eszamla>{hivszamlaszam}{hivdijbekszam}<kelt>2026-09-03</kelt>{telj}{rendelesszam}<teszt>{test}</teszt>{sztornozott}</alap>
  <vevo><nev>Buyer</nev></vevo>
  <tetelek></tetelek>
  <osszegek><totalossz><netto>1000</netto><afa>270</afa><brutto>1270</brutto></totalossz></osszegek>
  {payments}
</szamla>"#,
            supplier_id = self.supplier_id,
            number = self.number,
            tipus = self.tipus,
            hivszamlaszam = opt("hivszamlaszam", self.referenced_invoice),
            hivdijbekszam = opt("hivdijbekszam", self.referenced_proforma),
            telj = opt("telj", telj.as_deref()),
            rendelesszam = opt("rendelesszam", self.order),
            test = self.test,
            sztornozott = if self.reversed {
                "<sztornozott>true</sztornozott>"
            } else {
                ""
            },
        );
        ResponseTemplate::new(200).set_body_raw(xml, "application/xml")
    }
}

pub(crate) fn not_found() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(
        r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod><![CDATA[7]]></hibakod><hibauzenet><![CDATA[Hiányzó adat]]></hibauzenet></xmlszamlavalasz>"#,
        "application/xml",
    )
}

pub(crate) fn created(number: &str, net: &str, gross: &str) -> ResponseTemplate {
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

pub(crate) fn api_error(code: &str, message: &str) -> ResponseTemplate {
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

/// szamlazz.hu's 152 on a create: the order number already exists on another
/// document, naming the order and never the existing document's number.
pub(crate) fn duplicate_order_number(order: &str) -> ResponseTemplate {
    api_error(
        "152",
        &format!(
            "Már létező rendelésszám: {order}. Az ismétlődés engedélyezhető a Beállítások oldalon."
        ),
    )
}

/// The credit-entry operation's success: the invoice's totals after the
/// update, with `outstanding` (`kintlevoseg`) distinct from `gross` so that
/// the response's field mapping is observable.
pub(crate) fn credited(number: &str, gross: &str, outstanding: &str) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("szlahu_szamlaszam", number)
        .insert_header("szlahu_bruttovegosszeg", gross)
        .insert_header("szlahu_kintlevoseg", outstanding)
        .set_body_raw(
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><szamlaszam>{number}</szamlaszam><szamlanetto>1000</szamlanetto><szamlabrutto>{gross}</szamlabrutto><kintlevoseg>{outstanding}</kintlevoseg></xmlszamlavalasz>"#
            ),
            "application/xml",
        )
}

/// The proforma deletion's success (`xmlszamladbkdelvalasz`).
pub(crate) fn proforma_deleted() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(
        r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamladbkdelvalasz xmlns="http://www.szamlazz.hu/xmlszamladbkdelvalasz"><sikeres>true</sikeres></xmlszamladbkdelvalasz>"#,
        "application/xml",
    )
}

/// The proforma deletion's code 335 (no such proforma: deleted already), in
/// headers and body as szamlazz.hu reports it.
pub(crate) fn proforma_gone() -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("szlahu_error_code", "335")
        .insert_header("szlahu_error", "Nincs+ilyen+d%C3%ADjbek%C3%A9r%C5%91")
        .set_body_raw(
            r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamladbkdelvalasz xmlns="http://www.szamlazz.hu/xmlszamladbkdelvalasz"><sikeres>false</sikeres><hibakod>335</hibakod><hibauzenet>Nincs ilyen díjbekérő</hibauzenet></xmlszamladbkdelvalasz>"#,
            "application/xml",
        )
}

fn op(action: &str) -> MockBuilder {
    Mock::given(method("POST")).and(body_string_contains(format!("name=\"{action}\"")))
}

pub(crate) fn external_id_query(id: &str) -> MockBuilder {
    op("action-szamla_agent_xml").and(body_string_contains(format!(
        "<szamlaKulsoAzon>{id}</szamlaKulsoAzon>"
    )))
}

pub(crate) fn order_query(order: &str) -> MockBuilder {
    op("action-szamla_agent_xml").and(body_string_contains(format!(
        "<rendelesSzam>{order}</rendelesSzam>"
    )))
}

pub(crate) fn number_query(number: &str) -> MockBuilder {
    op("action-szamla_agent_xml").and(body_string_contains(format!(
        "<szamlaszam>{number}</szamlaszam>"
    )))
}

pub(crate) fn create() -> MockBuilder {
    op("action-xmlagentxmlfile")
}

/// The `<szamlaagentkulcs>` element carrying `agent_key`: what tells one
/// account's traffic from another's on the wire.
pub(crate) fn agent_key_tag(agent_key: &str) -> String {
    format!("<szamlaagentkulcs>{agent_key}</szamlaagentkulcs>")
}

/// A create request carrying `agent_key`.
pub(crate) fn create_with_key(agent_key: &str) -> MockBuilder {
    create().and(body_string_contains(agent_key_tag(agent_key)))
}

/// A create request whose seller block carries `bank_account`.
pub(crate) fn create_with_bank_account(bank_account: &str) -> MockBuilder {
    create().and(body_string_contains(format!(
        "<bankszamlaszam>{bank_account}</bankszamlaszam>"
    )))
}

pub(crate) fn storno() -> MockBuilder {
    op("action-szamla_agent_st")
}

/// The credit-entry operation (`set_payments`).
pub(crate) fn credit() -> MockBuilder {
    op("action-szamla_agent_kifiz")
}

/// The proforma deletion (`delete_proforma`) of `number`.
pub(crate) fn delete_of(number: &str) -> MockBuilder {
    op("action-szamla_agent_dijbekero_torlese").and(body_string_contains(format!(
        "<szamlaszam>{number}</szamlaszam>"
    )))
}

/// The `<teljesitesDatum>` element carrying [`ORIGINAL_TELJ`]: the storno
/// repeating the original's fulfillment date.
pub(crate) fn original_telj_tag() -> String {
    format!("<teljesitesDatum>{ORIGINAL_TELJ}</teljesitesDatum>")
}

/// A storno request carrying the fixture's `telj` ([`ORIGINAL_TELJ`]) as its
/// `teljesitesDatum`: what every storno of a fixture document must send.
pub(crate) fn storno_repeating_telj() -> MockBuilder {
    storno().and(body_string_contains(original_telj_tag()))
}

/// The body of a `storno_invoice` / `Szamlazz.Agent.storno` call on `number`.
pub(crate) fn storno_of(number: &str) -> Value {
    json!({ "invoice_number": number })
}

/// A storno request that must not reach szamlazz.hu: a handler that stops
/// before sending.
pub(crate) async fn storno_never_sent(mock: &MockServer) {
    storno()
        .respond_with(created("SS-X", "-1000", "-1270"))
        .expect(0)
        .mount(mock)
        .await;
}

/// The sentinel external id `check_account` probes under the run's namespace.
const PROBE_ID: &str = "acct:check-account";

/// The probe's query carrying `agent_key`: which account's key was checked.
pub(crate) fn probe_with_key(agent_key: &str) -> MockBuilder {
    external_id_query(PROBE_ID).and(body_string_contains(agent_key_tag(agent_key)))
}

/// The taxpayer query (`xmltaxpayer`) of `prefix` carrying `agent_key`:
/// which account's key NAV was asked with.
pub(crate) fn taxpayer_query_with_key(prefix: &str, agent_key: &str) -> MockBuilder {
    op("action-szamla_agent_taxpayer")
        .and(body_string_contains(format!(
            "<torzsszam>{prefix}</torzsszam>"
        )))
        .and(body_string_contains(agent_key_tag(agent_key)))
}

/// NAV's answer for a known taxpayer (the agent crate's synthetic fixture).
pub(crate) fn taxpayer_known() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<QueryTaxpayerResponse xmlns="http://schemas.nav.gov.hu/OSA/2.0/api" xmlns:d="http://schemas.nav.gov.hu/OSA/2.0/data">
  <result><funcCode>OK</funcCode></result><taxpayerValidity>true</taxpayerValidity>
  <taxpayerData><taxpayerName>SYNTHETIC SOFTWARE KFT.</taxpayerName>
    <taxNumberDetail><d:taxpayerId>12345678</d:taxpayerId><d:vatCode>2</d:vatCode></taxNumberDetail>
    <taxpayerAddressList><taxpayerAddressItem><taxpayerAddressType>HQ</taxpayerAddressType><taxpayerAddress>
      <d:countryCode>HU</d:countryCode><d:postalCode>1111</d:postalCode><d:city>TESTVAROS</d:city>
      <d:streetName>MINTA</d:streetName><d:publicPlaceCategory>UTCA</d:publicPlaceCategory><d:number>1.</d:number>
    </taxpayerAddress></taxpayerAddressItem></taxpayerAddressList>
  </taxpayerData>
</QueryTaxpayerResponse>"#,
        "application/xml",
    )
}

/// NAV's answer for a prefix it knows no taxpayer under.
pub(crate) fn taxpayer_unknown() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<QueryTaxpayerResponse xmlns="http://schemas.nav.gov.hu/OSA/2.0/api"><result><funcCode>OK</funcCode></result><taxpayerValidity>false</taxpayerValidity></QueryTaxpayerResponse>"#,
        "application/xml",
    )
}

// ----- what szamlazz.hu holds: one document, every selector ------------------

/// szamlazz.hu holds `doc`: one body on every selector the document is
/// reachable by (its number, its order number when it carries one, its
/// external id when the test states one), so the stubs cannot disagree.
pub(crate) async fn holds(mock: &MockServer, doc: &Doc<'_>) {
    let selectors = [
        Some(number_query(doc.number)),
        doc.order.map(order_query),
        doc.external_id.map(external_id_query),
    ];
    for selector in selectors.into_iter().flatten() {
        selector.respond_with(doc.response()).mount(mock).await;
    }
}

/// The next external-id query for `id` loses its reply (a 500 with no body)
/// once; whatever is mounted after this answers from the second query on:
/// wiremock takes the first active match in mount order, and an
/// `up_to_n_times(1)` mock is inactive after its one match.
pub(crate) async fn loses_reply_once(mock: &MockServer, id: &str) {
    external_id_query(id)
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .mount(mock)
        .await;
}

/// szamlazz.hu holds `doc` under its external id from the `misses + 1`th
/// query on: code 7 for `misses` queries, the document afterwards. The number
/// and order selectors are not mounted: the document is absent before the
/// misses and nothing reads it by number or order after. `doc` must state its
/// external id.
pub(crate) async fn holds_after_misses(mock: &MockServer, misses: u64, doc: &Doc<'_>) {
    let id = doc
        .external_id
        .expect("holds_after_misses needs the document's external id");
    external_id_query(id)
        .respond_with(not_found())
        .up_to_n_times(misses)
        .mount(mock)
        .await;
    external_id_query(id)
        .respond_with(doc.response())
        .mount(mock)
        .await;
}

/// The create lands on szamlazz.hu but its reply is lost: `create()` answers
/// 500, `expect(1)`, and `doc` is the holder of its external id from the
/// moment the create request is received (code 7 before, the document after).
/// The transition is the create stub being matched (one flag, flipped by the
/// create's responder and read by the external id's), so how many queries
/// precede the send is not the test's to know. `doc` must state its external
/// id; the number and order selectors are not mounted.
pub(crate) async fn create_lands_but_reply_lost(mock: &MockServer, doc: &Doc<'_>) {
    let _sends = create_lands_when(mock, doc, vec![(ResponseTemplate::new(500), true)]).await;
}

/// The create lands and is answered, but its reply takes `delay` to arrive:
/// `doc` is the holder of its external id from the moment the create request
/// is **received** (code 7 before), while its sender is still waiting for
/// szamlazz.hu's answer. wiremock records the request and runs the responder
/// at receipt and sleeps the delay outside its lock, so a query in that window
/// finds the document at once. The window a second caller on the same order
/// or a cancellation arrives in, and the returned [`Sends`] is how a scenario
/// knows the window is open. `expect(1)`; `doc` must state its external id;
/// the number and order selectors are not mounted.
pub(crate) async fn create_lands_slowly(
    mock: &MockServer,
    doc: &Doc<'_>,
    delay: Duration,
) -> Sends {
    create_lands_when(
        mock,
        doc,
        vec![(created(doc.number, "1000", "1270").set_delay(delay), true)],
    )
    .await
}

/// The first create request is answered `first` **without landing** (a reply
/// to which szamlazz.hu did not act: `szlahu_down`, a 500) and the document
/// stays absent; the second lands, is answered `created` at once, and `doc` is
/// the holder of its external id from that request's receipt. What a create
/// step that re-executes after an *Unconfirmed* first send meets. `expect(2)`;
/// `doc` must state its external id; the number and order selectors are not
/// mounted.
pub(crate) async fn create_lands_on_the_second_send(
    mock: &MockServer,
    doc: &Doc<'_>,
    first: ResponseTemplate,
) -> Sends {
    create_lands_when(
        mock,
        doc,
        vec![(first, false), (created(doc.number, "1000", "1270"), true)],
    )
    .await
}

/// How many create requests the stub has **received**, as a signal a scenario
/// awaits: the moment szamlazz.hu has the send and its reply is still on its
/// way (a delayed stub), or the moment the first of two sends is answered. The
/// count moves at the responder, which wiremock runs at receipt, before any
/// delay: a transport-side fact, not a guess from the client's clock. The
/// document's landing is published **with** the count (both under the one
/// write that notifies the receiver), so a query made after `received`
/// resolves finds what the counted request landed.
#[derive(Clone)]
pub(crate) struct Sends(watch::Receiver<u64>);

impl Sends {
    /// How long [`received`](Self::received) waits before failing the
    /// scenario.
    const DEADLINE: Duration = Duration::from_secs(30);

    /// Resolves once the stub has received at least `count` create requests;
    /// fails the scenario when it has not within [`Self::DEADLINE`].
    pub(crate) async fn received(&mut self, count: u64) {
        let received = self.0.wait_for(|seen| *seen >= count);
        tokio::time::timeout(Self::DEADLINE, received)
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "szamlazz.hu did not receive {count} create request(s) within {:?}",
                    Self::DEADLINE
                )
            })
            .expect("the create stub outlives the scenario");
    }
}

/// szamlazz.hu holds `doc` under its external id from the moment a create
/// request **lands** (code 7 before). `answers` are szamlazz.hu's replies to
/// the create requests in the order they are received, each with whether that
/// request lands: the flag flips at its receipt, whatever the reply or its
/// delay, since a document exists on szamlazz.hu once the send reaches it.
/// The create stub expects exactly `answers.len()` requests; one beyond the
/// list is answered 500 without landing and fails the `expect` at the next
/// `reset`. Failure-injection sequencing, not a model of szamlazz.hu: one flag
/// for one document. The three helpers above are its callers; the [`Sends`]
/// counts the requests as they arrive.
async fn create_lands_when(
    mock: &MockServer,
    doc: &Doc<'_>,
    answers: Vec<(ResponseTemplate, bool)>,
) -> Sends {
    let id = doc
        .external_id
        .expect("a landing create needs the document's external id");
    let landed = Arc::new(AtomicBool::new(false));
    let flip = Arc::clone(&landed);
    let expected = u64::try_from(answers.len()).expect("a few answers");
    let (sends, received) = watch::channel(0u64);
    create()
        .respond_with(move |_: &Request| {
            // The answer is chosen and the landing recorded under the same
            // write that publishes the count: a receiver woken by the count
            // sees the document landed, never the count ahead of it.
            let mut reply = None;
            sends.send_modify(|count| {
                let index = usize::try_from(*count).expect("a few sends");
                reply = Some(match answers.get(index) {
                    Some((template, lands)) => {
                        if *lands {
                            flip.store(true, Ordering::SeqCst);
                        }
                        template.clone()
                    }
                    None => ResponseTemplate::new(500),
                });
                *count += 1;
            });
            reply.expect("chosen under the write")
        })
        .expect(expected)
        .mount(mock)
        .await;
    let document = doc.response();
    external_id_query(id)
        .respond_with(move |_: &Request| {
            if landed.load(Ordering::SeqCst) {
                document.clone()
            } else {
                not_found()
            }
        })
        .mount(mock)
        .await;
    Sends(received)
}

// ----- the harness's stub helpers, against wiremock alone -----------------------

/// The client the raw posts below go through: [`plain_http`], built.
fn http() -> reqwest::Client {
    plain_http().build().expect("http client")
}

/// A query as the Számla Agent client puts it on the wire, reduced to what
/// the selector matchers read: the operation's field name and the one
/// selector element.
async fn query_by(mock: &MockServer, selector: &str) -> (u16, String) {
    let response = http()
        .post(mock.uri())
        .body(format!("name=\"action-szamla_agent_xml\"\n{selector}"))
        .send()
        .await
        .expect("query");
    let status = response.status().as_u16();
    (status, response.text().await.expect("body"))
}

/// `holds` mounts one body on every selector the document is reachable by and
/// nothing else: with an order and an external id three stubs, without an
/// order two, without either one; an unmounted selector is wiremock's 404.
#[tokio::test]
async fn holds_answers_every_selector_the_document_is_reachable_by_with_one_body() {
    let mock = MockServer::start().await;
    holds(
        &mock,
        &Doc {
            external_id: Some("acct:ORD-1:invoice"),
            ..Doc::new("SZ-1", "SZ", "ORD-1")
        },
    )
    .await;
    holds(&mock, &Doc::new("SZ-2", "SZ", "ORD-2")).await;
    holds(&mock, &Doc::unmanaged("SZ-3", "SZ")).await;

    let mut bodies = Vec::new();
    for selector in [
        "<szamlaszam>SZ-1</szamlaszam>",
        "<rendelesSzam>ORD-1</rendelesSzam>",
        "<szamlaKulsoAzon>acct:ORD-1:invoice</szamlaKulsoAzon>",
    ] {
        let (status, body) = query_by(&mock, selector).await;
        assert_eq!(status, 200, "{selector}");
        assert!(
            body.contains("<szamlaszam>SZ-1</szamlaszam>"),
            "{selector}: {body}"
        );
        bodies.push(body);
    }
    assert!(
        bodies.iter().all(|body| body == &bodies[0]),
        "the three selectors answer one body"
    );

    for selector in [
        "<szamlaszam>SZ-2</szamlaszam>",
        "<rendelesSzam>ORD-2</rendelesSzam>",
    ] {
        let (status, body) = query_by(&mock, selector).await;
        assert_eq!(status, 200, "{selector}");
        assert!(
            body.contains("<szamlaszam>SZ-2</szamlaszam>"),
            "{selector}: {body}"
        );
    }
    let (status, _) = query_by(
        &mock,
        "<szamlaKulsoAzon>acct:ORD-2:invoice</szamlaKulsoAzon>",
    )
    .await;
    assert_eq!(status, 404, "no external id was stated: no stub");

    let (status, body) = query_by(&mock, "<szamlaszam>SZ-3</szamlaszam>").await;
    assert_eq!(status, 200);
    assert!(body.contains("<szamlaszam>SZ-3</szamlaszam>"), "{body}");
    let (status, _) = query_by(&mock, "<rendelesSzam>ORD-3</rendelesSzam>").await;
    assert_eq!(
        status, 404,
        "an unmanaged document is reachable by number only"
    );
}

/// `holds_after_misses(n, doc)` answers the document's external id with code
/// 7 exactly `n` times and the document from then on; the number and order
/// selectors are not mounted: the document is absent before the misses.
#[tokio::test]
async fn holds_after_misses_answers_code_7_n_times_then_the_document() {
    let mock = MockServer::start().await;
    holds_after_misses(
        &mock,
        2,
        &Doc {
            external_id: Some("acct:ORD-4:invoice"),
            reversed: true,
            ..Doc::new("SZ-4", "SZ", "ORD-4")
        },
    )
    .await;

    let by_id = "<szamlaKulsoAzon>acct:ORD-4:invoice</szamlaKulsoAzon>";
    for miss in 1..=2 {
        let (status, body) = query_by(&mock, by_id).await;
        assert_eq!(status, 200, "miss {miss}");
        assert!(
            body.contains("<hibakod><![CDATA[7]]></hibakod>"),
            "miss {miss}: {body}"
        );
    }
    for hit in 1..=2 {
        let (status, body) = query_by(&mock, by_id).await;
        assert_eq!(status, 200, "hit {hit}");
        assert!(
            body.contains("<szamlaszam>SZ-4</szamlaszam>"),
            "hit {hit}: {body}"
        );
        assert!(
            body.contains("<sztornozott>true</sztornozott>"),
            "hit {hit}: {body}"
        );
    }
    let (status, _) = query_by(&mock, "<szamlaszam>SZ-4</szamlaszam>").await;
    assert_eq!(status, 404, "the number selector is not mounted");
    let (status, _) = query_by(&mock, "<rendelesSzam>ORD-4</rendelesSzam>").await;
    assert_eq!(status, 404, "the order selector is not mounted");
}

/// `create_lands_but_reply_lost(doc)` answers the document's external id with
/// code 7 until the create request is received (however many queries precede
/// it), and with the document from that moment on; the create itself is a
/// 500. The transition is the create stub being matched, not a query count.
#[tokio::test]
async fn create_lands_but_reply_lost_makes_the_document_the_holder_on_the_create_hit() {
    let mock = MockServer::start().await;
    create_lands_but_reply_lost(
        &mock,
        &Doc {
            external_id: Some("acct:ORD-5:invoice"),
            ..Doc::new("SZ-5", "SZ", "ORD-5")
        },
    )
    .await;

    let by_id = "<szamlaKulsoAzon>acct:ORD-5:invoice</szamlaKulsoAzon>";
    for query in 1..=5 {
        let (status, body) = query_by(&mock, by_id).await;
        assert_eq!(status, 200, "query {query} before the create");
        assert!(
            body.contains("<hibakod><![CDATA[7]]></hibakod>"),
            "query {query} before the create: {body}"
        );
    }
    let response = http()
        .post(mock.uri())
        .body("name=\"action-xmlagentxmlfile\"\n<xmlszamla/>")
        .send()
        .await
        .expect("create");
    assert_eq!(response.status().as_u16(), 500, "the reply is lost");
    for query in 1..=2 {
        let (status, body) = query_by(&mock, by_id).await;
        assert_eq!(status, 200, "query {query} after the create");
        assert!(
            body.contains("<szamlaszam>SZ-5</szamlaszam>"),
            "query {query} after the create: {body}"
        );
    }
}

/// `create_lands_slowly(doc, delay)` makes the document the holder of its
/// external id the moment the create request is *received*, while the create's
/// own reply is still `delay` away: the [`Sends`] resolves at that receipt
/// with the send's reply still outstanding, a query then finds the document,
/// and the reply, when it comes, is the created document, no sooner than the
/// delay. The order is read off the send task (not finished when the signal
/// resolves) and the delay off a lower bound; no upper bound on the clock,
/// which a loaded host would break without the stub being wrong.
#[tokio::test]
async fn create_lands_slowly_makes_the_document_the_holder_while_the_reply_is_in_flight() {
    const DELAY: Duration = Duration::from_secs(2);
    let mock = MockServer::start().await;
    let mut sends = create_lands_slowly(
        &mock,
        &Doc {
            external_id: Some("acct:ORD-6:invoice"),
            ..Doc::new("SZ-6", "SZ", "ORD-6")
        },
        DELAY,
    )
    .await;

    let by_id = "<szamlaKulsoAzon>acct:ORD-6:invoice</szamlaKulsoAzon>";
    let (_, body) = query_by(&mock, by_id).await;
    assert!(
        body.contains("<hibakod><![CDATA[7]]></hibakod>"),
        "absent before the create: {body}"
    );

    let started = Instant::now();
    let send = tokio::spawn({
        let uri = mock.uri();
        async move {
            http()
                .post(uri)
                .body("name=\"action-xmlagentxmlfile\"\n<xmlszamla/>")
                .send()
                .await
                .expect("create")
        }
    });
    // The signal resolves at receipt: the request is recorded and the send
    // has no reply yet. The landing was published with the count, so the
    // query finds the document whenever it is made.
    sends.received(1).await;
    assert!(
        !send.is_finished(),
        "the receipt was signalled while the send's reply was still outstanding"
    );
    assert_eq!(mock.received_requests().await.expect("requests").len(), 2);
    let (_, body) = query_by(&mock, by_id).await;
    assert!(
        body.contains("<szamlaszam>SZ-6</szamlaszam>"),
        "the holder from the receipt on: {body}"
    );

    let response = send.await.expect("join");
    assert!(
        started.elapsed() >= DELAY,
        "the reply waited the delay: {:?}",
        started.elapsed()
    );
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(
        response
            .headers()
            .get("szlahu_szamlaszam")
            .and_then(|value| value.to_str().ok()),
        Some("SZ-6"),
        "the created document"
    );
}

/// `create_lands_on_the_second_send(doc, first)` answers the first create
/// request `first` without landing (the document stays absent), and the
/// second `created`, the document being the holder of its external id from
/// the second request's receipt; the [`Sends`] counts both.
#[tokio::test]
async fn create_lands_on_the_second_send_keeps_the_document_absent_until_the_second_create() {
    let mock = MockServer::start().await;
    let mut sends = create_lands_on_the_second_send(
        &mock,
        &Doc {
            external_id: Some("acct:ORD-7:invoice"),
            ..Doc::new("SZ-7", "SZ", "ORD-7")
        },
        ResponseTemplate::new(503).insert_header("szlahu_down", "maintenance"),
    )
    .await;

    let by_id = "<szamlaKulsoAzon>acct:ORD-7:invoice</szamlaKulsoAzon>";
    let create = || {
        http()
            .post(mock.uri())
            .body("name=\"action-xmlagentxmlfile\"\n<xmlszamla/>")
            .send()
    };
    let first = create().await.expect("first create");
    assert_eq!(first.status().as_u16(), 503, "the first answer");
    assert!(first.headers().contains_key("szlahu_down"));
    sends.received(1).await;
    let (_, body) = query_by(&mock, by_id).await;
    assert!(
        body.contains("<hibakod><![CDATA[7]]></hibakod>"),
        "still absent after a send that did not land: {body}"
    );

    let second = create().await.expect("second create");
    sends.received(2).await;
    assert_eq!(second.status().as_u16(), 200, "the second lands");
    assert_eq!(
        second
            .headers()
            .get("szlahu_szamlaszam")
            .and_then(|value| value.to_str().ok()),
        Some("SZ-7")
    );
    let (_, body) = query_by(&mock, by_id).await;
    assert!(
        body.contains("<szamlaszam>SZ-7</szamlaszam>"),
        "the holder from the second send on: {body}"
    );
}
