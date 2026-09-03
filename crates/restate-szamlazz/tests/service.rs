//! End-to-end tests of the `Order` Virtual Object against a real Restate
//! server (docker) with wiremock standing in for szamlazz.hu.
//!
//! Ignored by default: `cargo test -p restate-szamlazz --all-features -- --ignored e2e`.
//! Skips (with a message) when the docker daemon is not reachable. Set
//! `RESTATE_ADMIN_URL` / `RESTATE_INGRESS_URL` to reuse a running server
//! instead of starting a container.

#![cfg(feature = "service")]

use std::net::TcpListener;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jiff::civil::date;
use restate_sdk::prelude::{Endpoint, HttpServer};
use restate_szamlazz::config::Config;
use restate_szamlazz::contract::{BuyerInput, DocumentInput, LineItemInput, PaymentMethod};
use restate_szamlazz::{Order, SzamlaAgentService};
use rust_decimal::{Decimal, dec};
use serde_json::{Value, json};
use wiremock::matchers::{body_string_contains, method};
use wiremock::{Mock, MockBuilder, MockServer, ResponseTemplate};

const SUPPLIER: u64 = 972_720;
const IMAGE: &str = "docker.restate.dev/restatedev/restate:1.7.8";
const INGRESS_PORT: u16 = 18080;
const ADMIN_PORT: u16 = 19070;

// ----- szamlazz.hu fixtures (mirroring tests/szamla_agent.rs) ---------------

struct Doc<'a> {
    number: &'a str,
    tipus: &'a str,
    order: Option<&'a str>,
    reversed: bool,
    referenced_invoice: Option<&'a str>,
}

impl<'a> Doc<'a> {
    const fn new(number: &'a str, tipus: &'a str, order: &'a str) -> Self {
        Self {
            number,
            tipus,
            order: Some(order),
            reversed: false,
            referenced_invoice: None,
        }
    }

