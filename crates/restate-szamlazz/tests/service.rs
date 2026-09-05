//! End-to-end tests of the `Szamlazz.Order` Virtual Object against a real
//! Restate server (docker) with wiremock standing in for szamlazz.hu.
//!
//! Ignored by default: `cargo test -p restate-szamlazz --test service -- --ignored`.
//! Skips (with a message) when the docker daemon is not reachable. Set
//! `RESTATE_ADMIN_URL` / `RESTATE_INGRESS_URL` to reuse a running server
//! instead of starting a container.

use std::net::TcpListener;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jiff::civil::date;
use restate_sdk::prelude::{Endpoint, HttpServer};
use restate_szamlazz::config::Config;
use restate_szamlazz::contract::{
    BuyerInput, DocumentInput, DocumentState, LineItemInput, OrderStatus, PaymentMethod,
};
use restate_szamlazz::{Agent, Order};
use rust_decimal::{Decimal, dec};
use serde_json::{Value, json};
use wiremock::matchers::{body_string_contains, method};
use wiremock::{Mock, MockBuilder, MockServer, ResponseTemplate};

const SUPPLIER: u64 = 972_720;
const IMAGE: &str = "docker.restate.dev/restatedev/restate:1.7.8";
const INGRESS_PORT: u16 = 18080;
const ADMIN_PORT: u16 = 19070;

// ----- szamlazz.hu fixtures (mirroring tests/gateway.rs) --------------------

struct Doc<'a> {
    number: &'a str,
    tipus: &'a str,
    order: Option<&'a str>,
    reversed: bool,
    referenced_invoice: Option<&'a str>,
    referenced_proforma: Option<&'a str>,
}

impl<'a> Doc<'a> {
    const fn new(number: &'a str, tipus: &'a str, order: &'a str) -> Self {
        Self {
            number,
            tipus,
            order: Some(order),
            reversed: false,
            referenced_invoice: None,
            referenced_proforma: None,
        }
    }

    fn response(&self) -> ResponseTemplate {
        let opt = |tag: &str, value: Option<&str>| {
            value.map_or_else(String::new, |value| format!("<{tag}>{value}</{tag}>"))
        };
        let eszamla = if self.tipus == "D" { 0 } else { 2 };
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<szamla xmlns="http://www.szamlazz.hu/szamla">
  <szallito><id>{SUPPLIER}</id><nev>Seller</nev><cim><irsz>1111</irsz><telepules>Budapest</telepules><cim>Fő u. 1.</cim></cim></szallito>
  <alap><id>924307338</id><szamlaszam>{number}</szamlaszam><tipus>{tipus}</tipus><eszamla>{eszamla}</eszamla>{hivszamlaszam}{hivdijbekszam}<kelt>2026-09-03</kelt>{rendelesszam}<teszt>true</teszt>{sztornozott}</alap>
  <vevo><nev>Buyer</nev></vevo>
  <tetelek></tetelek>
  <osszegek><totalossz><netto>1000</netto><afa>270</afa><brutto>1270</brutto></totalossz></osszegek>
</szamla>"#,
            number = self.number,
            tipus = self.tipus,
            hivszamlaszam = opt("hivszamlaszam", self.referenced_invoice),
            hivdijbekszam = opt("hivdijbekszam", self.referenced_proforma),
            rendelesszam = opt("rendelesszam", self.order),
            sztornozott = if self.reversed {
                "<sztornozott>true</sztornozott>"
            } else {
                ""
            },
        );
        ResponseTemplate::new(200).set_body_raw(xml, "application/xml")
    }
}

fn not_found() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(
        r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod><![CDATA[7]]></hibakod><hibauzenet><![CDATA[Hiányzó adat]]></hibauzenet></xmlszamlavalasz>"#,
        "application/xml",
    )
}

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

fn op(action: &str) -> MockBuilder {
    Mock::given(method("POST")).and(body_string_contains(format!("name=\"{action}\"")))
}

fn external_id_query(id: &str) -> MockBuilder {
    op("action-szamla_agent_xml").and(body_string_contains(format!(
        "<szamlaKulsoAzon>{id}</szamlaKulsoAzon>"
    )))
}

