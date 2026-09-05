//! End-to-end tests of the `Szamlazz.Order` Virtual Object and the
//! `Szamlazz.Agent` service against a real Restate server (docker) with
//! wiremock standing in for szamlazz.hu.
//!
//! Ignored by default: `cargo test -p restate-szamlazz --test service -- --ignored`.
//! Skips (with a message) when the docker daemon is not reachable. Set
//! `RESTATE_ADMIN_URL` / `RESTATE_INGRESS_URL` to reuse a running server
//! instead of starting a container; it must run with the three experimental
//! flags ([`SERVER_FLAGS`]) — `compose.yaml` sets them.
//!
//! The harness calls through the `/restate/call/…` and
//! `/restate/scope/{scope}/call/…` ingress paths, reports the invocation id
//! (`x-restate-id`) and parses fault bodies, and reads `sys_journal` /
//! `sys_invocation` through the SQL introspection API — `raw` hex-decoded to
//! bytes, since run results are stored as bytes.

use std::net::TcpListener;
use std::process::Command;
use std::time::{Duration, Instant};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use jiff::civil::date;
use restate_sdk::prelude::{Endpoint, HttpServer};
use restate_szamlazz::account::{
    Account, AccountResolver, Accounts, BoxFuture, CredentialRef, CredentialStore, FetchError,
    ResolveError, StaticResolver,
};
use restate_szamlazz::config::{Config, ResolveConfig, WorkerConfig};
use restate_szamlazz::contract::{
    BuyerInput, DocumentInput, DocumentState, LineItemInput, OrderStatus, PaymentMethod,
};
use restate_szamlazz::{Agent, Order};
use rust_decimal::{Decimal, dec};
use serde::Deserialize;
use serde_json::{Value, json};
use szamlazz_agent::Credentials;
use wiremock::matchers::{body_string_contains, method};
use wiremock::{Mock, MockBuilder, MockServer, ResponseTemplate};

const SUPPLIER: u64 = 972_720;
/// The agent key of the test account: a sentinel that must never appear in a
/// journal entry.
const AGENT_KEY: &str = "e2e-agent-key-sentinel-7d1f4b";
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

/// The experimental server flags multi-account mode depends on (design, ADR
/// pending in #28): vqueues, protocol v7 (below it the SDK sees no scope) and
/// scoped Virtual Objects. Set on the container and expected of a server
/// reused through the environment.
const SERVER_FLAGS: [&str; 3] = [
    "RESTATE_EXPERIMENTAL_ENABLE_VQUEUES=true",
    "RESTATE_EXPERIMENTAL_ENABLE_PROTOCOL_V7=true",
    "RESTATE_EXPERIMENTAL_ENABLE_SCOPED_VIRTUAL_OBJECTS=true",
];

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
        let mut args = vec![
            "run".to_owned(),
            "--rm".to_owned(),
            "-d".to_owned(),
            // Docker Desktop resolves `host.docker.internal` on its own;
            // a Linux daemon needs the alias to reach the endpoint.
            "--add-host=host.docker.internal:host-gateway".to_owned(),
            "-p".to_owned(),
            format!("{INGRESS_PORT}:8080"),
            "-p".to_owned(),
            format!("{ADMIN_PORT}:9070"),
        ];
        for flag in SERVER_FLAGS {
            args.push("-e".to_owned());
            args.push(flag.to_owned());
        }
        args.push(IMAGE.to_owned());
        let output = Command::new("docker")
            .args(&args)
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

/// An ingress reply: the status, the parsed body and the invocation id the
/// ingress reports in `x-restate-id`.
#[derive(Debug)]
struct Reply {
    status: u16,
    body: Value,
    invocation_id: Option<String>,
}

impl Reply {
    fn invocation_id(&self) -> &str {
        self.invocation_id
            .as_deref()
            .unwrap_or_else(|| panic!("no x-restate-id on the reply: {}", self.body))
    }