    fn response(&self) -> ResponseTemplate {
        let opt = |tag: &str, value: Option<&str>| {
            value.map_or_else(String::new, |value| format!("<{tag}>{value}</{tag}>"))
        };
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<szamla xmlns="http://www.szamlazz.hu/szamla">
  <szallito><id>{SUPPLIER}</id><nev>Seller</nev><cim><irsz>1111</irsz><telepules>Budapest</telepules><cim>Fő u. 1.</cim></cim></szallito>
  <alap><id>924307338</id><szamlaszam>{number}</szamlaszam><tipus>{tipus}</tipus><eszamla>2</eszamla>{hivszamlaszam}<kelt>2026-09-03</kelt>{rendelesszam}<teszt>true</teszt>{sztornozott}</alap>
  <vevo><nev>Buyer</nev></vevo>
  <tetelek></tetelek>
  <osszegek><totalossz><netto>1000</netto><afa>270</afa><brutto>1270</brutto></totalossz></osszegek>
</szamla>"#,
            number = self.number,
            tipus = self.tipus,
            hivszamlaszam = opt("hivszamlaszam", self.referenced_invoice),
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

fn create_body(request_id: &str, unit_price: Decimal, reissue: bool) -> Value {
    json!({
        "request_id": request_id,
        "document": document(unit_price),
        "options": { "reissue": reissue, "proforma": "none" },
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
        let config: Arc<Config> = Arc::new(
            serde_json::from_value(json!({
                "account": {
                    "slug": "acct",
                    "agent_key": "key",
                    "fp_secret": "fp",
                    "endpoint": mock.uri(),
                    "mode": "test",
                    "supplier_id": SUPPLIER,
                },
                "issue": { "max_attempts": 2, "first_backoff": "1s", "max_backoff": "2s" },
            }))
            .expect("config"),
        );
        let order = Order::new(Arc::clone(&config)).expect("order");
        let agent = SzamlaAgentService::from_parts(Arc::clone(order.agent()), config);
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

    async fn call(&self, key: &str, handler: &str, body: &Value) -> (u16, Value) {
        let response = self
            .http
            .post(format!("{}/Order/{key}/{handler}", self.restate.ingress))
            .json(body)
            .send()
            .await
            .expect("ingress call");
        let status = response.status().as_u16();
        let text = response.text().await.expect("body");
        let value = serde_json::from_str(&text).unwrap_or(Value::String(text));
        (status, value)
    }

    async fn ok(&self, key: &str, handler: &str, body: &Value) -> Value {
        let (status, value) = self.call(key, handler, body).await;
        assert_eq!(status, 200, "{handler} on {key}: {value}");
        value
    }

    async fn reset(&self) {
        self.mock.reset().await;
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
    issue_then_already_issued(&h).await;
    new_request_id_and_payload_drift(&h).await;
    duplicate_order_number_reconciles(&h).await;
    storno_then_reissue(&h).await;
    external_reversal_detected(&h).await;
    proforma_consumed_by_invoice(&h).await;
    snapshot_shape(&h).await;
    private_handlers_are_not_public(&h).await;
}

/// (i) issue, then the same request id again ⇒ `already_issued`.
async fn issue_then_already_issued(h: &Harness) {
    // (i) issue, then the same request id again ⇒ already_issued.
    h.reset().await;
    external_id_query("acct:E2E-1:invoice:0")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    order_query("E2E-1")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-1", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    number_query("SZ-1")
        .respond_with(Doc::new("SZ-1", "SZ", "E2E-1").response())
        .mount(&h.mock)
        .await;

    let first = h
        .ok(
            "E2E-1",
            "create_invoice",
            &create_body("r-1", dec!(1000), false),
        )
        .await;
    assert_eq!(first["outcome"], "issued", "{first}");
    assert_eq!(first["invoice_number"], "SZ-1");
    assert_eq!(first["external_id"], "acct:E2E-1:invoice:0");
    assert_eq!(first["gen"], 0);
    assert_eq!(first["gross_total"], "1270");

    let again = h
        .ok(
            "E2E-1",
            "create_invoice",
            &create_body("r-1", dec!(1000), false),
        )
        .await;
    assert_eq!(again["outcome"], "already_issued", "{again}");
    assert_eq!(again["invoice_number"], "SZ-1");
    eprintln!("(i) issued → already_issued: pass");
}

/// (ii) a new request id: same payload ⇒ `already_issued`, drift ⇒
/// `conflict{payload_mismatch}`.
async fn new_request_id_and_payload_drift(h: &Harness) {
    // (ii) a different request id with the same document ⇒ already_issued;
    // with a different gross ⇒ conflict{payload_mismatch}.
    let other = h
        .ok(
            "E2E-1",
            "create_invoice",
            &create_body("r-2", dec!(1000), false),
        )
        .await;
    assert_eq!(other["outcome"], "already_issued", "{other}");
    let drifted = h
        .ok(
            "E2E-1",
            "create_invoice",
            &create_body("r-3", dec!(2000), false),
        )
        .await;
    assert_eq!(drifted["outcome"], "conflict", "{drifted}");
    assert_eq!(drifted["conflict_reason"], "payload_mismatch");
    assert_eq!(drifted["existing_number"], "SZ-1");
    eprintln!("(ii) new id same payload → already_issued; drift → payload_mismatch: pass");
}

/// (iii) 152 on create, then the external-id re-query finds the document ⇒
/// `reconciled`.
async fn duplicate_order_number_reconciles(h: &Harness) {
    // (iii) 152 on create, then the external-id re-query finds the document
    // ⇒ reconciled.
    h.reset().await;
    external_id_query("acct:E2E-3:invoice:0")
        .respond_with(not_found())
        .up_to_n_times(1)
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-3:invoice:0")
        .respond_with(Doc::new("SZ-3", "SZ", "E2E-3").response())
        .mount(&h.mock)
        .await;
    order_query("E2E-3")
        .respond_with(not_found())
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
            &create_body("r-1", dec!(1000), false),
        )
        .await;
    assert_eq!(reconciled["outcome"], "reconciled", "{reconciled}");
    assert_eq!(reconciled["invoice_number"], "SZ-3");
    eprintln!("(iii) 152 + ext-id re-query → reconciled: pass");
}

/// (iv) storno ⇒ `reversed`; the old request id ⇒ `reversed`; a new id with
/// `reissue` ⇒ `issued` at generation 1.
async fn storno_then_reissue(h: &Harness) {
    // (iv) storno ⇒ reversed; old request id ⇒ reversed; new id + reissue ⇒
    // issued at gen 1.
    h.reset().await;
    number_query("SZ-1")
        .respond_with(Doc::new("SZ-1", "SZ", "E2E-1").response())
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-1:invoice:0:storno")
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
        )
        .await;
    assert_eq!(reversed["outcome"], "reversed", "{reversed}");
    assert_eq!(reversed["storno_number"], "SS-1");

    let stale = h
        .ok(
            "E2E-1",
            "create_invoice",
            &create_body("r-1", dec!(1000), false),
        )
        .await;
    assert_eq!(stale["outcome"], "reversed", "{stale}");
    assert_eq!(stale["invoice_number"], "SZ-1");
    assert_eq!(stale["storno_number"], "SS-1");

    h.reset().await;
    external_id_query("acct:E2E-1:invoice:1")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    order_query("E2E-1")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-2", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reissued = h
        .ok(
            "E2E-1",
            "create_invoice",
            &create_body("r-4", dec!(1000), true),
        )
        .await;
    assert_eq!(reissued["outcome"], "issued", "{reissued}");
    assert_eq!(reissued["gen"], 1);
    assert_eq!(reissued["external_id"], "acct:E2E-1:invoice:1");
    assert_eq!(reissued["invoice_number"], "SZ-2");
    eprintln!("(iv) storno → reversed; stale id → reversed; reissue → issued gen 1: pass");
}

/// (v) verification finds the recorded document reversed ⇒ `reversed`; the
/// same id with `reissue` ⇒ `invalid_input` (400).
async fn external_reversal_detected(h: &Harness) {
    // (v) verification finds the recorded document reversed ⇒ reversed; the
    // same id with `reissue` ⇒ invalid_input (400).
    h.reset().await;
    external_id_query("acct:E2E-5:invoice:0")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    order_query("E2E-5")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-5", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let issued = h
        .ok(
            "E2E-5",
            "create_invoice",
            &create_body("r-1", dec!(1000), false),
        )
        .await;
    assert_eq!(issued["outcome"], "issued", "{issued}");
    h.reset().await;
    number_query("SZ-5")
        .respond_with(
            Doc {
                reversed: true,
                ..Doc::new("SZ-5", "SZ", "E2E-5")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    let detected = h
        .ok(
            "E2E-5",
            "create_invoice",
            &create_body("r-1", dec!(1000), false),
        )
        .await;
    assert_eq!(detected["outcome"], "reversed", "{detected}");
    assert_eq!(detected["invoice_number"], "SZ-5");
    let (status, body) = h
        .call(
            "E2E-5",
            "create_invoice",
            &create_body("r-1", dec!(1000), true),
        )
        .await;
    assert_eq!(status, 400, "{body}");
    assert!(
        body["message"]
            .as_str()
            .is_some_and(|m| m.contains("invalid_input")),
        "{body}"
    );
    eprintln!("(v) external reversal detected → reversed; reissue with known id → 400: pass");
}

/// (v-b) a proforma, then an invoice with `proforma: ledger` ⇒ the create
/// carries `dijbekeroSzamlaszam` and the proforma slot becomes `consumed`.
async fn proforma_consumed_by_invoice(h: &Harness) {
    h.reset().await;
    external_id_query("acct:E2E-7:proforma:0")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-7:invoice:0")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
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
    number_query("D-7")
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

    let proforma = h
        .ok(
            "E2E-7",
            "create_proforma",
            &json!({ "request_id": "p-1", "document": document(dec!(1000)) }),
        )
        .await;
    assert_eq!(proforma["outcome"], "issued", "{proforma}");
    assert_eq!(proforma["invoice_number"], "D-7");

    let invoice = h
        .ok(
            "E2E-7",
            "create_invoice",
            &json!({
                "request_id": "r-1",
                "document": document(dec!(1000)),
                "options": { "reissue": false, "proforma": "ledger" },
            }),
        )
        .await;
    assert_eq!(invoice["outcome"], "issued", "{invoice}");
    assert_eq!(invoice["invoice_number"], "SZ-7");
    assert_eq!(invoice["warnings"], json!([]));

    let snapshot = h.ok("E2E-7", "get", &json!({})).await;
    assert_eq!(
        snapshot["slots"]["proforma"]["status"], "consumed",
        "{snapshot}"
    );
    assert_eq!(snapshot["slots"]["proforma"]["gen"], 1);
    assert_eq!(snapshot["slots"]["proforma"]["number"], "D-7");
    assert_eq!(snapshot["slots"]["invoice"]["status"], "committed");
    assert_eq!(snapshot["slots"]["invoice"]["number"], "SZ-7");
    let consumed = snapshot["history"]
        .as_array()
        .expect("history")
        .iter()
        .find(|event| event["event"] == "consumed")
        .expect("a consumed event");
    assert_eq!(consumed["by"], "SZ-7");
    assert_eq!(consumed["kind"], "proforma");
    eprintln!("(v-b) proforma → invoice with proforma: ledger → consumed: pass");
}

/// (vi) the `get` snapshot after (iv).
async fn snapshot_shape(h: &Harness) {
    // (vi) the snapshot.
    let snapshot = h.ok("E2E-1", "get", &json!({})).await;
    assert_eq!(snapshot["freshness"], "snapshot", "{snapshot}");
    assert_eq!(snapshot["supplier_id"], SUPPLIER);
    assert_eq!(snapshot["slots"]["invoice"]["status"], "committed");
    assert_eq!(snapshot["slots"]["invoice"]["gen"], 1);
    assert_eq!(snapshot["slots"]["invoice"]["number"], "SZ-2");
    assert_eq!(snapshot["slots"]["invoice"]["request_id"], "r-4");
    assert_eq!(snapshot["slots"]["proforma"], Value::Null);
    let events: Vec<_> = snapshot["history"]
        .as_array()
        .expect("history")
        .iter()
        .map(|event| event["event"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(events, ["issued", "reversed", "issued"], "{snapshot}");
    assert_eq!(snapshot["history"][1]["by"], "SS-1");
    assert_eq!(snapshot["history"][1]["origin"], "service");
    let snapshot: restate_szamlazz::contract::OrderSnapshot =
        serde_json::from_value(snapshot).expect("snapshot deserialises");
    assert!(snapshot.verification.is_empty());
    eprintln!("(vi) get snapshot shape: pass");
}

/// (vii) private handlers are not reachable from the ingress.
async fn private_handlers_are_not_public(h: &Harness) {
    // (vii) private handlers are not reachable from the ingress.
    let (status, body) = h
        .call(
            "E2E-1",
            "record_reversal",
            &json!({ "invoice_number": "SZ-2", "result": "live" }),
        )
        .await;
    assert_eq!(status, 400, "{body}");
    assert!(
        body["message"]
            .as_str()
            .is_some_and(|m| m.contains("not public")),
        "{body}"
    );
    eprintln!("(vii) record_reversal from the ingress → 400 not public: pass");
}