fn order_query(order: &str) -> MockBuilder {
    op("action-szamla_agent_xml").and(body_string_contains(format!(
        "<rendelesSzam>{order}</rendelesSzam>"
    )))
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

fn document(unit_price: Decimal) -> DocumentInput {
    DocumentInput::new(
        BuyerInput::new("Kovács Bt.", "2030", "Érd", "Tárnoki út 23."),
        vec![LineItemInput::new(
            "Elado izé",
            dec!(1),
            "db",
            unit_price,
            "27",
        )],
        date(2026, 9, 3),
        date(2026, 9, 11),
        PaymentMethod::Transfer,
    )
}

fn create_body(unit_price: Decimal, reissue: bool) -> Value {
    json!({
        "document": document(unit_price),
        "options": { "reissue": reissue },
    })
}

// ----- the harness -----------------------------------------------------------

fn docker_available() -> bool {
    Command::new("docker")
        .args(["info"])
        .output()
        .is_ok_and(|output| output.status.success())
}

/// A Restate server: an existing one (from the environment) or a container
/// removed on drop.
struct Restate {
    admin: String,
    ingress: String,
    container: Option<String>,
}

impl Restate {
    fn start() -> Self {
        if let (Ok(admin), Ok(ingress)) = (
            std::env::var("RESTATE_ADMIN_URL"),
            std::env::var("RESTATE_INGRESS_URL"),
        ) {
            return Self {
                admin,
                ingress,
                container: None,
            };
        }
        let output = Command::new("docker")
            .args([
                "run",
                "--rm",
                "-d",
                // Docker Desktop resolves `host.docker.internal` on its own;
                // a Linux daemon needs the alias to reach the endpoint.
                "--add-host=host.docker.internal:host-gateway",
                "-p",
                &format!("{INGRESS_PORT}:8080"),
                "-p",
                &format!("{ADMIN_PORT}:9070"),
                IMAGE,
            ])
            .output()
            .expect("docker run");
        assert!(
            output.status.success(),
            "docker run failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let container = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        Self {
            admin: format!("http://127.0.0.1:{ADMIN_PORT}"),
            ingress: format!("http://127.0.0.1:{INGRESS_PORT}"),
            container: Some(container),
        }
    }
}

impl Drop for Restate {
    fn drop(&mut self) {
        if let Some(container) = &self.container {
            let _ = Command::new("docker")
                .args(["rm", "-f", container])
                .output();
        }
    }
}

struct Harness {
    restate: Restate,
    mock: MockServer,
    http: reqwest::Client,
}

impl Harness {
    async fn start() -> Self {
        let restate = Restate::start();
        let mock = MockServer::start().await;
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("client");

        // Wait for the admin API.
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            if let Ok(response) = http.get(format!("{}/health", restate.admin)).send().await
                && response.status().is_success()
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Restate admin API did not come up"
            );
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // Serve the endpoint on a free port and register it.
        let config: Config = serde_json::from_value(json!({
            "account": {
                "slug": "acct",
                "agent_key": "key",
                "endpoint": mock.uri(),
                "mode": "test",
                "supplier_id": SUPPLIER,
            },
            // A short issue policy: two executions of the create step, one
            // second apart, so exhaustion is observable within the test.
            "issue": {
                "max_attempts": 2,
                "initial_delay": "1s",
                "factor": 2.0,
                "max_delay": "2s",
                "max_duration": "1m",
            },
        }))
        .expect("config");
        let order = Order::new(&config).expect("order");
        let agent = Agent::from_parts(Arc::clone(order.gateway()), order.config().clone());
        let listener = TcpListener::bind("0.0.0.0:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        listener.set_nonblocking(true).expect("nonblocking");
        let listener = tokio::net::TcpListener::from_std(listener).expect("tokio listener");
        tokio::spawn(async move {
            HttpServer::new(Endpoint::builder().bind(order).bind(agent).build())
                .serve(listener)
                .await;
        });

        let deployment =
            json!({ "uri": format!("http://host.docker.internal:{port}"), "force": true });
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let response = http
                .post(format!("{}/deployments", restate.admin))
                .json(&deployment)
                .send()
                .await;
            match response {
                Ok(response) if response.status().is_success() => break,
                Ok(response) => {
                    let body = response.text().await.unwrap_or_default();
                    assert!(
                        Instant::now() < deadline,
                        "deployment registration failed: {body}"
                    );
                }
                Err(error) => assert!(Instant::now() < deadline, "admin unreachable: {error}"),
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        Self {
            restate,
            mock,
            http,
        }
    }

    /// Calls `handler` on `key` with an `Idempotency-Key`.
    async fn call(
        &self,
        key: &str,
        handler: &str,
        body: &Value,
        idempotency: &str,
    ) -> (u16, Value) {
        let response = self
            .http
            .post(format!(
                "{}/Szamlazz.Order/{key}/{handler}",
                self.restate.ingress
            ))
            .header("idempotency-key", idempotency)
            .json(body)
            .send()
            .await
            .expect("ingress call");
        let status = response.status().as_u16();
        let text = response.text().await.expect("body");
        let value = serde_json::from_str(&text).unwrap_or(Value::String(text));
        (status, value)
    }

    async fn ok(&self, key: &str, handler: &str, body: &Value, idempotency: &str) -> Value {
        let (status, value) = self.call(key, handler, body, idempotency).await;
        assert_eq!(status, 200, "{handler} on {key}: {value}");
        value
    }

    /// `Szamlazz.Order.get`: no input, no idempotency key.
    async fn get(&self, key: &str) -> Value {
        let response = self
            .http
            .post(format!("{}/Szamlazz.Order/{key}/get", self.restate.ingress))
            .send()
            .await
            .expect("ingress call");
        let status = response.status().as_u16();
        let text = response.text().await.expect("body");
        let value: Value = serde_json::from_str(&text).unwrap_or(Value::String(text));
        assert_eq!(status, 200, "get on {key}: {value}");
        value
    }

    async fn reset(&self) {
        self.mock.reset().await;
    }

    async fn requests_seen(&self) -> usize {
        self.mock.received_requests().await.expect("requests").len()
    }

    /// Mounts the code-7 answers for the external ids of `kinds` under
    /// `order`.
    async fn absent(&self, order: &str, kinds: &[&str]) {
        for kind in kinds {
            external_id_query(&format!("acct:{order}:{kind}"))
                .respond_with(not_found())
                .mount(&self.mock)
                .await;
        }
    }
}

// ----- scenarios ---------------------------------------------------------------

#[tokio::test]
#[ignore = "needs docker"]
async fn e2e_order_protocol() {
    if !docker_available() {
        eprintln!("skipping: docker daemon not available");
        return;
    }
    let h = Harness::start().await;
    issued_then_already_issued(&h).await;
    idempotency_key_replays_without_calling_szamlazz(&h).await;
    duplicate_order_number_reconciles(&h).await;
    storno_then_stale_create_then_reissue(&h).await;
    reissue_on_live_is_a_conflict(&h).await;
    external_reversal_detected(&h).await;
    proforma_auto_link_and_consumed(&h).await;
    status_shape(&h).await;
    secondary_lookup_collision_refuses_to_create(&h).await;
    prepayment_takes_no_proforma_option(&h).await;
    exhausted_create_step_is_a_structured_outcome_unknown(&h).await;
}

/// (i) create ⇒ `issued`; a second call with a **new** key ⇒
/// `already_issued` from the lookup step.
async fn issued_then_already_issued(h: &Harness) {
    h.reset().await;
    h.absent("E2E-1", &["prepayment", "proforma"]).await;
    order_query("E2E-1")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    // The lookup step and the create step's own leading query both miss;
    // the second call's lookup finds the document.
    external_id_query("acct:E2E-1:invoice")
        .respond_with(not_found())
        .up_to_n_times(2)
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-1:invoice")
        .respond_with(Doc::new("SZ-1", "SZ", "E2E-1").response())
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-1", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;

    let first = h
        .ok(
            "E2E-1",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-1-k1",
        )
        .await;
    assert_eq!(first["outcome"], "issued", "{first}");
    assert_eq!(first["invoice_number"], "SZ-1");
    assert_eq!(first["kind"], "invoice");
    assert_eq!(first["external_id"], "acct:E2E-1:invoice");
    assert_eq!(first["gross_total"], "1270");
    assert_eq!(first.get("gen"), None);
    assert_eq!(first.get("request_id"), None);

    let again = h
        .ok(
            "E2E-1",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-1-k2",
        )
        .await;
    assert_eq!(again["outcome"], "already_issued", "{again}");
    assert_eq!(again["invoice_number"], "SZ-1");
    assert_eq!(again["gross_total"], "1270");
    assert_eq!(again["outstanding"], "1270");
    eprintln!("(i) issued → already_issued (new key): pass");
}

/// (ii) the same `Idempotency-Key` ⇒ the stored completion, byte for byte,
/// without a single call to szamlazz.hu.
async fn idempotency_key_replays_without_calling_szamlazz(h: &Harness) {
    let before = h.requests_seen().await;
    let replay = h
        .ok(
            "E2E-1",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-1-k1",
        )
        .await;
    assert_eq!(replay["outcome"], "issued", "{replay}");
    assert_eq!(replay["invoice_number"], "SZ-1");
    assert_eq!(
        h.requests_seen().await,
        before,
        "a replayed completion reaches neither the query nor the create mock"
    );
    // The create mock's `expect(1)` is verified when the server is reset.
    eprintln!("(ii) same key → identical response, no szamlazz.hu call: pass");
}

/// (iii) 152 on create, then the external-id re-query finds the document ⇒
/// `reconciled`.
async fn duplicate_order_number_reconciles(h: &Harness) {
    h.reset().await;
    h.absent("E2E-3", &["prepayment", "proforma"]).await;
    order_query("E2E-3")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    // Lookup and the create step's leading query miss; the re-query after
    // the 152 finds the document.
    external_id_query("acct:E2E-3:invoice")
        .respond_with(not_found())
        .up_to_n_times(2)
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-3:invoice")
        .respond_with(Doc::new("SZ-3", "SZ", "E2E-3").response())
        .mount(&h.mock)
        .await;
    create()
        .respond_with(api_error("152", "duplicate"))
        .expect(1)
        .mount(&h.mock)
        .await;

    let reconciled = h
        .ok(
            "E2E-3",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-3-k1",
        )
        .await;
    assert_eq!(reconciled["outcome"], "reconciled", "{reconciled}");
    assert_eq!(reconciled["invoice_number"], "SZ-3");
    eprintln!("(iii) 152 + ext-id re-query → reconciled: pass");
}

/// After the storno of `SZ-1` on `E2E-1`: the reversed document under our
/// external id, its storno `SS-1` as the newest document under the order,
/// nothing under the other ids.
async fn mount_reversed_sz1(h: &Harness) {
    h.reset().await;
    h.absent("E2E-1", &["prepayment", "proforma"]).await;
    external_id_query("acct:E2E-1:invoice")
        .respond_with(
            Doc {
                reversed: true,
                ..Doc::new("SZ-1", "SZ", "E2E-1")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    order_query("E2E-1")
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-1"),
                ..Doc::new("SS-1", "SS", "E2E-1")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
}

/// (iv) storno ⇒ `reversed{storno_number}`; a create ⇒ `reversed`; a create
/// with `reissue` ⇒ `issued` as the newest holder of the same external id.
async fn storno_then_stale_create_then_reissue(h: &Harness) {
    h.reset().await;
    number_query("SZ-1")
        .respond_with(Doc::new("SZ-1", "SZ", "E2E-1").response())
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-1:storno:SZ-1")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    storno()
        .respond_with(created("SS-1", "-1000", "-1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reversed = h
        .ok(
            "E2E-1",
            "storno_invoice",
            &json!({ "invoice_number": "SZ-1" }),
            "e2e-1-s1",
        )
        .await;
    assert_eq!(reversed["outcome"], "reversed", "{reversed}");
    assert_eq!(reversed["storno_number"], "SS-1");
    assert_eq!(reversed["invoice_number"], "SZ-1");

    // The stale create: the lookup finds the reversed document.
    mount_reversed_sz1(h).await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let stale = h
        .ok(
            "E2E-1",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-1-k3",
        )
        .await;
    assert_eq!(stale["outcome"], "reversed", "{stale}");
    assert_eq!(stale["invoice_number"], "SZ-1");
    assert_eq!(stale["storno_number"], "SS-1");

    // Reissue: the lookup passes the reversed document and its hint sees the
    // storno; the create step's leading query sees the same reversed
    // document and issues the next one under the same id.
    mount_reversed_sz1(h).await;
    create()
        .respond_with(created("SZ-2", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reissued = h
        .ok(
            "E2E-1",
            "create_invoice",
            &create_body(dec!(1000), true),
            "e2e-1-k4",
        )
        .await;
    assert_eq!(reissued["outcome"], "issued", "{reissued}");
    assert_eq!(reissued["external_id"], "acct:E2E-1:invoice");
    assert_eq!(reissued["invoice_number"], "SZ-2");
    eprintln!("(iv) storno → reversed; stale create → reversed; reissue → issued: pass");
}

/// (v) `reissue: true` while the document is live ⇒ `conflict{live}`.
async fn reissue_on_live_is_a_conflict(h: &Harness) {
    h.reset().await;
    h.absent("E2E-1", &["prepayment", "proforma"]).await;
    external_id_query("acct:E2E-1:invoice")
        .respond_with(Doc::new("SZ-2", "SZ", "E2E-1").response())
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let conflict = h
        .ok(
            "E2E-1",
            "create_invoice",
            &create_body(dec!(1000), true),
            "e2e-1-k5",
        )
        .await;
    assert_eq!(conflict["outcome"], "conflict", "{conflict}");
    assert_eq!(conflict["conflict_reason"], "live");
    assert_eq!(conflict["existing_number"], "SZ-2");
    eprintln!("(v) reissue on a live document → conflict{{live}}: pass");
}

/// (vi) the lookup returns `<sztornozott>true</sztornozott>` (a UI storno)
/// ⇒ `reversed`, storno number unknown.
async fn external_reversal_detected(h: &Harness) {
    h.reset().await;
    h.absent("E2E-6", &["prepayment", "proforma", "invoice"])
        .await;
    order_query("E2E-6")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-6", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let issued = h
        .ok(
            "E2E-6",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-6-k1",
        )
        .await;
    assert_eq!(issued["outcome"], "issued", "{issued}");

    h.reset().await;
    h.absent("E2E-6", &["prepayment", "proforma"]).await;
    external_id_query("acct:E2E-6:invoice")
        .respond_with(
            Doc {
                reversed: true,
                ..Doc::new("SZ-6", "SZ", "E2E-6")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    order_query("E2E-6")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let detected = h
        .ok(
            "E2E-6",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-6-k2",
        )
        .await;
    assert_eq!(detected["outcome"], "reversed", "{detected}");
    assert_eq!(detected["invoice_number"], "SZ-6");
    assert_eq!(detected["storno_number"], Value::Null);
    eprintln!("(vi) sztornozott on the lookup → reversed: pass");
}

/// (vii) a proforma, then an invoice with the default `proforma: auto` ⇒ the
/// create carries `dijbekeroSzamlaszam`; `get` then reports the proforma
/// `consumed` by the invoice.
async fn proforma_auto_link_and_consumed(h: &Harness) {
    h.reset().await;
    h.absent("E2E-7", &["proforma"]).await;
    order_query("E2E-7")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create()
        .and(body_string_contains("<dijbekero>true</dijbekero>"))
        .respond_with(created("D-7", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let proforma = h
        .ok(
            "E2E-7",
            "create_proforma",
            &json!({ "document": document(dec!(1000)) }),
            "e2e-7-p1",
        )
        .await;
    assert_eq!(proforma["outcome"], "issued", "{proforma}");
    assert_eq!(proforma["kind"], "proforma");
    assert_eq!(proforma["invoice_number"], "D-7");
    assert_eq!(proforma["external_id"], "acct:E2E-7:proforma");

    h.reset().await;
    h.absent("E2E-7", &["prepayment", "invoice"]).await;
    external_id_query("acct:E2E-7:proforma")
        .respond_with(Doc::new("D-7", "D", "E2E-7").response())
        .mount(&h.mock)
        .await;
    order_query("E2E-7")
        .respond_with(Doc::new("D-7", "D", "E2E-7").response())
        .mount(&h.mock)
        .await;
    create()
        .and(body_string_contains(
            "<dijbekeroSzamlaszam>D-7</dijbekeroSzamlaszam>",
        ))
        .respond_with(created("SZ-7", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let invoice = h
        .ok(
            "E2E-7",
            "create_invoice",
            &json!({ "document": document(dec!(1000)) }),
            "e2e-7-k1",
        )
        .await;
    assert_eq!(invoice["outcome"], "issued", "{invoice}");
    assert_eq!(invoice["invoice_number"], "SZ-7");
    assert_eq!(invoice["warnings"], json!([]));

    // After the conversion the proforma is gone from the query surface and
    // the invoice carries `hivdijbekszam`.
    h.reset().await;
    h.absent("E2E-7", &["proforma", "prepayment", "final"])
        .await;
    external_id_query("acct:E2E-7:invoice")
        .respond_with(
            Doc {
                referenced_proforma: Some("D-7"),
                ..Doc::new("SZ-7", "SZ", "E2E-7")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    let status = h.get("E2E-7").await;
    assert_eq!(status["proforma"]["state"], "consumed", "{status}");
    assert_eq!(status["proforma"]["by"], "SZ-7");
    assert_eq!(status["proforma"]["number"], "D-7");
    assert_eq!(status["invoice"]["state"], "live");
    assert_eq!(status["invoice"]["number"], "SZ-7");
    assert_eq!(status["invoice"]["referenced_proforma"], "D-7");
    eprintln!("(vii) proforma → invoice (auto link) → get shows consumed: pass");
}

/// (viii) the `get` live view after (iv)/(v).
async fn status_shape(h: &Harness) {
    h.reset().await;
    h.absent("E2E-1", &["proforma", "prepayment", "final"])
        .await;
    external_id_query("acct:E2E-1:invoice")
        .respond_with(Doc::new("SZ-2", "SZ", "E2E-1").response())
        .mount(&h.mock)
        .await;
    let status = h.get("E2E-1").await;
    assert_eq!(status["invoice"]["number"], "SZ-2", "{status}");
    assert_eq!(status["invoice"]["state"], "live");
    assert_eq!(status["invoice"]["gross"], "1270");
    assert_eq!(status["invoice"]["net"], "1000");
    assert_eq!(status["invoice"]["payments"], json!([]));
    assert_eq!(status["invoice"]["e_invoice"], true);
    assert_eq!(status["proforma"], Value::Null);
    assert_eq!(status["prepayment"], Value::Null);
    assert_eq!(status["final"], Value::Null);
    let status: OrderStatus = serde_json::from_value(status).expect("status deserialises");
    let invoice = status.invoice.expect("invoice");
    assert_eq!(invoice.state, DocumentState::Live);
    assert_eq!(invoice.gross, Some(dec!(1270)));
    assert!(status.proforma.is_none());
    eprintln!("(viii) get shape: pass");
}

/// (ix) a valid-looking document under `…:prepayment` that carries another
/// order's number — an external-id collision on a *secondary* lookup — ⇒
/// `conflict{external_id_collision}` from `create_invoice`, nothing created:
/// the newest holder may hide a live prepayment of ours behind it. `get`
/// reports the same slot as absent (a read must not fail).
async fn secondary_lookup_collision_refuses_to_create(h: &Harness) {
    h.reset().await;
    h.absent("E2E-9", &["proforma", "invoice", "final"]).await;
    external_id_query("acct:E2E-9:prepayment")
        .respond_with(Doc::new("ES-X", "ES", "OTHER-ORDER").response())
        .mount(&h.mock)
        .await;
    order_query("E2E-9")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let conflict = h
        .ok(
            "E2E-9",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-9-k1",
        )
        .await;
    assert_eq!(conflict["outcome"], "conflict", "{conflict}");
    assert_eq!(conflict["conflict_reason"], "external_id_collision");
    assert_eq!(conflict["existing_number"], "ES-X");
    assert_eq!(conflict["kind"], "invoice");
    assert_eq!(conflict["external_id"], "acct:E2E-9:invoice");

    let status = h.get("E2E-9").await;
    assert_eq!(status["prepayment"], Value::Null, "{status}");
    assert_eq!(status["invoice"], Value::Null);
    eprintln!("(ix) collision on the prepayment lookup → conflict{{external_id_collision}}: pass");
}

/// (x) `create_prepayment` takes no `options.proforma` — anything but `auto`
/// is `invalid_input` before any szamlazz.hu call — and under `auto` it runs
/// no proforma lookup: the server converts the order's live proforma by
/// shared order number on its own.
async fn prepayment_takes_no_proforma_option(h: &Harness) {
    h.reset().await;
    let before = h.requests_seen().await;
    let (status, body) = h
        .call(
            "E2E-10",
            "create_prepayment",
            &json!({ "document": document(dec!(1000)), "options": { "proforma": "none" } }),
            "e2e-10-k1",
        )
        .await;
    assert_eq!(status, 400, "{body}");
    assert!(
        body.to_string().contains("invalid_input"),
        "carries the fault code: {body}"
    );
    assert_eq!(h.requests_seen().await, before, "refused before any call");

    h.absent("E2E-10", &["invoice", "prepayment"]).await;
    external_id_query("acct:E2E-10:proforma")
        .respond_with(Doc::new("D-10", "D", "E2E-10").response())
        .expect(0)
        .mount(&h.mock)
        .await;
    order_query("E2E-10")
        .respond_with(Doc::new("D-10", "D", "E2E-10").response())
        .mount(&h.mock)
        .await;
    create()
        .and(body_string_contains("<elolegszamla>true</elolegszamla>"))
        .respond_with(created("ES-10", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let issued = h
        .ok(
            "E2E-10",
            "create_prepayment",
            &json!({ "document": document(dec!(1000)) }),
            "e2e-10-k2",
        )
        .await;
    assert_eq!(issued["outcome"], "issued", "{issued}");
    assert_eq!(issued["kind"], "prepayment");
    assert_eq!(issued["invoice_number"], "ES-10");
    assert_eq!(issued["external_id"], "acct:E2E-10:prepayment");
    eprintln!("(x) create_prepayment: proforma option refused, no proforma lookup: pass");
}

/// (xi) every execution of the create step loses its reply and the re-query
/// finds nothing ⇒ the run retry policy re-executes the step (one second
/// later under the test policy — not the handler's two-minute
/// `initial_interval`), and its exhaustion is a structured `outcome_unknown`
/// fault naming the order, kind and external id. The
/// `sys_invocation.retry_count` assertion — run retries must not consume the
/// handler's `invocation_retry_policy` budget — waits for the SQL helper
/// of #29.
async fn exhausted_create_step_is_a_structured_outcome_unknown(h: &Harness) {
    h.reset().await;
    h.absent("E2E-11", &["prepayment", "proforma", "invoice"])
        .await;
    order_query("E2E-11")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create()
        .respond_with(ResponseTemplate::new(500))
        .expect(2)
        .mount(&h.mock)
        .await;

    let started = Instant::now();
    let (status, body) = h
        .call(
            "E2E-11",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-11-k1",
        )
        .await;
    let elapsed = started.elapsed();
    assert_eq!(status, 500, "{body}");
    assert!(
        elapsed < Duration::from_secs(60),
        "the run policy's delay was honoured, not the handler's: {elapsed:?}"
    );

    // The ingress wraps the handler's terminal error; the fault is the JSON
    // in its message.
    let message = body["message"]
        .as_str()
        .unwrap_or_else(|| panic!("an error envelope with a message: {body}"));
    let fault: Value = serde_json::from_str(message)
        .unwrap_or_else(|error| panic!("a structured fault ({error}): {message}"));
    assert_eq!(fault["code"], "outcome_unknown", "{fault}");
    assert_eq!(fault["order"], "E2E-11");
    assert_eq!(fault["kind"], "invoice");
    assert_eq!(fault["external_id"], "acct:E2E-11:invoice");
    assert!(
        fault["message"]
            .as_str()
            .is_some_and(|text| text.contains("retry with a new Idempotency-Key")),
        "{fault}"
    );
    eprintln!("(xi) exhausted create step → structured outcome_unknown: pass");
}