    /// The structured fault inside the ingress error envelope: the handler's
    /// `TerminalError` message is the fault JSON.
    fn fault(&self) -> Fault {
        let message = self.body["message"]
            .as_str()
            .unwrap_or_else(|| panic!("an error envelope with a message: {}", self.body));
        serde_json::from_str(message)
            .unwrap_or_else(|error| panic!("a structured fault ({error}): {message}"))
    }
}

/// The fault body of a `TerminalError` (design §7).
#[derive(Debug, Deserialize)]
struct Fault {
    code: String,
    message: String,
    #[serde(default)]
    order: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    external_id: Option<String>,
}

/// A `sys_journal` row with `raw` decoded from hex to bytes: run results are
/// stored as bytes and render as integer arrays in `entry_json`, so a text
/// match on `entry_json` is vacuous.
///
/// Under protocol v7 (journal v2) a run is two rows: `Command: Run`, which
/// carries the name, and the `Notification: Run` that follows it, which
/// carries the result bytes (verified against 1.7.8). A leak check must scan
/// every row, not the named ones.
#[derive(Debug)]
struct JournalEntry {
    index: u64,
    entry_type: String,
    name: Option<String>,
    raw: Vec<u8>,
}

impl JournalEntry {
    /// Whether the entry is a `ctx.run` command (named).
    fn is_run(&self) -> bool {
        self.entry_type == "Command: Run"
    }

    /// Whether the entry's bytes contain `needle`.
    fn raw_contains(&self, needle: &str) -> bool {
        self.raw
            .windows(needle.len())
            .any(|window| window == needle.as_bytes())
    }
}

/// The result of the run named `name`: the `Notification: Run` row that
/// follows its command before any other command (the handlers await every
/// run, so its notification is the next journal event after the command).
fn run_result<'a>(journal: &'a [JournalEntry], name: &str) -> Option<&'a JournalEntry> {
    let command = journal
        .iter()
        .position(|entry| entry.is_run() && entry.name.as_deref() == Some(name))?;
    journal[command + 1..]
        .iter()
        .take_while(|entry| !entry.entry_type.starts_with("Command:"))
        .find(|entry| entry.entry_type == "Notification: Run")
}

/// What [`Harness::watch`] saw of an invocation's attempts while it ran.
#[derive(Debug, Default)]
struct Retries {
    max_retry_count: u64,
    failures: Vec<String>,
    failing_commands: Vec<String>,
}

/// A `sys_invocation` row of a completed invocation. `retry_count` and the
/// last failure are attempt state, gone once the invocation completed — see
/// [`Harness::watch`] for them.
#[derive(Debug)]
struct Invocation {
    status: String,
    completion_failure: Option<String>,
    scope: Option<String>,
    handler: String,
}

fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect()
}

/// The static resolver and store behind a script: the resolver fails the
/// next N resolutions with `unavailable`, and the store can be taken down.
/// What the prologue's e2e drives — a resolver that fails then succeeds, a
/// store that always fails — on the one deployment the harness registers.
#[derive(Debug)]
struct ScriptedAccounts {
    inner: StaticResolver,
    /// Resolutions left to fail with `unavailable`.
    resolver_failures: AtomicU32,
    /// How many times the resolver was asked.
    resolutions: AtomicU32,
    /// Whether every fetch fails with `unavailable`.
    store_down: AtomicBool,
    /// How many times the store was asked.
    fetches: AtomicU32,
}

impl ScriptedAccounts {
    fn new(inner: StaticResolver) -> Self {
        Self {
            inner,
            resolver_failures: AtomicU32::new(0),
            resolutions: AtomicU32::new(0),
            store_down: AtomicBool::new(false),
            fetches: AtomicU32::new(0),
        }
    }

    fn fail_next_resolutions(&self, count: u32) {
        self.resolver_failures.store(count, Ordering::SeqCst);
    }

    fn set_store_down(&self, down: bool) {
        self.store_down.store(down, Ordering::SeqCst);
    }

    fn resolutions(&self) -> u32 {
        self.resolutions.load(Ordering::SeqCst)
    }

    fn fetches(&self) -> u32 {
        self.fetches.load(Ordering::SeqCst)
    }
}

impl AccountResolver for ScriptedAccounts {
    fn resolve<'a>(
        &'a self,
        scope: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Account, ResolveError>> {
        Box::pin(async move {
            self.resolutions.fetch_add(1, Ordering::SeqCst);
            let outstanding = self.resolver_failures.load(Ordering::SeqCst);
            if outstanding > 0 {
                self.resolver_failures
                    .store(outstanding - 1, Ordering::SeqCst);
                return Err(ResolveError::unavailable(std::io::Error::other(
                    "scripted resolver outage",
                )));
            }
            self.inner.resolve(scope).await
        })
    }
}

impl CredentialStore for ScriptedAccounts {
    fn fetch<'a>(
        &'a self,
        credential_ref: &'a CredentialRef,
    ) -> BoxFuture<'a, Result<Credentials, FetchError>> {
        Box::pin(async move {
            self.fetches.fetch_add(1, Ordering::SeqCst);
            if self.store_down.load(Ordering::SeqCst) {
                return Err(FetchError::unavailable(std::io::Error::other(
                    "scripted store outage",
                )));
            }
            self.inner.fetch(credential_ref).await
        })
    }
}

/// The two services for the test account at `endpoint`, over the scripted
/// resolver and store, with short policies so that retries and exhaustion are
/// observable within the test.
fn services(endpoint: &str) -> (Arc<ScriptedAccounts>, Order, Agent) {
    let config: Config = serde_json::from_value(json!({
        "account": {
            "slug": "acct",
            "agent_key": AGENT_KEY,
            "endpoint": endpoint,
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
    // The static resolver of `config` behind the script, and a short resolve
    // policy so a scripted outage is retried within the test.
    let scripted = Arc::new(ScriptedAccounts::new(
        StaticResolver::try_from(&config).expect("resolver"),
    ));
    let accounts = Accounts::new(
        Arc::clone(&scripted) as Arc<dyn AccountResolver>,
        Arc::clone(&scripted) as Arc<dyn CredentialStore>,
    );
    let worker = WorkerConfig {
        resolve: ResolveConfig {
            initial_delay: Duration::from_secs(1),
            factor: 1.0,
            max_delay: Duration::from_secs(1),
            max_duration: Duration::from_secs(30),
        },
        ..WorkerConfig::from(&config)
    };
    let order = Order::from_parts(accounts.clone(), worker.clone());
    let agent = Agent::from_parts(accounts, worker);
    (scripted, order, agent)
}

struct Harness {
    restate: Restate,
    mock: MockServer,
    http: reqwest::Client,
    script: Arc<ScriptedAccounts>,
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

        // The server must run with the three experimental flags; a reused
        // server (from the environment) is checked the same way.
        let version: Value = http
            .get(format!("{}/version", restate.admin))
            .send()
            .await
            .expect("version")
            .json()
            .await
            .expect("version json");
        for feature in ["vqueues", "protocol_v7", "scoped_virtual_objects"] {
            assert_eq!(
                version["features"][feature],
                Value::Bool(true),
                "the Restate server must run with {feature} enabled: {version}"
            );
        }

        // Serve the endpoint on a free port and register it.
        let (scripted, order, agent) = services(&mock.uri());
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
            script: scripted,
        }
    }

    /// Calls `Szamlazz.Order.{handler}` on `key` with an `Idempotency-Key`,
    /// unscoped.
    async fn call(&self, key: &str, handler: &str, body: &Value, idempotency: &str) -> Reply {
        self.invoke(
            &format!("/restate/call/Szamlazz.Order/{key}/{handler}"),
            Some(body),
            Some(idempotency),
        )
        .await
    }

    /// Calls `Szamlazz.Order.{handler}` on `key` under `scope`
    /// (`/restate/scope/{scope}/call/…`).
    async fn call_scoped(
        &self,
        scope: &str,
        key: &str,
        handler: &str,
        body: &Value,
        idempotency: &str,
    ) -> Reply {
        self.invoke(
            &format!("/restate/scope/{scope}/call/Szamlazz.Order/{key}/{handler}"),
            Some(body),
            Some(idempotency),
        )
        .await
    }

    /// Calls `Szamlazz.Agent.{handler}` under `scope`.
    async fn call_agent_scoped(&self, scope: &str, handler: &str, body: &Value) -> Reply {
        self.invoke(
            &format!("/restate/scope/{scope}/call/Szamlazz.Agent/{handler}"),
            Some(body),
            None,
        )
        .await
    }

    async fn invoke(&self, path: &str, body: Option<&Value>, idempotency: Option<&str>) -> Reply {
        let mut request = self.http.post(format!("{}{path}", self.restate.ingress));
        if let Some(idempotency) = idempotency {
            request = request.header("idempotency-key", idempotency);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().await.expect("ingress call");
        let status = response.status().as_u16();
        let invocation_id = response
            .headers()
            .get("x-restate-id")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let text = response.text().await.expect("body");
        let body = serde_json::from_str(&text).unwrap_or(Value::String(text));
        Reply {
            status,
            body,
            invocation_id,
        }
    }

    async fn ok(&self, key: &str, handler: &str, body: &Value, idempotency: &str) -> Value {
        let reply = self.call(key, handler, body, idempotency).await;
        assert_eq!(reply.status, 200, "{handler} on {key}: {}", reply.body);
        reply.body
    }

    /// `Szamlazz.Order.get`: no input, no idempotency key.
    async fn get(&self, key: &str) -> Value {
        let reply = self
            .invoke(
                &format!("/restate/call/Szamlazz.Order/{key}/get"),
                None,
                None,
            )
            .await;
        assert_eq!(reply.status, 200, "get on {key}: {}", reply.body);
        reply.body
    }

    /// Runs a SQL query against the introspection API (`POST :9070/query`).
    async fn sql(&self, query: &str) -> Vec<Value> {
        let response = self
            .http
            .post(format!("{}/query", self.restate.admin))
            .header("accept", "application/json")
            .json(&json!({ "query": query }))
            .send()
            .await
            .expect("sql query");
        let status = response.status().as_u16();
        let body: Value = response.json().await.expect("sql json");
        assert_eq!(status, 200, "sql failed: {body}");
        body["rows"]
            .as_array()
            .unwrap_or_else(|| panic!("rows: {body}"))
            .clone()
    }

    /// The journal of an invocation, in index order.
    async fn journal(&self, invocation_id: &str) -> Vec<JournalEntry> {
        let rows = self
            .sql(&format!(
                "SELECT index, entry_type, name, raw FROM sys_journal WHERE id = '{invocation_id}' ORDER BY index"
            ))
            .await;
        rows.iter()
            .map(|row| JournalEntry {
                index: row["index"].as_u64().expect("index"),
                entry_type: row["entry_type"].as_str().unwrap_or_default().to_owned(),
                name: row["name"].as_str().map(str::to_owned),
                raw: row["raw"]
                    .as_str()
                    .map(|hex| decode_hex(hex).unwrap_or_else(|| panic!("hex raw: {hex}")))
                    .unwrap_or_default(),
            })
            .collect()
    }

    /// The `sys_invocation` row of an invocation.
    async fn invocation(&self, invocation_id: &str) -> Invocation {
        let rows = self
            .sql(&format!(
                "SELECT status, completion_failure, scope, target_handler_name FROM sys_invocation WHERE id = '{invocation_id}'"
            ))
            .await;
        let row = rows
            .first()
            .unwrap_or_else(|| panic!("no sys_invocation row for {invocation_id}"));
        Invocation {
            status: row["status"].as_str().unwrap_or_default().to_owned(),
            completion_failure: row["completion_failure"].as_str().map(str::to_owned),
            scope: row["scope"].as_str().map(str::to_owned),
            handler: row["target_handler_name"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
        }
    }

    /// Watches the invocations on Virtual Object `key` for four seconds (a
    /// detached task, so it carries its own copy of the query — `sql` borrows
    /// the harness) and records what `sys_invocation` reports **while they
    /// are in flight**:
    /// `retry_count`, `last_failure` and `last_failure_related_command_name`
    /// are attempt state, cleared once the invocation completes — a completed
    /// row shows neither the count nor the failing command (verified against
    /// 1.7.8). Start it before the call, await it after.
    fn watch(&self, key: &str) -> tokio::task::JoinHandle<Retries> {
        let admin = self.restate.admin.clone();
        let http = self.http.clone();
        let key = key.to_owned();
        tokio::spawn(async move {
            let mut retries = Retries::default();
            for _ in 0..40 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                let body: Value = http
                    .post(format!("{admin}/query"))
                    .header("accept", "application/json")
                    .json(&json!({
                        "query": format!(
                            "SELECT retry_count, last_failure, last_failure_related_command_name FROM sys_invocation WHERE target_service_key = '{key}'"
                        )
                    }))
                    .send()
                    .await
                    .expect("sql query")
                    .json()
                    .await
                    .expect("sql json");
                for row in body["rows"].as_array().into_iter().flatten() {
                    if let Some(count) = row["retry_count"].as_u64() {
                        retries.max_retry_count = retries.max_retry_count.max(count);
                    }
                    if let Some(failure) = row["last_failure"].as_str()
                        && !retries.failures.iter().any(|seen| seen == failure)
                    {
                        retries.failures.push(failure.to_owned());
                    }
                    if let Some(command) = row["last_failure_related_command_name"].as_str()
                        && !retries.failing_commands.iter().any(|seen| seen == command)
                    {
                        retries.failing_commands.push(command.to_owned());
                    }
                }
            }
            retries
        })
    }

    /// Purges a completed invocation (`PATCH /invocations/{id}/purge`), so a
    /// later call runs against an order Restate has no memory of.
    async fn purge(&self, invocation_id: &str) {
        let response = self
            .http
            .patch(format!(
                "{}/invocations/{invocation_id}/purge",
                self.restate.admin
            ))
            .send()
            .await
            .expect("purge");
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        assert!(
            (200..300).contains(&status),
            "purge of {invocation_id} failed ({status}): {body}"
        );
        // The purge is asynchronous; wait for the row to go.
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let rows = self
                .sql(&format!(
                    "SELECT id FROM sys_invocation WHERE id = '{invocation_id}'"
                ))
                .await;
            if rows.is_empty() {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "invocation {invocation_id} still present after purge"
            );
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
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
    harness_scoped_call_and_leak_positive_control(&h).await;
    purged_invocation_queries_szamlazz_again(&h).await;
    flaky_resolver_is_retried_by_the_resolve_policy(&h).await;
    failing_credential_store_is_a_terminal_unavailable(&h).await;
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

    let reply = h
        .call(
            "E2E-1",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-1-k1",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let first = &reply.body;
    assert_eq!(first["outcome"], "issued", "{first}");
    assert_eq!(first["invoice_number"], "SZ-1");
    assert_eq!(first["kind"], "invoice");
    assert_eq!(first["external_id"], "acct:E2E-1:invoice");
    assert_eq!(first["gross_total"], "1270");
    assert_eq!(first.get("gen"), None);
    assert_eq!(first.get("request_id"), None);

    // The prologue: the namespace pin and exactly one `account` entry, both
    // before the operation's first step; the journaled account carries its
    // id and never the agent key.
    let journal = h.journal(reply.invocation_id()).await;
    let runs: Vec<_> = journal
        .iter()
        .filter(|entry| entry.is_run())
        .filter_map(|entry| entry.name.as_deref())
        .collect();
    assert_eq!(
        &runs[..3],
        ["namespace", "account", "exclusivity-prepayment"],
        "{runs:?}"
    );
    assert_eq!(
        runs.iter().filter(|name| **name == "account").count(),
        1,
        "one account entry per invocation: {runs:?}"
    );
    let account = run_result(&journal, "account").expect("the account result");
    assert!(
        account.raw_contains("\"id\":\"acct\""),
        "{:?}",
        String::from_utf8_lossy(&account.raw)
    );
    assert!(
        !journal.iter().any(|entry| entry.raw_contains(AGENT_KEY)),
        "the agent key is in no journal entry"
    );

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
    let reply = h
        .call(
            "E2E-10",
            "create_prepayment",
            &json!({ "document": document(dec!(1000)), "options": { "proforma": "none" } }),
            "e2e-10-k1",
        )
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    assert_eq!(reply.fault().code, "invalid_input", "{}", reply.body);
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
    let watch = h.watch("E2E-11");
    let reply = h
        .call(
            "E2E-11",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-11-k1",
        )
        .await;
    let elapsed = started.elapsed();
    let retries = watch.await.expect("watch");
    assert_eq!(reply.status, 500, "{}", reply.body);
    assert!(
        elapsed < Duration::from_secs(60),
        "the run policy's delay was honoured, not the handler's: {elapsed:?}"
    );

    // The ingress wraps the handler's terminal error; the fault is the JSON
    // in its message.
    let fault = reply.fault();
    assert_eq!(fault.code, "outcome_unknown", "{fault:?}");
    assert_eq!(fault.order.as_deref(), Some("E2E-11"));
    assert_eq!(fault.kind.as_deref(), Some("invoice"));
    assert_eq!(fault.external_id.as_deref(), Some("acct:E2E-11:invoice"));
    assert!(
        fault.message.contains("retry with a new Idempotency-Key"),
        "{fault:?}"
    );

    // The run's re-execution is visible while the invocation is in flight:
    // `retry_count` counts it (at least 1; the server's exact accounting is
    // its own) with the create step named as the failing command — and the
    // completed invocation carries the structured fault.
    assert!(retries.max_retry_count >= 1, "{retries:?}");
    assert_eq!(
        retries.failing_commands,
        ["create-invoice"],
        "the run, not the handler, is what retried: {retries:?}"
    );
    assert!(
        retries
            .failures
            .iter()
            .all(|failure| failure.contains("transport failure")),
        "the last failure is the Unconfirmed message: {retries:?}"
    );
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.handler, "create_invoice");
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert!(
        invocation
            .completion_failure
            .as_deref()
            .is_some_and(|failure| failure.contains("outcome_unknown")),
        "{invocation:?}"
    );
    let journal = h.journal(reply.invocation_id()).await;
    let runs: Vec<_> = journal
        .iter()
        .filter(|entry| entry.is_run())
        .filter_map(|entry| entry.name.as_deref())
        .collect();
    assert!(
        runs.contains(&"lookup-invoice") && runs.contains(&"create-invoice"),
        "the two steps are journaled by name: {runs:?}"
    );
    eprintln!("(xi) exhausted create step → structured outcome_unknown; run retries visible: pass");
}

/// (xii) the harness capabilities of #29 that the multi-account tickets
/// assert through: a scoped call reaches the handler (the scope needs no
/// Virtual Object routing for `Szamlazz.Agent`, and is on the invocation
/// either way) — on this single-account deployment the prologue answers it
/// with `unknown_account` and nothing reaches szamlazz.hu; the leak check
/// has a positive control — a sentinel string in a wiremock rejection is
/// found in the hex-decoded `raw` of the create run's journal entry.
async fn harness_scoped_call_and_leak_positive_control(h: &Harness) {
    const SENTINEL: &str = "SENTINEL-8f3a2c-LEAK-CONTROL";
    h.reset().await;
    number_query("SZ-12")
        .respond_with(Doc::new("SZ-12", "SZ", "E2E-12").response())
        .expect(0)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent_scoped(
            "acme-events",
            "query",
            &json!({ "selector": { "invoice_number": "SZ-12" } }),
        )
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "unknown_account", "{fault:?}");
    assert!(fault.message.contains("acme-events"), "{fault:?}");
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(
        invocation.scope.as_deref(),
        Some("acme-events"),
        "{invocation:?}"
    );
    assert_eq!(invocation.handler, "query");

    // The same through the Virtual Object: the journal has the `account`
    // entry — the resolution is data — and nothing after it.
    let reply = h
        .call_scoped(
            "acme-events",
            "E2E-12",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-12-scoped",
        )
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    assert_eq!(reply.fault().code, "unknown_account", "{}", reply.body);
    let runs: Vec<_> = h
        .journal(reply.invocation_id())
        .await
        .into_iter()
        .filter(JournalEntry::is_run)
        .filter_map(|entry| entry.name)
        .collect();
    assert_eq!(runs, ["namespace", "account"], "{runs:?}");
    assert_eq!(h.requests_seen().await, 0, "nothing reached szamlazz.hu");

    // Positive control: the sentinel travels through szamlazz.hu's rejection
    // message into the create run's journaled result.
    h.reset().await;
    h.absent("E2E-12", &["prepayment", "proforma", "invoice"])
        .await;
    order_query("E2E-12")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create()
        .respond_with(api_error("259", SENTINEL))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call(
            "E2E-12",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-12-k1",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "rejected", "{}", reply.body);
    assert_eq!(reply.body["message"], SENTINEL);

    let journal = h.journal(reply.invocation_id()).await;
    let create_result = run_result(&journal, "create-invoice")
        .unwrap_or_else(|| panic!("the create-invoice run's result entry: {journal:?}"));
    assert!(
        create_result.raw_contains(SENTINEL),
        "the sentinel is found in the hex-decoded raw of entry {}: {:?}",
        create_result.index,
        String::from_utf8_lossy(&create_result.raw)
    );
    let lookup_result = run_result(&journal, "lookup-invoice").expect("the lookup's result");
    assert!(
        !lookup_result.raw_contains(SENTINEL),
        "the sentinel is not in an entry it did not pass through"
    );
    let leaked: Vec<u64> = journal
        .iter()
        .filter(|entry| entry.raw_contains(SENTINEL))
        .map(|entry| entry.index)
        .collect();
    assert_eq!(
        leaked,
        [create_result.index, journal.last().expect("output").index],
        "the sentinel is in exactly the create result and the output"
    );
    eprintln!(
        "(xii) scoped call on a single-account deployment → unknown_account; leak positive control: pass"
    );
}

/// (xiii) an order Restate has no memory of: the `get` invocation is purged
/// and a second `get` queries szamlazz.hu again (nothing is served from a
/// retained journal or a Virtual Object state).
async fn purged_invocation_queries_szamlazz_again(h: &Harness) {
    h.reset().await;
    h.absent("E2E-13", &["proforma", "invoice", "prepayment", "final"])
        .await;
    let reply = h
        .invoke("/restate/call/Szamlazz.Order/E2E-13/get", None, None)
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let first = h.requests_seen().await;
    assert_eq!(first, 4, "four external-id queries");
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert_eq!(
        h.journal(reply.invocation_id())
            .await
            .iter()
            .filter(|entry| entry.is_run())
            .count(),
        6,
        "get's journal is retained and inspectable: the prologue's two steps and four queries"
    );

    h.purge(reply.invocation_id()).await;
    assert!(
        h.journal(reply.invocation_id()).await.is_empty(),
        "the journal is gone with the invocation"
    );

    let reply = h
        .invoke("/restate/call/Szamlazz.Order/E2E-13/get", None, None)
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(
        h.requests_seen().await,
        first + 4,
        "szamlazz.hu is queried again"
    );
    eprintln!("(xiii) purged invocation → szamlazz.hu queried again: pass");
}

/// (xiv) the resolver fails twice, then answers: the `account` step is
/// re-executed under the resolve policy (one second apart under the test
/// policy, not the handler's two-minute `initial_interval`), the invocation
/// completes with the outcome, `sys_invocation.retry_count` shows the run's
/// retries with `account` as the failing command, and the journal holds one
/// `account` entry.
async fn flaky_resolver_is_retried_by_the_resolve_policy(h: &Harness) {
    h.reset().await;
    h.absent("E2E-14", &["prepayment", "proforma", "invoice"])
        .await;
    order_query("E2E-14")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-14", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;

    let resolutions_before = h.script.resolutions();
    h.script.fail_next_resolutions(2);
    let started = Instant::now();
    let watch = h.watch("E2E-14");
    let reply = h
        .call(
            "E2E-14",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-14-k1",
        )
        .await;
    let elapsed = started.elapsed();
    let retries = watch.await.expect("watch");
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "issued", "{}", reply.body);
    assert!(
        elapsed < Duration::from_secs(60),
        "the resolve policy's delay was honoured, not the handler's: {elapsed:?}"
    );
    assert_eq!(
        h.script.resolutions() - resolutions_before,
        3,
        "two failures, then the answer"
    );
    // Two run failures: the server counts at least both (its exact
    // accounting — 3 was observed — is its own).
    assert!(retries.max_retry_count >= 2, "{retries:?}");
    assert_eq!(retries.failing_commands, ["account"], "{retries:?}");
    assert!(
        retries.failures.iter().all(|failure| failure
            .contains("the account resolver is unavailable")
            && !failure.contains("scripted")),
        "the resolver's own message is never echoed: {retries:?}"
    );

    let journal = h.journal(reply.invocation_id()).await;
    let runs: Vec<_> = journal
        .iter()
        .filter(|entry| entry.is_run())
        .filter_map(|entry| entry.name.as_deref())
        .collect();
    assert_eq!(
        runs.iter().filter(|name| **name == "account").count(),
        1,
        "the failed executions journaled nothing: {runs:?}"
    );
    assert!(runs.contains(&"create-invoice"), "{runs:?}");
    eprintln!("(xiv) flaky resolver → retried under the resolve policy, one account entry: pass");
}

/// (xv) the credential store fails on every fetch: the handler ends with a
/// terminal `unavailable` (503) after the in-process retry, without a single
/// szamlazz.hu request; the `account` step is journaled (the resolution
/// succeeded), nothing after it.
async fn failing_credential_store_is_a_terminal_unavailable(h: &Harness) {
    h.reset().await;
    h.absent("E2E-15", &["prepayment", "proforma", "invoice"])
        .await;
    create()
        .respond_with(created("SZ-15", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;

    let fetches_before = h.script.fetches();
    h.script.set_store_down(true);
    let started = Instant::now();
    let reply = h
        .call(
            "E2E-15",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-15-k1",
        )
        .await;
    let elapsed = started.elapsed();
    h.script.set_store_down(false);
    assert_eq!(reply.status, 503, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "unavailable", "{fault:?}");
    assert!(fault.message.contains("credentials"), "{fault:?}");
    assert!(!fault.message.contains("scripted"), "{fault:?}");
    assert!(
        elapsed < Duration::from_secs(30),
        "terminal, not routed into the handler's retries: {elapsed:?}"
    );
    assert_eq!(
        h.script.fetches() - fetches_before,
        3,
        "the short in-process retry: three fetches"
    );
    assert_eq!(h.requests_seen().await, 0, "zero szamlazz.hu requests");

    let runs: Vec<_> = h
        .journal(reply.invocation_id())
        .await
        .into_iter()
        .filter(JournalEntry::is_run)
        .filter_map(|entry| entry.name)
        .collect();
    assert_eq!(runs, ["namespace", "account"], "{runs:?}");
    let invocation = h.invocation(reply.invocation_id()).await;
    assert!(
        invocation
            .completion_failure
            .as_deref()
            .is_some_and(|failure| failure.contains("unavailable")),
        "{invocation:?}"
    );

    // The store is back: the same order issues on the next call.
    h.reset().await;
    h.absent("E2E-15", &["prepayment", "proforma", "invoice"])
        .await;
    order_query("E2E-15")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-15", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let issued = h
        .ok(
            "E2E-15",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-15-k2",
        )
        .await;
    assert_eq!(issued["outcome"], "issued", "{issued}");
    eprintln!(
        "(xv) failing credential store → terminal unavailable, zero szamlazz.hu requests: pass"
    );
}
