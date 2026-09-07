//! End-to-end tests of the `Szamlazz.Order` Virtual Object and the
//! `Szamlazz.Agent` service against a real Restate server with wiremock
//! standing in for szamlazz.hu.
//!
//! The two end-to-end tests are ignored by default:
//! `cargo test -p restate-szamlazz --test service -- --ignored`.
//! The server comes from the environment, decided once ([`server_gate`]):
//! `RESTATE_ADMIN_URL` / `RESTATE_INGRESS_URL` reuse a running server (with the
//! three experimental flags, [`SERVER_FLAGS`] — `compose.yaml` sets them; the
//! main suite only), `RESTATE_SERVER_BIN` names a `restate-server` binary the
//! harness spawns on the loopback (what the Dagger check does), otherwise a
//! docker daemon runs a container of [`IMAGE`]. With none of them the suite
//! skips with a message — and fails when `CI` is set, since a skipped run in
//! CI proves nothing. The tests of the harness's own helpers at the end of
//! the file need only wiremock and run un-ignored.
//!
//! The main run has two phases on one Restate server. The first registers a
//! **single-account** deployment (the static resolver's `[account]` behind a
//! scripted resolver and store) and runs the order protocol unscoped. The
//! second performs the documented single → multi **flag day** — private,
//! drain, register the **multi-account** deployment (two accounts, reachable
//! by scope only, behind a test-local mutable resolver and store), public —
//! and proves the isolation properties multi-account mode leans on: the same
//! order key issuing concurrently under two scopes with each account's own
//! key on the wire, the same `Idempotency-Key` under two scopes being two
//! invocations, credential rotation and account changes between executions,
//! an order Restate has no memory of, and — over the hex-decoded `raw` of
//! every journal entry of every invocation in the run — that no agent key
//! was ever journaled; and, last, that every invocation's `ctx.run` names are
//! a prefix of its handler's pinned path ([`RUN_NAMES`]). The second test is
//! the protocol-v7 canary on a server of its own without the flag.
//!
//! The harness calls through the `/restate/call/…` and
//! `/restate/scope/{scope}/call/…` ingress paths, reports the invocation id
//! (`x-restate-id`) and parses fault bodies, and reads `sys_journal` /
//! `sys_invocation` through the SQL introspection API — `raw` hex-decoded to
//! bytes, since run results are stored as bytes. What szamlazz.hu holds is
//! stated per document ([`Harness::holds`] and its siblings), so one `<szamla>`
//! body answers every selector the document is reachable by (design §11).

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use jiff::civil::{Date, date};
use restate_sdk::prelude::{Endpoint, HttpServer};
use restate_sdk::service::Discoverable;
use restate_szamlazz::account::{
    Account, AccountResolver, Accounts, BoxFuture, CredentialRef, CredentialStore, FetchError,
    ResolveError, StaticConfig, StaticResolver,
};
use restate_szamlazz::config::WorkerConfig;
use restate_szamlazz::contract::{
    BuyerInput, DocumentInput, DocumentState, LineItemInput, OrderStatus, PaymentMethod,
};
use restate_szamlazz::{Agent, Order};
use rust_decimal::{Decimal, dec};
use serde::Deserialize;
use serde_json::{Value, json};
use szamlazz_agent::Credentials;
use wiremock::matchers::{body_string_contains, method};
use wiremock::{Mock, MockBuilder, MockServer, Request, ResponseTemplate};

/// The `szallito/id` the rendered documents carry: the seller record's id as
/// szamlazz.hu prints it in a query body (972720 on the test account). Wire
/// realism only — the worker holds no account pin (ADR 0006, account-pin
/// amendment); the scenario that renders [`SUPPLIER_B`] asserts exactly that.
const SUPPLIER: u64 = 972_720;
/// Another seller record's id: what a document of another szamlazz.hu account
/// would carry. Compared with nothing.
const SUPPLIER_B: u64 = 972_721;
/// The agent key of the test account (and of `acme` after the flag day): a
/// sentinel that must never appear in a journal entry.
const AGENT_KEY: &str = "e2e-agent-key-sentinel-7d1f4b";
/// The agent key of the `beta` account; a second sentinel.
const KEY_B: &str = "e2e-agent-key-b-sentinel-2e7f41";
/// `beta`'s key after the rotation scenario; a third sentinel.
const KEY_B_V2: &str = "e2e-agent-key-b-rotated-sentinel-5ba9c0";
/// Every agent key the run puts on the wire; none may be journaled.
const AGENT_KEYS: [&str; 3] = [AGENT_KEY, KEY_B, KEY_B_V2];
/// `acme`'s seller bank account in the multi-account phase, and the value
/// the account-change scenario replaces it with.
const BANK_ACCOUNT: &str = "11111111-22222222-33333333";
const BANK_ACCOUNT_CHANGED: &str = "44444444-55555555-66666666";
const IMAGE: &str = "docker.restate.dev/restatedev/restate:1.7.8";

// ----- szamlazz.hu fixtures (mirroring tests/gateway.rs) --------------------

/// The `telj` every document of the run carries unless a scenario says
/// otherwise: the fulfillment date a storno of it must repeat (ADR 0007).
const ORIGINAL_TELJ: Date = date(2026, 7, 15);

struct Doc<'a> {
    number: &'a str,
    tipus: &'a str,
    order: Option<&'a str>,
    reversed: bool,
    referenced_invoice: Option<&'a str>,
    referenced_proforma: Option<&'a str>,
    /// `teszt` — whether a test account issued the document. Projected by
    /// `query`, compared with nothing (ADR 0006, account-pin amendment).
    test: bool,
    /// `szallito/id` — the seller record's id in the `<szallito>` block.
    /// Parsed, compared with nothing.
    supplier_id: u64,
    /// The external id the document sits under, when the test states it:
    /// szamlazz.hu never echoes it, so it is not in the body, but it is a
    /// selector the document is reachable by ([`holds`]).
    external_id: Option<&'a str>,
    /// `telj`; `None` renders no element — szamlazz.hu breaking its schema.
    fulfillment_date: Option<Date>,
    /// `eszamla`; `None` follows `tipus` — `0` on a proforma, `2` (an
    /// e-invoice code) on anything else. szamlazz.hu reports `1` for a paper
    /// invoice and `3` for one created with `eszamla=true` (P73).
    eszamla: Option<i32>,
}

impl<'a> Doc<'a> {
    /// A live test-account document of `order`.
    const fn new(number: &'a str, tipus: &'a str, order: &'a str) -> Self {
        Self {
            order: Some(order),
            ..Self::unmanaged(number, tipus)
        }
    }

    /// A live test-account document carrying no order number: issued outside
    /// the worker, reachable by number only.
    const fn unmanaged(number: &'a str, tipus: &'a str) -> Self {
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
        }
    }

    fn response(&self) -> ResponseTemplate {
        let opt = |tag: &str, value: Option<&str>| {
            value.map_or_else(String::new, |value| format!("<{tag}>{value}</{tag}>"))
        };
        let eszamla = self
            .eszamla
            .unwrap_or(if self.tipus == "D" { 0 } else { 2 });
        let telj = self.fulfillment_date.map(|date| date.to_string());
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<szamla xmlns="http://www.szamlazz.hu/szamla">
  <szallito><id>{supplier_id}</id><nev>Seller</nev><cim><irsz>1111</irsz><telepules>Budapest</telepules><cim>Fő u. 1.</cim></cim></szallito>
  <alap><id>924307338</id><szamlaszam>{number}</szamlaszam><tipus>{tipus}</tipus><eszamla>{eszamla}</eszamla>{hivszamlaszam}{hivdijbekszam}<kelt>2026-09-03</kelt>{telj}{rendelesszam}<teszt>{test}</teszt>{sztornozott}</alap>
  <vevo><nev>Buyer</nev></vevo>
  <tetelek></tetelek>
  <osszegek><totalossz><netto>1000</netto><afa>270</afa><brutto>1270</brutto></totalossz></osszegek>
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

/// The `<szamlaagentkulcs>` element carrying `agent_key`: what tells one
/// account's traffic from another's on the wire.
fn agent_key_tag(agent_key: &str) -> String {
    format!("<szamlaagentkulcs>{agent_key}</szamlaagentkulcs>")
}

/// A create request carrying `agent_key`.
fn create_with_key(agent_key: &str) -> MockBuilder {
    create().and(body_string_contains(agent_key_tag(agent_key)))
}

/// A create request whose seller block carries `bank_account`.
fn create_with_bank_account(bank_account: &str) -> MockBuilder {
    create().and(body_string_contains(format!(
        "<bankszamlaszam>{bank_account}</bankszamlaszam>"
    )))
}

fn storno() -> MockBuilder {
    op("action-szamla_agent_st")
}

/// The `<teljesitesDatum>` element carrying [`ORIGINAL_TELJ`]: the storno
/// repeating the original's fulfillment date (ADR 0007).
fn original_telj_tag() -> String {
    format!("<teljesitesDatum>{ORIGINAL_TELJ}</teljesitesDatum>")
}

/// A storno request carrying the fixture's `telj` ([`ORIGINAL_TELJ`]) as its
/// `teljesitesDatum` — what every storno of a fixture document must send.
fn storno_repeating_telj() -> MockBuilder {
    storno().and(body_string_contains(original_telj_tag()))
}

/// The body of a `storno_invoice` / `Szamlazz.Agent.storno` call on `number`.
fn storno_of(number: &str) -> Value {
    json!({ "invoice_number": number })
}

/// A storno request that must not reach szamlazz.hu: a handler that stops
/// before sending.
async fn storno_never_sent(mock: &MockServer) {
    storno()
        .respond_with(created("SS-X", "-1000", "-1270"))
        .expect(0)
        .mount(mock)
        .await;
}

/// The sentinel external id `check_account` probes under the run's namespace.
const PROBE_ID: &str = "acct:check-account";

/// The probe's query carrying `agent_key`: which account's key was checked.
fn probe_with_key(agent_key: &str) -> MockBuilder {
    external_id_query(PROBE_ID).and(body_string_contains(agent_key_tag(agent_key)))
}

/// The taxpayer query (`xmltaxpayer`) of `prefix` carrying `agent_key`:
/// which account's key NAV was asked with.
fn taxpayer_query_with_key(prefix: &str, agent_key: &str) -> MockBuilder {
    op("action-szamla_agent_taxpayer")
        .and(body_string_contains(format!(
            "<torzsszam>{prefix}</torzsszam>"
        )))
        .and(body_string_contains(agent_key_tag(agent_key)))
}

/// NAV's answer for a known taxpayer (the agent crate's synthetic fixture).
fn taxpayer_known() -> ResponseTemplate {
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
fn taxpayer_unknown() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<QueryTaxpayerResponse xmlns="http://schemas.nav.gov.hu/OSA/2.0/api"><result><funcCode>OK</funcCode></result><taxpayerValidity>false</taxpayerValidity></QueryTaxpayerResponse>"#,
        "application/xml",
    )
}

// ----- what szamlazz.hu holds: one document, every selector ------------------

/// szamlazz.hu holds `doc`: one body on every selector the document is
/// reachable by — its number, its order number when it carries one, its
/// external id when the test states one — so the stubs cannot disagree.
async fn holds(mock: &MockServer, doc: &Doc<'_>) {
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
/// once; whatever is mounted after this answers from the second query on —
/// wiremock takes the first active match in mount order, and an
/// `up_to_n_times(1)` mock is inactive after its one match.
async fn loses_reply_once(mock: &MockServer, id: &str) {
    external_id_query(id)
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .mount(mock)
        .await;
}

/// szamlazz.hu holds `doc` under its external id from the `misses + 1`th
/// query on: code 7 for `misses` queries, the document afterwards. The number
/// and order selectors are not mounted — the document is absent before the
/// misses and nothing reads it by number or order after. `doc` must state its
/// external id.
async fn holds_after_misses(mock: &MockServer, misses: u64, doc: &Doc<'_>) {
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

/// The create lands on szamlazz.hu but its reply is lost (design §5 step 4,
/// ADR 0003): `create()` answers 500, `expect(1)`, and `doc` is the holder of
/// its external id from the moment the create request is received — code 7
/// before, the document after. The transition is the create stub being
/// matched (one flag, flipped by the create's responder and read by the
/// external id's), so how many queries precede the send is not the test's to
/// know. Failure-injection sequencing, not a model of szamlazz.hu: one flag
/// for one document. `doc` must state its external id; the number and order
/// selectors are not mounted.
async fn create_lands_but_reply_lost(mock: &MockServer, doc: &Doc<'_>) {
    let id = doc
        .external_id
        .expect("create_lands_but_reply_lost needs the document's external id");
    let landed = Arc::new(AtomicBool::new(false));
    let flip = Arc::clone(&landed);
    create()
        .respond_with(move |_: &Request| {
            flip.store(true, Ordering::SeqCst);
            ResponseTemplate::new(500)
        })
        .expect(1)
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
}

// ----- request bodies ----------------------------------------------------------

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

/// Where the suite's Restate server comes from, decided from the environment
/// before anything starts ([`server_gate`]).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Launcher {
    /// `RESTATE_ADMIN_URL` / `RESTATE_INGRESS_URL`: a running server, with the
    /// three flags on ([`SERVER_FLAGS`]); nothing is started or stopped.
    Reuse { admin: String, ingress: String },
    /// `RESTATE_SERVER_BIN`: a `restate-server` binary the harness spawns on
    /// this host, on the ports the server spec names — what the Dagger check
    /// uses, where there is no docker.
    Binary(PathBuf),
    /// The docker daemon: a container of [`IMAGE`].
    Docker,
}

/// The server gate, the decision behind [`launcher_or_skip`]: `reuse` first,
/// then `binary`, then docker (probed only when neither is given); with none
/// of them `Ok(None)` — a skip — unless `ci` is set (non-empty), in which
/// case the suite must not pass by skipping and the answer is the failure
/// message, naming every way to provide a server.
fn server_gate(
    reuse: Option<(String, String)>,
    binary: Option<PathBuf>,
    docker_available: impl FnOnce() -> bool,
    ci: Option<&OsStr>,
) -> Result<Option<Launcher>, String> {
    if let Some((admin, ingress)) = reuse {
        return Ok(Some(Launcher::Reuse { admin, ingress }));
    }
    if let Some(binary) = binary {
        return Ok(Some(Launcher::Binary(binary)));
    }
    if docker_available() {
        return Ok(Some(Launcher::Docker));
    }
    if ci.is_some_and(|value| !value.is_empty()) {
        return Err(
            "no Restate server to run the end-to-end suite against, and CI is set: a skipped run \
             proves nothing. Provide one by setting RESTATE_SERVER_BIN to a restate-server binary \
             (spawned on this host), by making a docker daemon reachable (a container of the \
             Restate image), or by setting RESTATE_ADMIN_URL and RESTATE_INGRESS_URL to a running \
             server with the three experimental flags."
                .to_owned(),
        );
    }
    Ok(None)
}

/// Whether the suite may reuse a server from the environment: the main suite
/// may, a suite that needs a server of its own shape (without a flag) may not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reuse {
    Allowed,
    Never,
}

/// The launcher the environment provides, or `None` after printing why the
/// suite skips; panics with the gate's message under `CI`.
fn launcher_or_skip(reuse: Reuse) -> Option<Launcher> {
    let reusable = match reuse {
        Reuse::Allowed => std::env::var("RESTATE_ADMIN_URL")
            .ok()
            .zip(std::env::var("RESTATE_INGRESS_URL").ok()),
        Reuse::Never => None,
    };
    let binary = std::env::var_os("RESTATE_SERVER_BIN")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    match server_gate(
        reusable,
        binary,
        docker_available,
        std::env::var_os("CI").as_deref(),
    ) {
        Ok(Some(launcher)) => Some(launcher),
        Ok(None) => {
            eprintln!(
                "skipping: no Restate server (no docker daemon, no RESTATE_SERVER_BIN, no \
                 RESTATE_ADMIN_URL / RESTATE_INGRESS_URL)"
            );
            None
        }
        Err(message) => panic!("{message}"),
    }
}

/// The `max_attempts` of `handler`'s invocation retry policy as the service
/// discovers it — what the deployment registered with the server, read from
/// the same source rather than copied.
fn discovered_max_attempts<S: Discoverable>(handler: &str) -> u64 {
    S::discover()
        .handlers
        .into_iter()
        .find(|h| h.name.as_str() == handler)
        .unwrap_or_else(|| panic!("no handler {handler}"))
        .retry_policy_max_attempts
        .unwrap_or_else(|| panic!("{handler} pins no max_attempts"))
}

/// The three experimental server features multi-account mode depends on
/// (design §4, ADR 0006) — vqueues, protocol v7 (below it the SDK sees no
/// scope) and scoped Virtual Objects — as `/version` reports each, with the
/// environment flag that enables it.
const FEATURES: [(&str, &str); 3] = [
    ("vqueues", "RESTATE_EXPERIMENTAL_ENABLE_VQUEUES=true"),
    (
        "protocol_v7",
        "RESTATE_EXPERIMENTAL_ENABLE_PROTOCOL_V7=true",
    ),
    (
        "scoped_virtual_objects",
        "RESTATE_EXPERIMENTAL_ENABLE_SCOPED_VIRTUAL_OBJECTS=true",
    ),
];

/// The flags of the main suite's server: all three. Set on the server the
/// harness starts and expected of one reused through the environment.
const SERVER_FLAGS: [&str; 3] = [FEATURES[0].1, FEATURES[1].1, FEATURES[2].1];

/// The shape of a server the harness starts: its flags and the host ports of
/// its ingress and admin APIs (and, for a spawned binary, of its node port).
/// Two suites in one test binary run concurrently, so each has its own.
struct ServerSpec {
    flags: &'static [&'static str],
    ingress_port: u16,
    admin_port: u16,
    node_port: u16,
}

/// The main suite's server: the three flags.
const MAIN_SERVER: ServerSpec = ServerSpec {
    flags: &SERVER_FLAGS,
    ingress_port: 18080,
    admin_port: 19070,
    node_port: 15122,
};

/// The protocol-v7 canary's server: vqueues and scoped Virtual Objects on,
/// protocol v7 off — a deployment that forgot the one flag the scope needs to
/// reach the SDK. Its own ports: the two suites run concurrently.
const WITHOUT_PROTOCOL_V7: ServerSpec = ServerSpec {
    flags: &[FEATURES[0].1, FEATURES[2].1],
    ingress_port: 18081,
    admin_port: 19071,
    node_port: 15222,
};

/// A Restate server: an existing one (from the environment), a `restate-server`
/// process, or a container — the last two stopped on drop.
struct Restate {
    admin: String,
    ingress: String,
    /// The flags the server runs with — what `/version` must report.
    flags: &'static [&'static str],
    /// The host name under which the server reaches this process's endpoint.
    endpoint_host: String,
    container: Option<String>,
    process: Option<Child>,
    /// The spawned server's base directory, removed on drop unless the test
    /// is failing — then it stays, with `restate-server.log` in it.
    base_dir: Option<PathBuf>,
}

impl Launcher {
    /// The server of `spec`'s shape: started from the binary or the image, or
    /// the running one taken as it is (checked against `spec`'s flags like
    /// the others — the caller reuses only where the shape is the main
    /// suite's).
    fn launch(self, spec: &ServerSpec) -> Restate {
        let endpoint_host = |default: &str| {
            std::env::var("RESTATE_ENDPOINT_HOST").unwrap_or_else(|_| default.to_owned())
        };
        match self {
            Self::Reuse { admin, ingress } => {
                let mut restate =
                    Restate::on_host_ports(spec, endpoint_host("host.docker.internal"));
                restate.admin = admin;
                restate.ingress = ingress;
                restate
            }
            Self::Binary(binary) => Restate::spawn(&binary, spec, endpoint_host("127.0.0.1")),
            Self::Docker => Restate::container(spec, endpoint_host("host.docker.internal")),
        }
    }
}

impl Restate {
    /// A server reachable on `spec`'s host ports with `spec`'s flags, running
    /// nothing of its own yet: what every launcher fills in.
    fn on_host_ports(spec: &ServerSpec, endpoint_host: String) -> Self {
        Self {
            admin: format!("http://127.0.0.1:{}", spec.admin_port),
            ingress: format!("http://127.0.0.1:{}", spec.ingress_port),
            flags: spec.flags,
            endpoint_host,
            container: None,
            process: None,
            base_dir: None,
        }
    }

    /// A container of [`IMAGE`] with `spec`'s flags, its ingress and admin
    /// ports published on `spec`'s host ports.
    fn container(spec: &ServerSpec, endpoint_host: String) -> Self {
        let mut args = vec![
            "run".to_owned(),
            "--rm".to_owned(),
            "-d".to_owned(),
            // Docker Desktop resolves `host.docker.internal` on its own;
            // a Linux daemon needs the alias to reach the endpoint.
            "--add-host=host.docker.internal:host-gateway".to_owned(),
            "-p".to_owned(),
            format!("{}:8080", spec.ingress_port),
            "-p".to_owned(),
            format!("{}:9070", spec.admin_port),
        ];
        for flag in spec.flags {
            args.push("-e".to_owned());
            args.push((*flag).to_owned());
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
        let mut restate = Self::on_host_ports(spec, endpoint_host);
        restate.container = Some(container);
        restate
    }

    /// A `restate-server` process from `binary` with `spec`'s flags, bound to
    /// the loopback on `spec`'s ports, its data and log under a directory of
    /// its own in the temp dir. Configured through Restate's environment
    /// (`RESTATE_<SECTION>__<KEY>`), so no config file is written.
    fn spawn(binary: &PathBuf, spec: &ServerSpec, endpoint_host: String) -> Self {
        let base_dir = std::env::temp_dir().join(format!(
            "restate-szamlazz-e2e-{}-{}",
            std::process::id(),
            spec.admin_port
        ));
        fs::create_dir_all(&base_dir).expect("the server's base dir");
        let log = fs::File::create(base_dir.join("restate-server.log")).expect("the server log");
        let mut command = Command::new(binary);
        command
            .arg("--no-logo")
            .env("RESTATE_BASE_DIR", &base_dir)
            .env("RESTATE_NODE_NAME", format!("e2e-{}", spec.admin_port))
            .env("RESTATE_LISTEN_MODE", "tcp")
            .env(
                "RESTATE_BIND_ADDRESS",
                format!("127.0.0.1:{}", spec.node_port),
            )
            .env(
                "RESTATE_ADVERTISED_ADDRESS",
                format!("http://127.0.0.1:{}", spec.node_port),
            )
            .env(
                "RESTATE_INGRESS__BIND_ADDRESS",
                format!("127.0.0.1:{}", spec.ingress_port),
            )
            .env(
                "RESTATE_ADMIN__BIND_ADDRESS",
                format!("127.0.0.1:{}", spec.admin_port),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone().expect("the server log")))
            .stderr(Stdio::from(log));
        for flag in spec.flags {
            let (name, value) = flag.split_once('=').expect("NAME=value");
            command.env(name, value);
        }
        let process = command
            .spawn()
            .unwrap_or_else(|error| panic!("spawn {}: {error}", binary.display()));
        eprintln!(
            "restate-server (pid {}) on admin {} / ingress {}, base dir {}",
            process.id(),
            spec.admin_port,
            spec.ingress_port,
            base_dir.display()
        );
        let mut restate = Self::on_host_ports(spec, endpoint_host);
        restate.process = Some(process);
        restate.base_dir = Some(base_dir);
        restate
    }
}

impl Drop for Restate {
    fn drop(&mut self) {
        if let Some(container) = &self.container {
            let _ = Command::new("docker")
                .args(["rm", "-f", container])
                .output();
        }
        if let Some(process) = &mut self.process {
            let _ = process.kill();
            let _ = process.wait();
        }
        if let Some(base_dir) = &self.base_dir {
            if std::thread::panicking() {
                eprintln!(
                    "restate-server's base dir is kept for inspection: {}",
                    base_dir.display()
                );
            } else {
                let _ = fs::remove_dir_all(base_dir);
            }
        }
    }
}

/// An ingress reply: the status, the parsed body, the invocation id the
/// ingress reports in `x-restate-id` and the `x-restate-error-source` header
/// of an error reply.
#[derive(Debug)]
struct Reply {
    status: u16,
    body: Value,
    invocation_id: Option<String>,
    error_source: Option<String>,
}

impl Reply {
    fn invocation_id(&self) -> &str {
        self.invocation_id
            .as_deref()
            .unwrap_or_else(|| panic!("no x-restate-id on the reply: {}", self.body))
    }

    /// The structured fault inside the ingress error envelope, asserting the
    /// envelope the endpoint README documents (*Faults*): the body is
    /// Restate's `{"code": <HTTP status>, "message": "<string>", "source":
    /// "invocation"}`, `x-restate-error-source` is `invocation`, and the
    /// worker's fault is the JSON **string** in `message` — the handler's
    /// `TerminalError` message — parsed a second time.
    fn fault(&self) -> Fault {
        assert_eq!(
            self.body["code"].as_u64(),
            Some(u64::from(self.status)),
            "the envelope's code is the HTTP status: {}",
            self.body
        );
        assert_eq!(
            self.body["source"], "invocation",
            "a fault is the invocation's terminal error: {}",
            self.body
        );
        assert_eq!(
            self.error_source.as_deref(),
            Some("invocation"),
            "x-restate-error-source marks the fault as the worker's: {}",
            self.body
        );
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
    szamlazz_code: Option<String>,
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
    /// One `sys_journal` row (`index`, `entry_type`, `name`, `raw`).
    fn from_row(row: &Value) -> Self {
        Self {
            index: row["index"].as_u64().expect("index"),
            entry_type: row["entry_type"].as_str().unwrap_or_default().to_owned(),
            name: row["name"].as_str().map(str::to_owned),
            raw: row["raw"]
                .as_str()
                .map(|hex| decode_hex(hex).unwrap_or_else(|| panic!("hex raw: {hex}")))
                .unwrap_or_default(),
        }
    }

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

/// One pinned path of [`RUN_NAMES`]: a handler of a service and the ordered
/// `ctx.run` names it journals on that path.
struct RunPath {
    service: &'static str,
    handler: &'static str,
    path: &'static [&'static str],
}

impl RunPath {
    const fn new(
        service: &'static str,
        handler: &'static str,
        path: &'static [&'static str],
    ) -> Self {
        Self {
            service,
            handler,
            path,
        }
    }
}

/// The durable steps of every handler of both services, in order, as the
/// `ctx.run` names they journal — the part of the journal contract the type
/// fixtures (`tests/journal/`) do not cover. An in-flight invocation replays
/// the *previous* deployment's entries by name and position (ADR 0005), so a
/// renamed, inserted or reordered step strands it; this table makes that a
/// failing test instead of a killed invocation. A `{number}` / `{prefix}`
/// segment is a parameter ([`run_pattern`]); a handler with two rows has two
/// paths. The pin holds when every observed sequence of a handler is a prefix
/// of one of its paths (a handler that answers early journals the first steps
/// only — [`is_prefix_of_path`]) and every path is observed in full at least
/// once in the run. The parameter of a parametrized name is pinned by its
/// prefix only: a number that itself began with a pinned stem (`storno-1`)
/// would read as the longer pattern — none of the suite's do.
const RUN_NAMES: &[RunPath] = &[
    RunPath::new(
        "Szamlazz.Order",
        "create_proforma",
        &[
            "namespace",
            "account",
            "exclusivity-invoice",
            "exclusivity-prepayment",
            "exclusivity-final",
            "lookup-proforma",
            "create-proforma",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "create_invoice",
        &[
            "namespace",
            "account",
            "exclusivity-prepayment",
            "exclusivity-final",
            "proforma-link",
            "lookup-invoice",
            "create-invoice",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "create_invoice",
        &[
            "namespace",
            "account",
            "exclusivity-prepayment",
            "exclusivity-final",
            "verify-proforma-{number}",
            "lookup-invoice",
            "create-invoice",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "create_prepayment",
        &[
            "namespace",
            "account",
            "exclusivity-invoice",
            "exclusivity-final",
            "proforma-link",
            "lookup-prepayment",
            "create-prepayment",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "create_prepayment",
        &[
            "namespace",
            "account",
            "exclusivity-invoice",
            "exclusivity-final",
            "verify-proforma-{number}",
            "lookup-prepayment",
            "create-prepayment",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "create_final",
        &[
            "namespace",
            "account",
            "prepayment-for-final",
            "lookup-final",
            "create-final",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "correct_invoice",
        &[
            "namespace",
            "account",
            "verify-base-{number}",
            "lookup-corrective",
            "create-corrective",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "storno_invoice",
        &[
            "namespace",
            "account",
            "verify-storno-{number}",
            "lookup-storno-{number}",
            "storno-{number}",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "storno_invoice",
        &[
            "namespace",
            "account",
            "verify-storno-{number}",
            "hint-storno-{number}",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "delete_proforma",
        &[
            "namespace",
            "account",
            "proforma-for-delete",
            "delete-proforma-{number}",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "get",
        &[
            "namespace",
            "account",
            "get-proforma",
            "get-invoice",
            "get-prepayment",
            "get-final",
        ],
    ),
    RunPath::new(
        "Szamlazz.Agent",
        "check_account",
        &["namespace", "account", "probe"],
    ),
    RunPath::new(
        "Szamlazz.Agent",
        "query",
        &["namespace", "account", "query"],
    ),
    RunPath::new(
        "Szamlazz.Agent",
        "query_taxpayer",
        &["namespace", "account", "taxpayer-{prefix}"],
    ),
    RunPath::new(
        "Szamlazz.Agent",
        "set_payments",
        &["namespace", "account", "set-payments-{number}"],
    ),
    RunPath::new(
        "Szamlazz.Agent",
        "storno",
        &[
            "namespace",
            "account",
            "verify-{number}",
            "lookup-storno-{number}",
            "storno-{number}",
        ],
    ),
];

/// The parametrized run names of [`RUN_NAMES`] — every `{…}` pattern — by
/// the prefix that names the step, longest prefix first, so that a name is
/// read as the most specific pattern it starts with: `verify-storno-…` is
/// `verify-storno-{number}`, never `verify-{number}`. Derived from the table,
/// so the two cannot disagree.
static PARAMETRIZED_RUNS: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(|| {
    let mut patterns: Vec<(&str, &str)> = RUN_NAMES
        .iter()
        .flat_map(|row| row.path.iter())
        .filter_map(|pattern| {
            pattern
                .split_once('{')
                .map(|(prefix, _)| (prefix, *pattern))
        })
        .collect();
    patterns.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.cmp(b)));
    patterns.dedup();
    patterns
});

/// The [`RUN_NAMES`] pattern of a journaled run name: a parametrized name by
/// its prefix ([`PARAMETRIZED_RUNS`]), any other name as it is.
fn run_pattern(name: &str) -> String {
    PARAMETRIZED_RUNS
        .iter()
        .find(|(prefix, _)| name.starts_with(prefix) && name.len() > prefix.len())
        .map_or_else(|| name.to_owned(), |(_, pattern)| (*pattern).to_owned())
}

/// Whether `observed` (patterns, in journal order) is a prefix of `path`.
fn is_prefix_of_path(observed: &[String], path: &[&str]) -> bool {
    observed.len() <= path.len()
        && observed
            .iter()
            .zip(path)
            .all(|(seen, expected)| seen == expected)
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
    service: String,
    handler: String,
}

impl Invocation {
    /// The columns every `sys_invocation` query of the harness selects.
    const COLUMNS: &str =
        "status, completion_failure, scope, target_service_name, target_handler_name";

    /// One `sys_invocation` row with [`Self::COLUMNS`].
    fn from_row(row: &Value) -> Self {
        Self {
            status: row["status"].as_str().unwrap_or_default().to_owned(),
            completion_failure: row["completion_failure"].as_str().map(str::to_owned),
            scope: row["scope"].as_str().map(str::to_owned),
            service: row["target_service_name"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            handler: row["target_handler_name"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
        }
    }
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
    let accounts: StaticConfig = serde_json::from_value(json!({
        "account": {
            "id": "acct",
            "agent_key": AGENT_KEY,
            "endpoint": endpoint,
        },
    }))
    .expect("config");
    // The static resolver behind the script, and a short resolve policy so a
    // scripted outage is retried within the test.
    let scripted = Arc::new(ScriptedAccounts::new(
        StaticResolver::try_from(accounts).expect("resolver"),
    ));
    let accounts = Accounts::new(
        Arc::clone(&scripted) as Arc<dyn AccountResolver>,
        Arc::clone(&scripted) as Arc<dyn CredentialStore>,
    );
    let order = Order::from_parts(accounts.clone(), worker_config());
    let agent = Agent::from_parts(accounts, worker_config());
    (scripted, order, agent)
}

/// The deployment-level settings of both phases — the flag day keeps the
/// namespace — with short policies: two executions of the create step one
/// second apart, so exhaustion and re-execution are observable within the
/// test; three executions of a read step one second apart, so a retried and
/// an exhausted read are observable within the `watch` window; and a
/// one-second resolve policy so a scripted outage is retried within it.
///
/// Built in Rust and handed to `from_parts`, never through
/// `WorkerConfig::validate`: the 1 s issue delay is under the floor `validate`
/// holds a deployment to (`IssueConfig::MIN_INITIAL_DELAY`, the client timeout
/// plus a margin), which the endpoint's loader enforces and this suite — whose
/// szamlazz.hu is a scripted mock that answers at once — has no use for.
fn worker_config() -> WorkerConfig {
    serde_json::from_value(json!({
        "namespace": "acct",
        "issue": {
            "max_attempts": 2,
            "initial_delay": "1s",
            "factor": 2.0,
            "max_delay": "2s",
            "max_duration": "1m",
        },
        "read": {
            "max_attempts": 3,
            "initial_delay": "1s",
            "factor": 1.0,
            "max_delay": "1s",
            "max_duration": "30s",
        },
        "resolve": {
            "initial_delay": "1s",
            "factor": 1.0,
            "max_delay": "1s",
            "max_duration": "30s",
        },
    }))
    .expect("config")
}

/// A resolver and store whose accounts and keys the test can change while
/// invocations are in flight — what a database-backed deployment looks like
/// to the worker, and what the rotation and account-change scenarios drive.
/// Seeded from the static resolver's multi-account shape, so the shape is
/// exercised end to end too.
#[derive(Debug)]
struct MutableAccounts {
    /// The accounts by the scope each is reachable under.
    accounts: Mutex<BTreeMap<String, Account>>,
    /// The credentials by credential reference.
    keys: Mutex<BTreeMap<String, Credentials>>,
}

impl MutableAccounts {
    async fn seeded_from(resolver: &StaticResolver) -> Self {
        let mut accounts = BTreeMap::new();
        let mut keys = BTreeMap::new();
        for (scope, account) in resolver.accounts() {
            let scope = scope.expect("the multi-account shape scopes every account");
            let credentials = resolver
                .fetch(&account.credential_ref)
                .await
                .expect("the inline key");
            keys.insert(account.credential_ref.to_string(), credentials);
            accounts.insert(scope.to_owned(), account.clone());
        }
        Self {
            accounts: Mutex::new(accounts),
            keys: Mutex::new(keys),
        }
    }

    /// Replaces the credentials under `credential_ref`: a rotation.
    fn rotate(&self, credential_ref: &str, agent_key: &str) {
        self.keys
            .lock()
            .expect("keys")
            .insert(credential_ref.to_owned(), Credentials::agent_key(agent_key));
    }

    /// Changes the account reachable under `scope` in place.
    fn update(&self, scope: &str, change: impl FnOnce(&mut Account)) {
        let mut accounts = self.accounts.lock().expect("accounts");
        change(
            accounts
                .get_mut(scope)
                .unwrap_or_else(|| panic!("no account under scope {scope}")),
        );
    }
}

impl AccountResolver for MutableAccounts {
    fn resolve<'a>(
        &'a self,
        scope: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Account, ResolveError>> {
        Box::pin(async move {
            let Some(scope) = scope else {
                return Err(ResolveError::Unscoped);
            };
            self.accounts
                .lock()
                .expect("accounts")
                .get(scope)
                .cloned()
                .ok_or_else(|| ResolveError::Unknown {
                    scope: scope.to_owned(),
                })
        })
    }
}

impl CredentialStore for MutableAccounts {
    fn fetch<'a>(
        &'a self,
        credential_ref: &'a CredentialRef,
    ) -> BoxFuture<'a, Result<Credentials, FetchError>> {
        Box::pin(async move {
            self.keys
                .lock()
                .expect("keys")
                .get(credential_ref.as_str())
                .cloned()
                .ok_or_else(|| FetchError::Gone {
                    credential_ref: credential_ref.clone(),
                })
        })
    }
}

/// The two services of the multi-account phase at `endpoint`: `acme` is the
/// szamlazz.hu account of the single-account phase (same key — the flag day
/// changes configuration, not the account), `beta` is a second one. Reachable
/// by scope only.
async fn multi_account_services(endpoint: &str) -> (Arc<MutableAccounts>, Order, Agent) {
    let config: StaticConfig = serde_json::from_value(json!({
        "accounts": {
            "acme": {
                "id": "acme",
                "agent_key": AGENT_KEY,
                "endpoint": endpoint,
                "seller": { "bank_account": BANK_ACCOUNT },
            },
            "beta": {
                "id": "beta",
                "agent_key": KEY_B,
                "endpoint": endpoint,
            },
        },
    }))
    .expect("config");
    let resolver = StaticResolver::try_from(config).expect("resolver");
    assert!(resolver.is_scoped());
    let mutable = Arc::new(MutableAccounts::seeded_from(&resolver).await);
    let accounts = Accounts::new(
        Arc::clone(&mutable) as Arc<dyn AccountResolver>,
        Arc::clone(&mutable) as Arc<dyn CredentialStore>,
    );
    let order = Order::from_parts(accounts.clone(), worker_config());
    let agent = Agent::from_parts(accounts, worker_config());
    (mutable, order, agent)
}

/// The Restate server, the wiremock standing in for szamlazz.hu, and the
/// ingress, SQL and stub helpers the scenarios drive them through.
///
/// A scenario states what szamlazz.hu holds through the document-centric
/// helpers — one call per document, the same body on every selector the
/// document is reachable by, so the stubs cannot disagree — and through the
/// raw selector builders where it is about a specific wire sequence:
///
/// - [`Harness::absent`]: code 7 on the external ids of `kinds` under `order`.
/// - [`Harness::holds`]: `doc` on `number_query`, on `order_query` when it
///   carries an order, on `external_id_query` when it states an external id.
///   When two held documents carry one order, the one held first answers the
///   order query (wiremock answers with the first mounted match) — a scenario
///   whose order's newest document is not the one it holds keeps the raw
///   `order_query` builder.
/// - [`Harness::holds_after_misses`]: the external-id selector alone, code 7
///   for `misses` queries, then `doc` — the document appearing after a
///   hand-counted number of queries (the lookup step's, the create step's
///   leading query and re-query).
/// - [`Harness::create_lands_but_reply_lost`]: `create()` answers 500,
///   `expect(1)`, and `doc` holds its external id from the moment the create
///   request is received — the transition is the create stub being matched,
///   not a query count. Code 7 on the external id before.
/// - The raw builders (`number_query`, `order_query`, `external_id_query`,
///   `create`, `storno`), `expect(n)` and `up_to_n_times(n)`: a stub the
///   scenario asserts on (`expect`), a non-document answer (7, 500, an API
///   code) or an ordering-dependent shape stays explicit, byte for byte.
///
/// The three document helpers are checked against wiremock alone by the
/// non-ignored tests at the end of this file.
struct Harness {
    restate: Restate,
    mock: MockServer,
    http: reqwest::Client,
    script: Arc<ScriptedAccounts>,
    /// The multi-account phase's resolver and store, once the flag day ran.
    multi: Option<Arc<MutableAccounts>>,
}

impl Harness {
    /// The harness on `restate`: waits for its admin API, checks that
    /// `/version` reports exactly the features the server's flags enable, and
    /// serves and registers the single-account deployment.
    async fn start(restate: Restate) -> Self {
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

        // The server reports the features its flags enable and no other: the
        // main suite needs all three, the protocol-v7 canary needs one off; a
        // reused server (from the environment) is checked the same way.
        let version: Value = http
            .get(format!("{}/version", restate.admin))
            .send()
            .await
            .expect("version")
            .json()
            .await
            .expect("version json");
        for (feature, flag) in FEATURES {
            let expected = restate.flags.contains(&flag);
            assert_eq!(
                version["features"][feature],
                Value::Bool(expected),
                "the Restate server must run with {feature} {}: {version}",
                if expected { "enabled" } else { "disabled" }
            );
        }

        // Serve the endpoint on a free port and register it.
        let (scripted, order, agent) = services(&mock.uri());
        let harness = Self {
            restate,
            mock,
            http,
            script: scripted,
            multi: None,
        };
        harness.serve_and_register(order, agent).await;
        harness
    }

    /// Serves `order` and `agent` on a free port of this host and registers
    /// the deployment with the server: a new URI is a new revision of both
    /// services, and new invocations route to it.
    async fn serve_and_register(&self, order: Order, agent: Agent) {
        let listener = TcpListener::bind("0.0.0.0:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        listener.set_nonblocking(true).expect("nonblocking");
        let listener = tokio::net::TcpListener::from_std(listener).expect("tokio listener");
        tokio::spawn(async move {
            HttpServer::new(Endpoint::builder().bind(order).bind(agent).build())
                .serve(listener)
                .await;
        });

        let deployment = json!({
            "uri": format!("http://{}:{port}", self.restate.endpoint_host),
            "force": true,
        });
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let response = self
                .http
                .post(format!("{}/deployments", self.restate.admin))
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
    }

    /// The single → multi flag day, as the endpoint README scripts it: make
    /// both services private, poll `sys_invocation` until nothing is in
    /// flight, register the multi-account deployment (the same namespace, the
    /// same szamlazz.hu account now under scope `acme` plus a second one under
    /// `beta`), make the services public again. Callers then use scoped paths.
    async fn switch_to_multi_account(&mut self) {
        self.set_public(false).await;
        self.drain().await;
        let (mutable, order, agent) = multi_account_services(&self.mock.uri()).await;
        self.serve_and_register(order, agent).await;
        self.multi = Some(mutable);
        self.set_public(true).await;
    }

    /// The multi-account phase's resolver and store.
    fn multi(&self) -> &MutableAccounts {
        self.multi
            .as_deref()
            .expect("the flag day has run (switch_to_multi_account)")
    }

    /// `PATCH /services/{service}` with `public` for both services.
    async fn set_public(&self, public: bool) {
        for service in ["Szamlazz.Order", "Szamlazz.Agent"] {
            let response = self
                .http
                .patch(format!("{}/services/{service}", self.restate.admin))
                .json(&json!({ "public": public }))
                .send()
                .await
                .expect("modify service");
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            assert!(
                (200..300).contains(&status),
                "PATCH /services/{service} public={public} failed ({status}): {body}"
            );
        }
    }

    /// Waits until `sys_invocation` holds no invocation that is not completed.
    async fn drain(&self) {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let in_flight = self
                .sql("SELECT id, status FROM sys_invocation WHERE status <> 'completed'")
                .await;
            if in_flight.is_empty() {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "invocations still in flight: {in_flight:?}"
            );
            tokio::time::sleep(Duration::from_millis(200)).await;
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

    /// `Szamlazz.Agent.check_account`: no input, no idempotency key; unscoped
    /// or under `scope`.
    async fn check_account(&self, scope: Option<&str>) -> Reply {
        let path = match scope {
            Some(scope) => format!("/restate/scope/{scope}/call/Szamlazz.Agent/check_account"),
            None => "/restate/call/Szamlazz.Agent/check_account".to_owned(),
        };
        self.invoke(&path, None, None).await
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
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        let invocation_id = header("x-restate-id");
        let error_source = header("x-restate-error-source");
        let text = response.text().await.expect("body");
        let body = serde_json::from_str(&text).unwrap_or(Value::String(text));
        Reply {
            status,
            body,
            invocation_id,
            error_source,
        }
    }

    async fn ok(&self, key: &str, handler: &str, body: &Value, idempotency: &str) -> Value {
        let reply = self.call(key, handler, body, idempotency).await;
        assert_eq!(reply.status, 200, "{handler} on {key}: {}", reply.body);
        reply.body
    }

    /// `Szamlazz.Order.get`: no input, no idempotency key.
    async fn get(&self, key: &str) -> Value {
        let reply = self.get_reply(key).await;
        assert_eq!(reply.status, 200, "get on {key}: {}", reply.body);
        reply.body
    }

    /// `Szamlazz.Order.get` as the raw reply, for the invocation id.
    async fn get_reply(&self, key: &str) -> Reply {
        self.invoke(
            &format!("/restate/call/Szamlazz.Order/{key}/get"),
            None,
            None,
        )
        .await
    }

    /// `Szamlazz.Order.get` under `scope`.
    async fn get_scoped(&self, scope: &str, key: &str) -> Value {
        let reply = self
            .invoke(
                &format!("/restate/scope/{scope}/call/Szamlazz.Order/{key}/get"),
                None,
                None,
            )
            .await;
        assert_eq!(
            reply.status, 200,
            "get on {key} under {scope}: {}",
            reply.body
        );
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

    /// The names of the `ctx.run` commands of an invocation, in journal
    /// order: which durable steps ran.
    async fn runs(&self, invocation_id: &str) -> Vec<String> {
        self.journal(invocation_id)
            .await
            .into_iter()
            .filter(JournalEntry::is_run)
            .filter_map(|entry| entry.name)
            .collect()
    }

    /// The journal of an invocation, in index order.
    async fn journal(&self, invocation_id: &str) -> Vec<JournalEntry> {
        let rows = self
            .sql(&format!(
                "SELECT index, entry_type, name, raw FROM sys_journal WHERE id = '{invocation_id}' ORDER BY index"
            ))
            .await;
        rows.iter().map(JournalEntry::from_row).collect()
    }

    /// The `sys_invocation` row of an invocation.
    async fn invocation(&self, invocation_id: &str) -> Invocation {
        let rows = self
            .sql(&format!(
                "SELECT {} FROM sys_invocation WHERE id = '{invocation_id}'",
                Invocation::COLUMNS
            ))
            .await;
        let row = rows
            .first()
            .unwrap_or_else(|| panic!("no sys_invocation row for {invocation_id}"));
        Invocation::from_row(row)
    }

    /// How often [`watch_for`](Self::watch_for) samples `sys_invocation`; a
    /// run retry under the 1 s test policies is visible for ten samples.
    const WATCH_POLL: Duration = Duration::from_millis(100);

    /// [`watch_for`](Self::watch_for) over four seconds — enough for the one
    /// or two run retries a scenario provokes under the 1 s test policies.
    fn watch(&self, key: &str) -> tokio::task::JoinHandle<Retries> {
        self.watch_for(key, Duration::from_secs(4))
    }

    /// Watches the invocations on Virtual Object `key` for `window` (a
    /// detached task polling every [`Self::WATCH_POLL`], so it carries its own
    /// copy of the query — `sql` borrows the harness) and records what
    /// `sys_invocation` reports **while they are in flight**: `retry_count`
    /// (the invoker's count of starts), `last_failure` and
    /// `last_failure_related_command_name` are attempt state, cleared once the
    /// invocation completes — a completed row shows neither the count nor the
    /// failing command (verified against 1.7.8). Start it before the call,
    /// await it after.
    fn watch_for(&self, key: &str, window: Duration) -> tokio::task::JoinHandle<Retries> {
        let admin = self.restate.admin.clone();
        let http = self.http.clone();
        let key = key.to_owned();
        let polls = window.as_millis() / Self::WATCH_POLL.as_millis();
        tokio::spawn(async move {
            let mut retries = Retries::default();
            for _ in 0..polls {
                tokio::time::sleep(Self::WATCH_POLL).await;
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

    /// The bodies of the create requests szamlazz.hu has seen so far.
    async fn create_bodies(&self) -> Vec<String> {
        self.bodies_of("action-xmlagentxmlfile").await
    }

    /// The bodies of the storno requests szamlazz.hu has seen so far.
    async fn storno_bodies(&self) -> Vec<String> {
        self.bodies_of("action-szamla_agent_st").await
    }

    /// The bodies of the requests of `action` szamlazz.hu has seen so far.
    async fn bodies_of(&self, action: &str) -> Vec<String> {
        let marker = format!("name=\"{action}\"");
        self.mock
            .received_requests()
            .await
            .expect("requests")
            .iter()
            .map(|request| String::from_utf8_lossy(&request.body).into_owned())
            .filter(|body| body.contains(&marker))
            .collect()
    }

    /// Waits until szamlazz.hu has seen at least `count` create requests: the
    /// moment between two executions of a create step whose first lost its
    /// reply, when a between-executions change can be made.
    async fn wait_for_creates(&self, count: usize) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if self.create_bodies().await.len() >= count {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "szamlazz.hu did not see {count} create request(s)"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// Every journal entry of every invocation the server still holds, with
    /// `raw` hex-decoded, keyed by invocation id.
    async fn all_journals(&self) -> BTreeMap<String, Vec<JournalEntry>> {
        let rows = self
            .sql("SELECT id, index, entry_type, name, raw FROM sys_journal ORDER BY id, index")
            .await;
        let mut journals: BTreeMap<String, Vec<JournalEntry>> = BTreeMap::new();
        for row in &rows {
            journals
                .entry(row["id"].as_str().expect("id").to_owned())
                .or_default()
                .push(JournalEntry::from_row(row));
        }
        journals
    }

    /// Every `sys_invocation` row the server still holds.
    async fn all_invocations(&self) -> Vec<(String, Invocation)> {
        let rows = self
            .sql(&format!(
                "SELECT id, {} FROM sys_invocation ORDER BY id",
                Invocation::COLUMNS
            ))
            .await;
        rows.iter()
            .map(|row| {
                (
                    row["id"].as_str().expect("id").to_owned(),
                    Invocation::from_row(row),
                )
            })
            .collect()
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

    /// szamlazz.hu holds `doc`: see [`holds`].
    async fn holds(&self, doc: &Doc<'_>) {
        holds(&self.mock, doc).await;
    }

    /// szamlazz.hu holds `doc` under its external id after `misses` code-7
    /// answers: see [`holds_after_misses`].
    async fn holds_after_misses(&self, misses: u64, doc: &Doc<'_>) {
        holds_after_misses(&self.mock, misses, doc).await;
    }

    /// The next external-id query for `id` loses its reply once: see
    /// [`loses_reply_once`]. Mount before the steady answers.
    async fn loses_reply_once(&self, id: &str) {
        loses_reply_once(&self.mock, id).await;
    }

    /// The create lands but its reply is lost, and `doc` is the holder of its
    /// external id from that moment on: see [`create_lands_but_reply_lost`].
    async fn create_lands_but_reply_lost(&self, doc: &Doc<'_>) {
        create_lands_but_reply_lost(&self.mock, doc).await;
    }
}

// ----- scenarios ---------------------------------------------------------------

#[tokio::test]
#[ignore = "needs a Restate server: docker, RESTATE_SERVER_BIN or RESTATE_ADMIN_URL / RESTATE_INGRESS_URL"]
async fn e2e_order_protocol() {
    let Some(launcher) = launcher_or_skip(Reuse::Allowed) else {
        return;
    };
    let mut h = Harness::start(launcher.launch(&MAIN_SERVER)).await;

    // Phase 1: the single-account deployment, unscoped.
    issued_then_already_issued(&h).await;
    idempotency_key_replays_without_calling_szamlazz(&h).await;
    duplicate_order_number_reconciles(&h).await;
    storno_then_stale_create_then_reissue(&h).await;
    storno_repeats_the_originals_fulfillment_date_or_refuses(&h).await;
    reissue_on_live_is_a_conflict(&h).await;
    external_reversal_detected(&h).await;
    reversal_between_executions_is_reversed_not_reissued(&h).await;
    lost_create_reply_is_settled_by_the_immediate_requery(&h).await;
    proforma_auto_link_and_consumed(&h).await;
    proforma_by_number_is_checked_like_every_found_document(&h).await;
    corrective_is_issued_under_its_correction_id(&h).await;
    proforma_is_deleted_by_the_orders_handler(&h).await;
    status_shape(&h).await;
    secondary_lookup_collision_refuses_to_create(&h).await;
    prepayment_converts_the_proforma_like_the_invoice(&h).await;
    proforma_after_the_orders_invoice_is_order_invoiced_not_foreign(&h).await;
    a_live_final_closes_the_order_to_the_other_creates(&h).await;
    a_malformed_body_is_a_structured_invalid_input(&h).await;
    an_untrimmed_order_key_is_refused(&h).await;
    bounded_inputs_are_refused_and_disturb_no_other_invocation(&h).await;
    exhausted_create_step_is_a_structured_outcome_unknown(&h).await;
    flaky_lookup_read_is_retried_by_the_read_policy(&h).await;
    exhausted_lookup_read_is_a_structured_unavailable(&h).await;
    answered_code_on_the_create_leading_query_is_an_immediate_unavailable(&h).await;
    flaky_get_read_is_retried_by_the_read_policy(&h).await;
    run_retries_do_not_spend_invocation_attempts(&h).await;
    harness_scoped_call_and_leak_positive_control(&h).await;
    check_account_names_the_account_and_reports_the_credentials(&h).await;
    purged_invocation_queries_szamlazz_again(&h).await;
    flaky_resolver_is_retried_by_the_resolve_policy(&h).await;
    failing_credential_store_is_a_terminal_unavailable(&h).await;

    // Phase 2: the flag day, then the multi-account deployment by scope.
    flag_day_keeps_the_documents_and_refuses_unscoped_calls(&mut h).await;
    same_order_key_under_two_scopes_issues_on_both_accounts(&h).await;
    same_idempotency_key_under_two_scopes_is_two_invocations(&h).await;
    check_account_under_each_scope_names_its_account(&h).await;
    purged_order_is_stornoed_and_reissued(&h).await;
    agent_storno_acts_on_what_the_verify_finds(&h).await;
    agent_query_projects_what_it_finds(&h).await;
    every_fault_carries_a_terminal_code_and_the_szamlazz_code_beside_it(&h).await;
    agent_query_taxpayer_runs_on_the_scoped_account(&h).await;
    agent_storno_repeats_the_originals_fulfillment_date_or_refuses(&h).await;
    storno_is_issued_in_the_originals_form_not_the_accounts_default(&h).await;
    account_change_between_executions_does_not_reach_the_invocation(&h).await;
    credential_rotation_between_executions_is_picked_up(&h).await;
    no_agent_key_in_any_journal_of_the_run(&h).await;
    every_handler_journals_its_pinned_run_names(&h).await;
}

/// The deploy-time canary for protocol v7 (design §4, ADR 0006), provoked:
/// on a server without `RESTATE_EXPERIMENTAL_ENABLE_PROTOCOL_V7` the ingress
/// accepts a scoped path — it does not refuse one for the flag — and the
/// server keys the invocation by the scope (`sys_invocation.scope`), but the
/// SDK sees no scope. So a scoped `check_account` reports `scope: null`: on
/// the single-account deployment with its account and `credentials: ok` as a
/// 200 — the signal a deploy pipeline reads, since the worker has no
/// per-request way to tell "unscoped" from "scope not forwarded" — and on the
/// multi-account deployment as `unknown_account` naming the unscoped case,
/// with nothing sent: every scoped call fails closed, no account is reached
/// under the wrong scope. A server of its own, on its own ports; never a
/// reused one, whose flags are the main suite's.
#[tokio::test]
#[ignore = "needs a Restate server: docker or RESTATE_SERVER_BIN"]
async fn e2e_check_account_without_protocol_v7() {
    let Some(launcher) = launcher_or_skip(Reuse::Never) else {
        return;
    };
    let mut h = Harness::start(launcher.launch(&WITHOUT_PROTOCOL_V7)).await;

    // The single-account deployment: the scoped probe answers the account
    // and reports the scope it saw — none.
    probe_with_key(AGENT_KEY)
        .respond_with(not_found())
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h.check_account(Some("acme")).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(
        reply.body,
        json!({
            "scope": null,
            "account": { "id": "acct" },
            "namespace": "acct",
            "credentials": { "state": "ok" },
        }),
        "the canary: a scoped call reported without its scope"
    );
    assert_eq!(h.requests_seen().await, 1, "one probe query, nothing else");
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account", "probe"]
    );
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert_eq!(invocation.handler, "check_account");
    // The hazard, in one row: the server keyed the invocation by the scope
    // (`sys_invocation.scope`, the partition key) and the handler never saw
    // it — the response above is the only place the discrepancy shows.
    assert_eq!(
        invocation.scope.as_deref(),
        Some("acme"),
        "the server keyed the invocation by the scope it did not forward: {invocation:?}"
    );

    // The multi-account deployment on the same server: the scope selects no
    // account because none arrives — `unknown_account`, nothing sent.
    h.switch_to_multi_account().await;
    h.reset().await;
    let reply = h.check_account(Some("acme")).await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "unknown_account", "{fault:?}");
    assert!(fault.message.contains("unscoped"), "{fault:?}");
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account"]
    );
    assert_eq!(h.requests_seen().await, 0, "nothing reached szamlazz.hu");
    eprintln!(
        "(canary) without protocol v7: scoped check_account → scope: null on the single-account deployment, unknown_account on the multi-account one: pass"
    );
}

/// (i) create ⇒ `issued`; a second call with a **new** key ⇒
/// `already_issued` from the lookup step.
async fn issued_then_already_issued(h: &Harness) {
    h.reset().await;
    h.absent("E2E-1", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-1")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    // The lookup step and the create step's own leading query both miss;
    // the second call's lookup finds the document.
    h.holds_after_misses(
        2,
        &Doc {
            external_id: Some("acct:E2E-1:invoice"),
            ..Doc::new("SZ-1", "SZ", "E2E-1")
        },
    )
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
    h.absent("E2E-3", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-3")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    // Lookup and the create step's leading query miss; the re-query after
    // the 152 finds the document.
    h.holds_after_misses(
        2,
        &Doc {
            external_id: Some("acct:E2E-3:invoice"),
            ..Doc::new("SZ-3", "SZ", "E2E-3")
        },
    )
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
    h.absent("E2E-1", &["prepayment", "final", "proforma"])
        .await;
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

/// (iv) storno ⇒ `reversed{storno_number}` with the storno carrying the
/// original's `telj` as `teljesitesDatum` and no `keltDatum` (ADR 0007); a
/// create ⇒ `reversed`; a create with `reissue` ⇒ `issued` as the newest
/// holder of the same external id.
async fn storno_then_stale_create_then_reissue(h: &Harness) {
    h.reset().await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-1:invoice"),
        ..Doc::new("SZ-1", "SZ", "E2E-1")
    })
    .await;
    external_id_query("acct:E2E-1:storno:SZ-1")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    storno_repeating_telj()
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
    let stornos = h.storno_bodies().await;
    assert_eq!(stornos.len(), 1);
    assert!(
        !stornos[0].contains("<keltDatum>"),
        "no issue date on a storno: {}",
        stornos[0]
    );

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

/// (iv-b) the storno's `teljesitesDatum` (ADR 0007), end to end. A verified
/// original without a `telj` is 503 `unavailable` about the storno — order,
/// kind and the storno external id — with only the prologue and the verify
/// journaled and nothing sent; the fault comes **after** the answers that
/// need no send, so a `telj`-less document of another order is still
/// `conflict{not_managed}`, a `telj`-less proforma still
/// `rejected{not_stornoable}` and a `telj`-less reversed invoice still
/// `reversed` with its storno number from the hint. And a storno whose first
/// reply is lost is re-executed under the issue policy with a byte-identical
/// body — the date is a pure function of the journaled verify — under one
/// `storno-{number}` entry.
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the fault, its three predecessors and the re-executed step"
)]
async fn storno_repeats_the_originals_fulfillment_date_or_refuses(h: &Harness) {
    let without_telj = |number: &'static str, order: &'static str| Doc {
        fulfillment_date: None,
        ..Doc::new(number, "SZ", order)
    };

    // The fault: a live invoice of this order without a `telj`.
    h.reset().await;
    number_query("SZ-4A")
        .respond_with(without_telj("SZ-4A", "E2E-4").response())
        .expect(1)
        .mount(&h.mock)
        .await;
    storno_never_sent(&h.mock).await;
    let reply = h
        .call("E2E-4", "storno_invoice", &storno_of("SZ-4A"), "e2e-4-s1")
        .await;
    assert_eq!(reply.status, 503, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "unavailable", "{fault:?}");
    assert!(fault.message.contains("SZ-4A"), "{fault:?}");
    assert!(fault.message.contains("fulfillment date"), "{fault:?}");
    assert!(fault.message.contains("nothing was sent"), "{fault:?}");
    assert_eq!(fault.order.as_deref(), Some("E2E-4"), "{fault:?}");
    assert_eq!(fault.kind.as_deref(), Some("invoice"), "{fault:?}");
    assert_eq!(
        fault.external_id.as_deref(),
        Some("acct:E2E-4:storno:SZ-4A"),
        "{fault:?}"
    );
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account", "verify-storno-SZ-4A"],
        "the verify is the last step journaled"
    );
    assert_eq!(h.requests_seen().await, 1, "the verify, nothing else");

    // Before the fault: another order's document is `conflict{not_managed}`.
    h.reset().await;
    number_query("SZ-4B")
        .respond_with(without_telj("SZ-4B", "OTHER-4").response())
        .mount(&h.mock)
        .await;
    storno_never_sent(&h.mock).await;
    let conflict = h
        .ok("E2E-4", "storno_invoice", &storno_of("SZ-4B"), "e2e-4-s2")
        .await;
    assert_eq!(conflict["outcome"], "conflict", "{conflict}");
    assert_eq!(conflict["conflict_reason"], "not_managed", "{conflict}");
    assert_eq!(h.requests_seen().await, 1);

    // Before the fault: a proforma is `rejected{not_stornoable}`.
    h.reset().await;
    number_query("D-4C")
        .respond_with(
            Doc {
                fulfillment_date: None,
                ..Doc::new("D-4C", "D", "E2E-4")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    storno_never_sent(&h.mock).await;
    let rejected = h
        .ok("E2E-4", "storno_invoice", &storno_of("D-4C"), "e2e-4-s3")
        .await;
    assert_eq!(rejected["outcome"], "rejected", "{rejected}");
    assert_eq!(rejected["code"], "not_stornoable", "{rejected}");
    assert_eq!(h.requests_seen().await, 1);

    // Before the fault: an already reversed invoice is `reversed`, with the
    // storno number from the hint.
    h.reset().await;
    number_query("SZ-4D")
        .respond_with(
            Doc {
                reversed: true,
                ..without_telj("SZ-4D", "E2E-4")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    order_query("E2E-4")
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-4D"),
                ..Doc::new("SS-4D", "SS", "E2E-4")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    storno_never_sent(&h.mock).await;
    let reply = h
        .call("E2E-4", "storno_invoice", &storno_of("SZ-4D"), "e2e-4-s4")
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "reversed", "{}", reply.body);
    assert_eq!(reply.body["storno_number"], "SS-4D", "{}", reply.body);
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "verify-storno-SZ-4D",
            "hint-storno-SZ-4D"
        ]
    );
    assert_eq!(h.requests_seen().await, 2, "the verify and the hint");

    // A lost reply: the first execution's send answers 500 and its re-query
    // still misses; the second execution's send lands. Both sends carry the
    // same bytes — the same `teljesitesDatum` — under one run entry.
    h.reset().await;
    number_query("SZ-4E")
        .respond_with(Doc::new("SZ-4E", "SZ", "E2E-4").response())
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-4:storno:SZ-4E")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    storno_repeating_telj()
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .expect(1)
        .mount(&h.mock)
        .await;
    storno_repeating_telj()
        .respond_with(created("SS-4E", "-1000", "-1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call("E2E-4", "storno_invoice", &storno_of("SZ-4E"), "e2e-4-s5")
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "reversed", "{}", reply.body);
    assert_eq!(reply.body["storno_number"], "SS-4E", "{}", reply.body);
    let stornos = h.storno_bodies().await;
    assert_eq!(stornos.len(), 2, "two executions of the storno step");
    assert_eq!(
        stornos[0], stornos[1],
        "the re-executed storno is byte-identical"
    );
    assert!(stornos[0].contains(&original_telj_tag()));
    assert!(!stornos[0].contains("<keltDatum>"));
    assert!(
        stornos[0].contains("<eszamla>true</eszamla>"),
        "an e-invoice original (eszamla 2) is reversed as an e-invoice, whatever the account default (paper): {}",
        stornos[0]
    );
    let runs = h.runs(reply.invocation_id()).await;
    assert_eq!(
        runs.iter().filter(|name| *name == "storno-SZ-4E").count(),
        1,
        "one storno step entry: {runs:?}"
    );
    eprintln!(
        "(iv-b) telj-less original → unavailable about the storno with nothing sent, after not_managed / not_stornoable / reversed; lost storno reply → byte-identical re-execution: pass"
    );
}

/// (v) `reissue: true` while the document is live ⇒ `conflict{live}`.
async fn reissue_on_live_is_a_conflict(h: &Harness) {
    h.reset().await;
    h.absent("E2E-1", &["prepayment", "final", "proforma"])
        .await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-1:invoice"),
        ..Doc::new("SZ-2", "SZ", "E2E-1")
    })
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
    h.absent("E2E-6", &["prepayment", "final", "proforma", "invoice"])
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
    h.absent("E2E-6", &["prepayment", "final", "proforma"])
        .await;
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

/// (vi-b) the hole #36 closes, end to end: the lookup sees nothing, the
/// first execution of the create step sends (the document lands, the reply
/// is lost, the immediate re-query still misses), and before the run policy
/// re-executes the step the document is reversed in the szamlazz.hu UI. The
/// second execution's leading query finds it reversed and **does not send
/// again**: `outcome: reversed`, exactly one create on the wire.
async fn reversal_between_executions_is_reversed_not_reissued(h: &Harness) {
    h.reset().await;
    h.absent("E2E-6B", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-6B")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    // The lookup step, the first execution's leading query and its re-query
    // miss; the second execution's leading query finds the document
    // reversed.
    h.holds_after_misses(
        3,
        &Doc {
            external_id: Some("acct:E2E-6B:invoice"),
            reversed: true,
            ..Doc::new("SZ-6B", "SZ", "E2E-6B")
        },
    )
    .await;
    create()
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&h.mock)
        .await;

    let reply = h
        .call(
            "E2E-6B",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-6b-k1",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let response = &reply.body;
    assert_eq!(response["outcome"], "reversed", "{response}");
    assert_eq!(response["invoice_number"], "SZ-6B");
    assert_eq!(response["storno_number"], Value::Null);

    // The create step was re-executed (one run retry) and journaled once.
    let journal = h.journal(reply.invocation_id()).await;
    let runs: Vec<_> = journal
        .iter()
        .filter(|entry| entry.is_run())
        .filter_map(|entry| entry.name.as_deref())
        .collect();
    assert_eq!(
        runs.iter()
            .filter(|name| **name == "create-invoice")
            .count(),
        1,
        "one create step entry: {runs:?}"
    );
    eprintln!(
        "(vi-b) document reversed between two executions of the create step → reversed, one create on the wire: pass"
    );
}

/// (vi-c) the create lands but its reply is lost (design §5 step 4): the
/// create step's immediate re-query finds the document under the external
/// id and settles the step as `issued` **within the same execution** — no
/// run retry, exactly one create on the wire, one create step entry. The
/// harness drives the transition from the create request itself: how many
/// queries precede the send is the protocol's, not the test's, to know.
async fn lost_create_reply_is_settled_by_the_immediate_requery(h: &Harness) {
    h.reset().await;
    h.absent("E2E-6C", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-6C")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    h.create_lands_but_reply_lost(&Doc {
        external_id: Some("acct:E2E-6C:invoice"),
        ..Doc::new("SZ-6C", "SZ", "E2E-6C")
    })
    .await;

    let watch = h.watch("E2E-6C");
    let reply = h
        .call(
            "E2E-6C",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-6c-k1",
        )
        .await;
    let retries = watch.await.expect("watch");
    assert_eq!(reply.status, 200, "{}", reply.body);
    let response = &reply.body;
    assert_eq!(response["outcome"], "issued", "{response}");
    assert_eq!(response["invoice_number"], "SZ-6C");
    assert_eq!(response["external_id"], "acct:E2E-6C:invoice");
    assert_eq!(response["gross_total"], "1270");

    // Settled inside the one execution: no run failed, so no failure and no
    // failing command were recorded, and `retry_count` stayed at the first
    // execution's 1 (the server's count includes it — (xiv) observed
    // failures + 1).
    assert!(retries.max_retry_count <= 1, "{retries:?}");
    assert!(retries.failures.is_empty(), "{retries:?}");
    assert!(retries.failing_commands.is_empty(), "{retries:?}");
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert_eq!(invocation.completion_failure, None, "{invocation:?}");
    let runs = h.runs(reply.invocation_id()).await;
    assert_eq!(
        runs.iter().filter(|name| *name == "create-invoice").count(),
        1,
        "one create step entry: {runs:?}"
    );
    assert_eq!(h.create_bodies().await.len(), 1, "exactly one create");
    eprintln!(
        "(vi-c) create landed, reply lost, immediate re-query finds it → issued in one execution, one create on the wire: pass"
    );
}

/// (vii) a proforma, then an invoice with the default `proforma: auto` ⇒ the
/// create carries `dijbekeroSzamlaszam`; `get` then reports the proforma
/// `consumed` by the invoice.
async fn proforma_auto_link_and_consumed(h: &Harness) {
    h.reset().await;
    // The proforma create checks that the order is not invoiced yet.
    h.absent("E2E-7", &["invoice", "prepayment", "final", "proforma"])
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
    h.absent("E2E-7", &["prepayment", "final", "invoice"]).await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-7:proforma"),
        ..Doc::new("D-7", "D", "E2E-7")
    })
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
    h.holds(&Doc {
        external_id: Some("acct:E2E-7:invoice"),
        referenced_proforma: Some("D-7"),
        ..Doc::new("SZ-7", "SZ", "E2E-7")
    })
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

/// (vii-b) `options.proforma: {number}` validates the named proforma like
/// every other document found by number (design §3, §5 step 2): a proforma
/// carrying another order's number, or none, is `conflict{not_managed,
/// existing_number}` after the verify alone — another order's live proforma
/// cannot be linked into this order's invoice, nothing sent; a proforma of
/// this order proceeds as before and the create carries `dijbekeroSzamlaszam`
/// — whatever its `teszt` says, since the worker holds no account pin (ADR
/// 0006, account-pin amendment).
async fn proforma_by_number_is_checked_like_every_found_document(h: &Harness) {
    let body = |number: &str| {
        json!({
            "document": document(dec!(1000)),
            "options": { "proforma": { "number": number } },
        })
    };

    // Another order's proforma, and one carrying no order number at all.
    for (number, order) in [("D-31", Some("E2E-31")), ("D-32", None)] {
        h.reset().await;
        h.absent("E2E-30", &["prepayment", "final"]).await;
        number_query(number)
            .respond_with(
                Doc {
                    order,
                    ..Doc::unmanaged(number, "D")
                }
                .response(),
            )
            .expect(1)
            .mount(&h.mock)
            .await;
        create()
            .respond_with(created("SZ-30", "1000", "1270"))
            .expect(0)
            .mount(&h.mock)
            .await;
        let reply = h
            .call(
                "E2E-30",
                "create_invoice",
                &body(number),
                &format!("e2e-30-{number}"),
            )
            .await;
        assert_eq!(reply.status, 200, "{number}: {}", reply.body);
        let response = &reply.body;
        assert_eq!(response["outcome"], "conflict", "{number}: {response}");
        assert_eq!(
            response["conflict_reason"], "not_managed",
            "{number}: {response}"
        );
        assert_eq!(response["existing_number"], number, "{number}: {response}");
        assert_eq!(response["kind"], "invoice", "{number}: {response}");
        assert_eq!(
            response["external_id"], "acct:E2E-30:invoice",
            "{number}: {response}"
        );
        assert_eq!(
            h.runs(reply.invocation_id()).await,
            [
                "namespace",
                "account",
                "exclusivity-prepayment",
                "exclusivity-final",
                &format!("verify-proforma-{number}")
            ],
            "{number}: the verify is the last step journaled"
        );
        assert_eq!(
            h.requests_seen().await,
            3,
            "{number}: the two exclusivity lookups and the verify, nothing else"
        );
    }

    // A proforma of this order: the create proceeds and carries
    // `dijbekeroSzamlaszam`. Its `teszt` says a live account issued it;
    // nothing compares that with anything.
    h.reset().await;
    h.absent("E2E-30", &["prepayment", "final", "invoice"])
        .await;
    h.holds(&Doc {
        test: false,
        ..Doc::new("D-33", "D", "E2E-30")
    })
    .await;
    create()
        .and(body_string_contains(
            "<dijbekeroSzamlaszam>D-33</dijbekeroSzamlaszam>",
        ))
        .respond_with(created("SZ-30", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let invoice = h
        .ok("E2E-30", "create_invoice", &body("D-33"), "e2e-30-k4")
        .await;
    assert_eq!(invoice["outcome"], "issued", "{invoice}");
    assert_eq!(invoice["invoice_number"], "SZ-30");
    eprintln!(
        "(vii-b) proforma by number: another order's or an order-less proforma → conflict{{not_managed}} with nothing sent; this order's → issued with dijbekeroSzamlaszam, its teszt compared with nothing: pass"
    );
}

/// (vii-c) `correct_invoice`: the base is verified by number — it must carry
/// this order's number — then the corrective is issued
/// under `{namespace}:{order}:corrective:{correction_id}` with the base named
/// on the wire (`helyesbitettSzamlaszam`), through `verify-base-{number}`,
/// `lookup-corrective` and `create-corrective`; the same `correction_id` again
/// (new key) finds it: `already_issued`.
async fn corrective_is_issued_under_its_correction_id(h: &Harness) {
    h.reset().await;
    h.holds(&Doc::new("SZ-C1", "SZ", "E2E-C1")).await;
    // The corrective's id: absent for the lookup step and the create step's
    // leading query, then the issued corrective.
    h.holds_after_misses(
        2,
        &Doc {
            external_id: Some("acct:E2E-C1:corrective:fix-1"),
            referenced_invoice: Some("SZ-C1"),
            ..Doc::new("HS-C1", "HS", "E2E-C1")
        },
    )
    .await;
    create()
        .and(body_string_contains(
            "<helyesbitettSzamlaszam>SZ-C1</helyesbitettSzamlaszam>",
        ))
        .respond_with(created("HS-C1", "-1000", "-1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let body = json!({
        "invoice_number": "SZ-C1",
        "correction_id": "fix-1",
        "document": document(dec!(-1000)),
    });

    let reply = h
        .call("E2E-C1", "correct_invoice", &body, "e2e-c1-k1")
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "issued", "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "HS-C1");
    assert_eq!(reply.body["kind"], "corrective");
    assert_eq!(reply.body["external_id"], "acct:E2E-C1:corrective:fix-1");
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "verify-base-SZ-C1",
            "lookup-corrective",
            "create-corrective"
        ]
    );

    let again = h.ok("E2E-C1", "correct_invoice", &body, "e2e-c1-k2").await;
    assert_eq!(again["outcome"], "already_issued", "{again}");
    assert_eq!(again["invoice_number"], "HS-C1");
    eprintln!(
        "(vii-c) correct_invoice → issued under the correction id with the base on the wire; again → already_issued: pass"
    );
}

/// (vii-d) `delete_proforma`: the order's live proforma is found under its
/// external id (`proforma-for-delete`) and deleted (`delete-proforma-{number}`,
/// one send); once gone, a second call finds nothing and answers
/// `deleted{reason: absent}` without sending.
async fn proforma_is_deleted_by_the_orders_handler(h: &Harness) {
    h.reset().await;
    // Live for the first call's lookup, gone after the delete.
    external_id_query("acct:E2E-D1:proforma")
        .respond_with(Doc::new("D-D1", "D", "E2E-D1").response())
        .up_to_n_times(1)
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-D1:proforma")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    op("action-szamla_agent_dijbekero_torlese")
        .and(body_string_contains("<szamlaszam>D-D1</szamlaszam>"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamladbkdelvalasz xmlns="http://www.szamlazz.hu/xmlszamladbkdelvalasz"><sikeres>true</sikeres></xmlszamladbkdelvalasz>"#,
            "application/xml",
        ))
        .expect(1)
        .mount(&h.mock)
        .await;

    let reply = h
        .call("E2E-D1", "delete_proforma", &json!({}), "e2e-d1-k1")
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["deleted"], true, "{}", reply.body);
    assert!(reply.body["reason"].is_null(), "{}", reply.body);
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "proforma-for-delete",
            "delete-proforma-D-D1"
        ]
    );

    let again = h
        .ok("E2E-D1", "delete_proforma", &json!({}), "e2e-d1-k2")
        .await;
    assert_eq!(again["deleted"], true, "{again}");
    assert_eq!(again["reason"], "absent", "{again}");
    eprintln!(
        "(vii-d) delete_proforma → deleted after one send; again → absent, nothing sent: pass"
    );
}

/// (viii) the `get` live view after (iv)/(v).
async fn status_shape(h: &Harness) {
    h.reset().await;
    h.absent("E2E-1", &["proforma", "prepayment", "final"])
        .await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-1:invoice"),
        ..Doc::new("SZ-2", "SZ", "E2E-1")
    })
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
    // Another order's prepayment invoice under our prepayment id.
    h.holds(&Doc {
        external_id: Some("acct:E2E-9:prepayment"),
        ..Doc::new("ES-X", "ES", "OTHER-ORDER")
    })
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

/// (x) `create_prepayment` consumes the order's proforma like `create_invoice`
/// does (#69): `options.proforma: none` while a live proforma of ours exists
/// is `conflict{proforma_live, existing_number}` after the `proforma-link`
/// read with nothing sent — szamlazz.hu would link it by shared order number
/// anyway — under the default `auto` the create carries
/// `dijbekeroSzamlaszam` beside the `elolegszamla` flag, and under
/// `{number}` the named proforma is verified like every found document
/// (`verify-proforma-{number}` in place of `proforma-link`) and linked.
/// `create_final` and `create_proforma` still take no `options.proforma`:
/// anything but `auto` is `invalid_input` before any szamlazz.hu call.
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the two refusals, the conflict, then the issued prepayment invoice under auto and under a number"
)]
async fn prepayment_converts_the_proforma_like_the_invoice(h: &Harness) {
    h.reset().await;
    let before = h.requests_seen().await;
    for (i, handler) in ["create_final", "create_proforma"].into_iter().enumerate() {
        let reply = h
            .call(
                "E2E-10",
                handler,
                &json!({ "document": document(dec!(1000)), "options": { "proforma": "none" } }),
                &format!("e2e-10-refused-{i}"),
            )
            .await;
        assert_eq!(reply.status, 400, "{handler}: {}", reply.body);
        let fault = reply.fault();
        assert_eq!(fault.code, "invalid_input", "{handler}: {}", reply.body);
        assert!(
            fault.message.contains(&format!("not {handler}")),
            "{handler}: names the handler: {}",
            fault.message
        );
    }
    assert_eq!(h.requests_seen().await, before, "refused before any call");

    // The order's live proforma of ours, under `acct:E2E-10:proforma`.
    let proforma = Doc {
        external_id: Some("acct:E2E-10:proforma"),
        ..Doc::new("D-10", "D", "E2E-10")
    };
    h.absent("E2E-10", &["invoice", "prepayment", "final"])
        .await;
    h.holds(&proforma).await;
    create()
        .respond_with(created("ES-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let reply = h
        .call(
            "E2E-10",
            "create_prepayment",
            &json!({ "document": document(dec!(1000)), "options": { "proforma": "none" } }),
            "e2e-10-k1",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let conflict = &reply.body;
    assert_eq!(conflict["outcome"], "conflict", "{conflict}");
    assert_eq!(conflict["conflict_reason"], "proforma_live", "{conflict}");
    assert_eq!(conflict["existing_number"], "D-10");
    assert_eq!(conflict["kind"], "prepayment");
    assert_eq!(conflict["external_id"], "acct:E2E-10:prepayment");
    let runs = h.runs(reply.invocation_id()).await;
    assert_eq!(
        runs,
        [
            "namespace",
            "account",
            "exclusivity-invoice",
            "exclusivity-final",
            "proforma-link",
        ],
        "the proforma link is what refused, nothing after it: {runs:?}"
    );

    // Under `auto` the same proforma is linked explicitly.
    h.reset().await;
    h.absent("E2E-10", &["invoice", "prepayment", "final"])
        .await;
    h.holds(&proforma).await;
    create()
        .and(body_string_contains(
            "<dijbekeroSzamlaszam>D-10</dijbekeroSzamlaszam>",
        ))
        .and(body_string_contains("<elolegszamla>true</elolegszamla>"))
        .respond_with(created("ES-10", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call(
            "E2E-10",
            "create_prepayment",
            &json!({ "document": document(dec!(1000)) }),
            "e2e-10-k2",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let issued = &reply.body;
    assert_eq!(issued["outcome"], "issued", "{issued}");
    assert_eq!(issued["kind"], "prepayment");
    assert_eq!(issued["invoice_number"], "ES-10");
    assert_eq!(issued["external_id"], "acct:E2E-10:prepayment");
    assert_eq!(h.create_bodies().await.len(), 1, "exactly one create");
    let runs = h.runs(reply.invocation_id()).await;
    assert_eq!(
        runs,
        [
            "namespace",
            "account",
            "exclusivity-invoice",
            "exclusivity-final",
            "proforma-link",
            "lookup-prepayment",
            "create-prepayment",
        ],
        "{runs:?}"
    );

    // Under `{number}` the named proforma is verified by number — this
    // order's — and linked, on another order.
    h.reset().await;
    h.absent("E2E-10p", &["invoice", "prepayment", "final"])
        .await;
    h.holds(&Doc::new("D-10p", "D", "E2E-10p")).await;
    create()
        .and(body_string_contains(
            "<dijbekeroSzamlaszam>D-10p</dijbekeroSzamlaszam>",
        ))
        .and(body_string_contains("<elolegszamla>true</elolegszamla>"))
        .respond_with(created("ES-10p", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call(
            "E2E-10p",
            "create_prepayment",
            &json!({
                "document": document(dec!(1000)),
                "options": { "proforma": { "number": "D-10p" } },
            }),
            "e2e-10p-k1",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "issued", "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "ES-10p");
    let runs = h.runs(reply.invocation_id()).await;
    assert_eq!(
        runs,
        [
            "namespace",
            "account",
            "exclusivity-invoice",
            "exclusivity-final",
            "verify-proforma-D-10p",
            "lookup-prepayment",
            "create-prepayment",
        ],
        "{runs:?}"
    );
    eprintln!(
        "(x) create_prepayment: none → conflict{{proforma_live}}, auto → dijbekeroSzamlaszam on the wire, {{number}} → verified and linked; create_final/create_proforma refuse the option: pass"
    );
}

/// (x-a) `create_proforma` on an order whose invoice or prepayment invoice is
/// live: our own document (under `…:invoice` / `…:prepayment`) is
/// `conflict{order_invoiced, existing_number}` — a proforma after the invoice
/// makes no sense, but the invoice is ours, not another channel's — while a
/// live invoice under the order number that is under none of our ids stays
/// `conflict{foreign}`. Nothing is created either way.
async fn proforma_after_the_orders_invoice_is_order_invoiced_not_foreign(h: &Harness) {
    let body = json!({ "document": document(dec!(1000)) });

    // Our own live invoice, then our own live prepayment invoice.
    for (ours, other, number, tipus) in [
        ("invoice", "prepayment", "SZ-33", "SZ"),
        ("prepayment", "invoice", "ES-33", "ES"),
    ] {
        h.reset().await;
        h.absent("E2E-33", &[other, "final", "proforma"]).await;
        h.holds(&Doc {
            external_id: Some(&format!("acct:E2E-33:{ours}")),
            ..Doc::new(number, tipus, "E2E-33")
        })
        .await;
        create()
            .respond_with(created("D-33", "1000", "1270"))
            .expect(0)
            .mount(&h.mock)
            .await;
        let conflict = h
            .ok(
                "E2E-33",
                "create_proforma",
                &body,
                &format!("e2e-33-p-{ours}"),
            )
            .await;
        assert_eq!(conflict["outcome"], "conflict", "{ours}: {conflict}");
        assert_eq!(
            conflict["conflict_reason"], "order_invoiced",
            "{ours}: {conflict}"
        );
        assert_eq!(conflict["existing_number"], number, "{ours}");
        assert_eq!(conflict["kind"], "proforma");
        assert_eq!(conflict["external_id"], "acct:E2E-33:proforma");
    }

    // A live invoice under the order number that is under none of our ids.
    h.reset().await;
    h.absent("E2E-33", &["invoice", "prepayment", "final", "proforma"])
        .await;
    h.holds(&Doc::new("SZ-FOREIGN", "SZ", "E2E-33")).await;
    create()
        .respond_with(created("D-33", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let conflict = h
        .ok("E2E-33", "create_proforma", &body, "e2e-33-p-foreign")
        .await;
    assert_eq!(conflict["outcome"], "conflict", "{conflict}");
    assert_eq!(conflict["conflict_reason"], "foreign", "{conflict}");
    assert_eq!(conflict["existing_number"], "SZ-FOREIGN");
    eprintln!(
        "(x-a) create_proforma after our invoice → conflict{{order_invoiced}}; after a foreign one → conflict{{foreign}}: pass"
    );
}

/// (x-d) a live final invoice under `…:final` closes the order to every
/// other create (#62). After `ES` → `VS` → storno of the `ES`, `…:prepayment`
/// is reversed, `…:invoice` is absent and the newest document under the order
/// is the `ES`'s storno — nothing the lookup step's hint would call foreign
/// — so without a row for the final invoice a plain `SZ` (or a second `ES`
/// under `reissue`) landed beside the live `VS`. The exclusivity step finds
/// the `VS`: `create_invoice` and `create_prepayment`, with and without
/// `reissue`, are `conflict{prepaid_chain, existing_number}`, `create_proforma`
/// is `conflict{order_invoiced, existing_number}`, the refusal comes from
/// `exclusivity-final` with no lookup step after it, and nothing is sent. A
/// **reversed** `VS` refuses nothing: `create_invoice`, and `create_prepayment`
/// with `reissue`, proceed to `issued`; `create_final` reports the reversed
/// final and, with `reissue`, issues the next one under the same id — its own
/// check, `prepayment-for-final`, is unchanged.
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the five refusals, then the three creates a reversed final does not refuse"
)]
async fn a_live_final_closes_the_order_to_the_other_creates(h: &Harness) {
    // ES-35 issued, VS-35 settled it, then ES-35 was reversed.
    h.reset().await;
    mount_prepaid_chain(h, "35", true, false).await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    for (i, (handler, kind, reissue, reason)) in [
        ("create_invoice", "invoice", false, "prepaid_chain"),
        ("create_invoice", "invoice", true, "prepaid_chain"),
        ("create_prepayment", "prepayment", false, "prepaid_chain"),
        ("create_prepayment", "prepayment", true, "prepaid_chain"),
        ("create_proforma", "proforma", false, "order_invoiced"),
    ]
    .into_iter()
    .enumerate()
    {
        let reply = h
            .call(
                "E2E-35",
                handler,
                &create_body(dec!(1000), reissue),
                &format!("e2e-35-k{i}"),
            )
            .await;
        assert_eq!(reply.status, 200, "{handler}: {}", reply.body);
        let conflict = &reply.body;
        assert_eq!(
            conflict["outcome"], "conflict",
            "{handler} reissue={reissue}: {conflict}"
        );
        assert_eq!(
            conflict["conflict_reason"], reason,
            "{handler} reissue={reissue}: {conflict}"
        );
        assert_eq!(
            conflict["existing_number"], "VS-35",
            "{handler} reissue={reissue}: {conflict}"
        );
        assert_eq!(conflict["kind"], kind, "{handler}: {conflict}");
        assert_eq!(
            conflict["external_id"],
            format!("acct:E2E-35:{kind}"),
            "{handler}: {conflict}"
        );
        let runs = h.runs(reply.invocation_id()).await;
        assert_eq!(
            runs.last().map(String::as_str),
            Some("exclusivity-final"),
            "{handler}: the final invoice's row is what refused: {runs:?}"
        );
        assert!(
            !runs.iter().any(|name| name.starts_with("lookup-")),
            "{handler}: no lookup step after the refusal: {runs:?}"
        );
    }
    assert!(
        h.create_bodies().await.is_empty(),
        "nothing was sent beside the live VS-35"
    );

    // A reversed final refuses nothing: with both the `ES` and the `VS`
    // reversed, the plain invoice proceeds and a second prepayment invoice
    // proceeds under `reissue` — the hint's newest document is the final's
    // storno, not foreign — and each create lands under its own external id.
    for (handler, kind, reissue, number) in [
        ("create_invoice", "invoice", false, "SZ-36"),
        ("create_prepayment", "prepayment", true, "ES-36B"),
    ] {
        h.reset().await;
        mount_prepaid_chain(h, "36", true, true).await;
        create()
            .and(body_string_contains(format!(
                "<szamlaKulsoAzon>acct:E2E-36:{kind}</szamlaKulsoAzon>"
            )))
            .respond_with(created(number, "1000", "1270"))
            .expect(1)
            .mount(&h.mock)
            .await;
        let reply = h
            .call(
                "E2E-36",
                handler,
                &create_body(dec!(1000), reissue),
                &format!("e2e-36-{handler}"),
            )
            .await;
        assert_eq!(reply.status, 200, "{handler}: {}", reply.body);
        assert_eq!(reply.body["outcome"], "issued", "{handler}: {}", reply.body);
        assert_eq!(reply.body["invoice_number"], number, "{handler}");
        assert_eq!(reply.body["external_id"], format!("acct:E2E-36:{kind}"));
        assert_eq!(
            h.create_bodies().await.len(),
            1,
            "{handler}: exactly one create"
        );
        let runs = h.runs(reply.invocation_id()).await;
        let mut expected = vec!["namespace", "account"];
        expected.extend(if kind == "invoice" {
            vec!["exclusivity-prepayment", "exclusivity-final"]
        } else {
            vec!["exclusivity-invoice", "exclusivity-final"]
        });
        // Both kinds convert a proforma (#69), so both run the link.
        expected.push("proforma-link");
        let (lookup, create_step) = (format!("lookup-{kind}"), format!("create-{kind}"));
        expected.extend([lookup.as_str(), create_step.as_str()]);
        assert_eq!(runs, expected, "{handler}: {runs:?}");
    }

    // A new final after a reversed one stays possible: `create_final` checks
    // its live prepayment (`prepayment-for-final`, unchanged), reports the
    // reversed final without `reissue`, and issues the next one with it,
    // settling the same prepayment.
    h.reset().await;
    mount_prepaid_chain(h, "37", false, true).await;
    create()
        .and(body_string_contains("<vegszamla>true</vegszamla>"))
        .and(body_string_contains(
            "<elolegSzamlaszam>ES-37</elolegSzamlaszam>",
        ))
        .respond_with(created("VS-38", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reversed = h
        .ok(
            "E2E-37",
            "create_final",
            &create_body(dec!(1000), false),
            "e2e-37-k1",
        )
        .await;
    assert_eq!(reversed["outcome"], "reversed", "{reversed}");
    assert_eq!(reversed["invoice_number"], "VS-37");
    assert_eq!(reversed["storno_number"], "SS-37");
    assert!(
        h.create_bodies().await.is_empty(),
        "reversed is not reissue"
    );
    let reply = h
        .call(
            "E2E-37",
            "create_final",
            &create_body(dec!(1000), true),
            "e2e-37-k2",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "issued", "{}", reply.body);
    assert_eq!(reply.body["kind"], "final");
    assert_eq!(reply.body["invoice_number"], "VS-38");
    assert_eq!(reply.body["external_id"], "acct:E2E-37:final");
    let runs = h.runs(reply.invocation_id()).await;
    assert_eq!(
        runs,
        [
            "namespace",
            "account",
            "prepayment-for-final",
            "lookup-final",
            "create-final",
        ],
        "no exclusivity row for the final invoice itself: {runs:?}"
    );
    eprintln!(
        "(x-d) live VS → create_invoice/create_prepayment conflict{{prepaid_chain}}, create_proforma conflict{{order_invoiced}}; reversed VS refuses nothing: pass"
    );
}

/// szamlazz.hu after `ES-{n}` → `VS-{n}` on order `E2E-{n}`, with the `ES`
/// and/or the `VS` reversed since: nothing under `…:invoice` or `…:proforma`,
/// the `ES` under `…:prepayment`, the `VS` (`hivszamlaszam` = the `ES`) under
/// `…:final`, and the newest document under the order the storno `SS-{n}` of
/// the last one reversed — the `VS` itself while both are live.
async fn mount_prepaid_chain(h: &Harness, n: &str, es_reversed: bool, vs_reversed: bool) {
    let order = format!("E2E-{n}");
    let (es, vs, ss) = (format!("ES-{n}"), format!("VS-{n}"), format!("SS-{n}"));
    h.absent(&order, &["invoice", "proforma"]).await;
    external_id_query(&format!("acct:{order}:prepayment"))
        .respond_with(
            Doc {
                reversed: es_reversed,
                ..Doc::new(&es, "ES", &order)
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    let final_invoice = Doc {
        reversed: vs_reversed,
        referenced_invoice: Some(&es),
        ..Doc::new(&vs, "VS", &order)
    };
    external_id_query(&format!("acct:{order}:final"))
        .respond_with(final_invoice.response())
        .mount(&h.mock)
        .await;
    let newest = match (es_reversed, vs_reversed) {
        (_, true) => Doc {
            referenced_invoice: Some(&vs),
            ..Doc::new(&ss, "SS", &order)
        },
        (true, false) => Doc {
            referenced_invoice: Some(&es),
            ..Doc::new(&ss, "SS", &order)
        },
        (false, false) => final_invoice,
    };
    order_query(&order)
        .respond_with(newest.response())
        .mount(&h.mock)
        .await;
}

/// (x-b) a malformed body — one carrying a field the contract does not
/// know, a misspelt `reissue` — is refused as the structured `invalid_input`
/// fault (400, `{code, message}` naming the field), not accepted as
/// `reissue: false` and not the SDK's plain-text `Cannot decode input
/// payload`. Refused before the prologue: nothing journaled, nothing sent,
/// the create mock `expect(0)`. A nested misspelling and a wrong type are
/// the same fault.
async fn a_malformed_body_is_a_structured_invalid_input(h: &Harness) {
    h.reset().await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let before = h.requests_seen().await;
    let reply = h
        .call(
            "E2E-10b",
            "create_invoice",
            &json!({ "document": document(dec!(1000)), "options": { "resissue": true } }),
            "e2e-10b-k1",
        )
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "invalid_input", "{fault:?}");
    assert!(
        fault.message.contains("unknown field `resissue`"),
        "names the field: {fault:?}"
    );
    assert_eq!(fault.order, None, "{fault:?}");
    assert_eq!(
        h.requests_seen().await,
        before,
        "nothing reached szamlazz.hu"
    );
    assert!(
        h.runs(reply.invocation_id()).await.is_empty(),
        "refused before the prologue: nothing journaled"
    );
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.handler, "create_invoice");
    assert!(
        invocation
            .completion_failure
            .as_deref()
            .is_some_and(|failure| failure.contains("invalid_input")),
        "{invocation:?}"
    );

    // A nested one — `buyer.tax_numer` — and a wrong type are the same
    // fault; the caller never gets an invoice without the tax number.
    let mut body = json!({ "document": document(dec!(1000)) });
    body["document"]["buyer"]["tax_numer"] = json!("12345678-2-42");
    let reply = h
        .call("E2E-10b", "create_invoice", &body, "e2e-10b-k2")
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "invalid_input", "{fault:?}");
    assert!(fault.message.contains("`tax_numer`"), "{fault:?}");

    let reply = h
        .call(
            "E2E-10b",
            "delete_proforma",
            &json!({ "force": "yes" }),
            "e2e-10b-k3",
        )
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    assert_eq!(reply.fault().code, "invalid_input", "{}", reply.body);
    assert_eq!(
        h.requests_seen().await,
        before,
        "nothing reached szamlazz.hu"
    );
    eprintln!("(x-b) malformed body → structured invalid_input, nothing issued: pass");
}

/// (x-c) a Virtual Object key that is not trimmed — `%20E2E-10c`, which the
/// ingress decodes to ` E2E-10c` — is refused as `invalid_input` naming the
/// rule. Restate's per-key lock is on the *raw* key, so ` E2E-10c` and
/// `E2E-10c` would be two instances with two locks mapping to one szamlazz.hu
/// order and identical external ids, and two concurrent creates under them
/// would both pass their lookup and both send; the caller trims (design §3).
/// Refused after the body decode and before the prologue: nothing journaled,
/// nothing sent, the create mock `expect(0)`. A trailing space is the same
/// fault; the trimmed key is accepted as before.
async fn an_untrimmed_order_key_is_refused(h: &Harness) {
    h.reset().await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let before = h.requests_seen().await;
    for (i, key) in ["%20E2E-10c", "E2E-10c%20", "%20E2E-10c%20"]
        .into_iter()
        .enumerate()
    {
        let reply = h
            .call(
                key,
                "create_invoice",
                &create_body(dec!(1000), false),
                &format!("e2e-10c-k{i}"),
            )
            .await;
        assert_eq!(reply.status, 400, "{key}: {}", reply.body);
        let fault = reply.fault();
        assert_eq!(fault.code, "invalid_input", "{key}: {fault:?}");
        assert!(
            fault
                .message
                .contains("must not have leading or trailing whitespace"),
            "{key}: names the rule: {fault:?}"
        );
        assert_eq!(fault.order, None, "{key}: {fault:?}");
        assert!(
            h.runs(reply.invocation_id()).await.is_empty(),
            "{key}: refused before the prologue: nothing journaled"
        );
        let invocation = h.invocation(reply.invocation_id()).await;
        assert_eq!(invocation.handler, "create_invoice");
        assert!(
            invocation
                .completion_failure
                .as_deref()
                .is_some_and(|failure| failure.contains("invalid_input")),
            "{key}: {invocation:?}"
        );
    }
    assert_eq!(
        h.requests_seen().await,
        before,
        "nothing reached szamlazz.hu"
    );
    eprintln!(
        "(x-c) untrimmed order key → invalid_input naming the rule, nothing journaled or issued: pass"
    );
}

/// (x-e) bounded inputs (#64). An order key outside the alphabet — an
/// internal space (`E2E%2010d`, which the ingress decodes to `E2E 10d`), a
/// `:`, 41 bytes — and an `invoice_number` over 40 bytes are refused as
/// `invalid_input` naming the rule before the prologue: nothing journaled,
/// nothing sent. A body whose line-item arithmetic overflows a decimal is the
/// same fault from the handler's own validation — after the prologue's two
/// entries (`namespace`, `account`), which the check needs for the account's
/// currency defaults, and before any read — never a panic: the request is
/// sent beside a healthy create on another order against the same endpoint,
/// and that create is `issued` without a retry; exactly one create reaches
/// szamlazz.hu, the healthy order's.
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the three bounds, then the overflow beside a healthy create"
)]
async fn bounded_inputs_are_refused_and_disturb_no_other_invocation(h: &Harness) {
    h.reset().await;
    let before = h.requests_seen().await;

    // The order-key alphabet, at the handler's key check.
    let too_long = "x".repeat(41);
    for (i, (key, rule)) in [
        ("E2E%2010d", "must not contain whitespace"),
        ("E2E:10d", "must not contain ':'"),
        (too_long.as_str(), "at most 40 are allowed"),
    ]
    .into_iter()
    .enumerate()
    {
        let reply = h
            .call(
                key,
                "create_invoice",
                &create_body(dec!(1000), false),
                &format!("e2e-10d-k{i}"),
            )
            .await;
        assert_eq!(reply.status, 400, "{key}: {}", reply.body);
        let fault = reply.fault();
        assert_eq!(fault.code, "invalid_input", "{key}: {fault:?}");
        assert!(
            fault.message.contains(rule),
            "{key}: names the rule: {fault:?}"
        );
        assert_eq!(fault.order, None, "{key}: {fault:?}");
        assert!(
            h.runs(reply.invocation_id()).await.is_empty(),
            "{key}: refused before the prologue: nothing journaled"
        );
    }

    // The invoice-number bound, at the body decode (a malformed body).
    let reply = h
        .call(
            "E2E-10d",
            "storno_invoice",
            &json!({ "invoice_number": too_long }),
            "e2e-10d-k3",
        )
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "invalid_input", "{fault:?}");
    assert!(
        fault
            .message
            .contains("invoice number is 41 bytes long, at most 40 are allowed"),
        "names the rule: {fault:?}"
    );
    assert!(
        h.runs(reply.invocation_id()).await.is_empty(),
        "refused before the prologue: nothing journaled"
    );
    assert_eq!(
        h.requests_seen().await,
        before,
        "nothing reached szamlazz.hu"
    );

    // The overflow, beside a healthy create on another order.
    h.absent("E2E-10e", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-10e")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    h.holds_after_misses(
        2,
        &Doc {
            external_id: Some("acct:E2E-10e:invoice"),
            ..Doc::new("SZ-10e", "SZ", "E2E-10e")
        },
    )
    .await;
    create()
        .respond_with(created("SZ-10e", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let mut overflowing = document(Decimal::MAX);
    overflowing.items[0].quantity = dec!(10);
    let overflowing = json!({ "document": overflowing, "options": {} });

    let healthy_body = create_body(dec!(1000), false);
    let started = Instant::now();
    let (healthy, refused) = tokio::join!(
        h.call("E2E-10e", "create_invoice", &healthy_body, "e2e-10e-k1"),
        h.call("E2E-10d", "create_invoice", &overflowing, "e2e-10d-k4"),
    );
    let elapsed = started.elapsed();

    assert_eq!(refused.status, 400, "{}", refused.body);
    let fault = refused.fault();
    assert_eq!(fault.code, "invalid_input", "{fault:?}");
    assert!(
        fault.message.contains("items[0]") && fault.message.contains("overflows a decimal"),
        "names the item and the rule: {fault:?}"
    );
    assert_eq!(
        h.runs(refused.invocation_id()).await,
        ["namespace", "account"],
        "the prologue ran (the check needs the account), no read did"
    );

    assert_eq!(healthy.status, 200, "{}", healthy.body);
    assert_eq!(healthy.body["outcome"], "issued", "{}", healthy.body);
    assert_eq!(healthy.body["invoice_number"], "SZ-10e");
    let creates = h.create_bodies().await;
    assert_eq!(creates.len(), 1, "exactly one create on the wire");
    assert!(
        creates[0].contains("<rendelesSzam>E2E-10e</rendelesSzam>"),
        "the healthy order's: {}",
        creates[0]
    );
    // A torn-down connection would have the healthy create retried no sooner
    // than the handler's two-minute `initial_interval`; it answered at once.
    assert!(
        elapsed < Duration::from_secs(60),
        "the healthy create was not retried: {elapsed:?}"
    );
    eprintln!(
        "(x-e) bounded inputs → invalid_input naming the rule; an overflowing body beside a healthy create issues nothing and disturbs nothing: pass"
    );
}

/// (xi) every execution of the create step loses its reply and the re-query
/// finds nothing ⇒ the run retry policy re-executes the step (one second
/// later under the test policy — not the handler's two-minute
/// `initial_interval`), and its exhaustion is a structured `outcome_unknown`
/// fault naming the order, kind and external id. That run retries spend
/// none of the handler's `invocation_retry_policy` attempts is (xi-e)'s
/// proof; here `retry_count` is only checked to have moved.
async fn exhausted_create_step_is_a_structured_outcome_unknown(h: &Harness) {
    h.reset().await;
    h.absent("E2E-11", &["prepayment", "final", "proforma", "invoice"])
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
    // `retry_count` — the invoker's count of starts — counts it, with the
    // create step named as the failing command — and the completed invocation
    // carries the structured fault.
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

/// (xi-b) a read that szamlazz.hu fails to answer once is retried by the
/// **read policy**, not failed terminally: the lookup step's external-id
/// query answers 500 to its first execution and code 7 afterwards; the create
/// completes `issued` in one invocation, the `lookup-invoice` run is what
/// retried (`last_failure_related_command_name` while in flight), and the
/// create mock sees exactly one request.
async fn flaky_lookup_read_is_retried_by_the_read_policy(h: &Harness) {
    h.reset().await;
    h.absent("E2E-27", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-27")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    // The first execution of the lookup step loses its reply; the second,
    // and the create step's own leading query, miss cleanly.
    h.loses_reply_once("acct:E2E-27:invoice").await;
    external_id_query("acct:E2E-27:invoice")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-27", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;

    let started = Instant::now();
    let watch = h.watch("E2E-27");
    let reply = h
        .call(
            "E2E-27",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-27-k1",
        )
        .await;
    let elapsed = started.elapsed();
    let retries = watch.await.expect("watch");
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "issued", "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "SZ-27");
    assert!(
        elapsed < Duration::from_secs(60),
        "the read policy's delay was honoured, not the handler's: {elapsed:?}"
    );

    // The run, not the handler, is what retried — and it was the lookup.
    assert!(retries.max_retry_count >= 1, "{retries:?}");
    assert_eq!(
        retries.failing_commands,
        ["lookup-invoice"],
        "the lookup step is the failing command: {retries:?}"
    );
    assert!(
        retries
            .failures
            .iter()
            .all(|failure| failure.contains("transport failure")),
        "the last failure is the Unanswered message: {retries:?}"
    );
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.handler, "create_invoice");
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert_eq!(invocation.completion_failure, None, "{invocation:?}");
    let runs = h.runs(reply.invocation_id()).await;
    assert_eq!(
        runs,
        [
            "namespace",
            "account",
            "exclusivity-prepayment",
            "exclusivity-final",
            "proforma-link",
            "lookup-invoice",
            "create-invoice",
        ],
        "one journal entry per step; the retried read is one entry: {runs:?}"
    );
    assert_eq!(h.create_bodies().await.len(), 1, "exactly one create");
    eprintln!("(xi-b) flaky lookup read → retried by the read policy, issued once: pass");
}

/// (xi-c) a read that szamlazz.hu never answers is, after the read policy
/// is exhausted, a structured `unavailable` (503) naming the order, kind and
/// external id — within the read policy's delays — and the create mock sees
/// zero requests.
async fn exhausted_lookup_read_is_a_structured_unavailable(h: &Harness) {
    h.reset().await;
    h.absent("E2E-28", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-28")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-28:invoice")
        .respond_with(ResponseTemplate::new(500))
        .expect(3)
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-28", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;

    let started = Instant::now();
    let watch = h.watch("E2E-28");
    let reply = h
        .call(
            "E2E-28",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-28-k1",
        )
        .await;
    let elapsed = started.elapsed();
    let retries = watch.await.expect("watch");
    assert_eq!(reply.status, 503, "{}", reply.body);
    assert!(
        elapsed < Duration::from_secs(60),
        "three executions one second apart, not the handler's policy: {elapsed:?}"
    );

    let fault = reply.fault();
    assert_eq!(fault.code, "unavailable", "{fault:?}");
    assert_eq!(fault.order.as_deref(), Some("E2E-28"));
    assert_eq!(fault.kind.as_deref(), Some("invoice"));
    assert_eq!(fault.external_id.as_deref(), Some("acct:E2E-28:invoice"));
    assert!(fault.message.contains("lookup-invoice"), "{fault:?}");
    assert!(fault.message.contains("transport failure"), "{fault:?}");
    assert!(
        fault.message.contains("retry with a new Idempotency-Key"),
        "{fault:?}"
    );

    assert!(retries.max_retry_count >= 1, "{retries:?}");
    assert_eq!(
        retries.failing_commands,
        ["lookup-invoice"],
        "the lookup step is the failing command: {retries:?}"
    );
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.handler, "create_invoice");
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert!(
        invocation
            .completion_failure
            .as_deref()
            .is_some_and(|failure| failure.contains("unavailable")),
        "{invocation:?}"
    );
    let runs = h.runs(reply.invocation_id()).await;
    assert!(
        runs.contains(&"lookup-invoice".to_owned()),
        "the lookup is journaled by name: {runs:?}"
    );
    assert!(
        !runs.contains(&"create-invoice".to_owned()),
        "the create step never ran: {runs:?}"
    );
    assert_eq!(h.create_bodies().await.len(), 0, "nothing was created");
    eprintln!("(xi-c) exhausted lookup read → structured unavailable, nothing created: pass");
}

/// (xi-c') an *answer* to the create step's leading query that is neither 7
/// nor a credential code — here 57 — is settled data, not `Unconfirmed`
/// (#63): the handler answers the structured `unavailable` (503) at once with
/// the code beside it, the `create-invoice` run is journaled as data with no
/// failure and no failing command recorded (the issue policy is not spent on
/// a read), and the create mock sees zero requests. The lookup step's own
/// query misses cleanly so that the create step is reached.
async fn answered_code_on_the_create_leading_query_is_an_immediate_unavailable(h: &Harness) {
    h.reset().await;
    h.absent("E2E-29", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-29")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    // The lookup step's query: code 7. The create step's leading query, the
    // next query of the same id: code 57. Two queries in all.
    external_id_query("acct:E2E-29:invoice")
        .respond_with(not_found())
        .up_to_n_times(1)
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-29:invoice")
        .respond_with(api_error("57", "Hibás XML."))
        .expect(1)
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-29", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;

    let watch = h.watch("E2E-29");
    let reply = h
        .call(
            "E2E-29",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-29-k1",
        )
        .await;
    let retries = watch.await.expect("watch");
    assert_eq!(reply.status, 503, "{}", reply.body);

    let fault = reply.fault();
    assert_eq!(fault.code, "unavailable", "{fault:?}");
    assert_eq!(fault.szamlazz_code.as_deref(), Some("57"), "{fault:?}");
    assert_eq!(fault.order.as_deref(), Some("E2E-29"));
    assert_eq!(fault.kind.as_deref(), Some("invoice"));
    assert_eq!(fault.external_id.as_deref(), Some("acct:E2E-29:invoice"));
    assert!(fault.message.contains("code 57"), "{fault:?}");
    assert!(
        fault.message.contains("retry with a new Idempotency-Key"),
        "{fault:?}"
    );

    // Settled inside the one execution: no run failed, so no failure and no
    // failing command were recorded, and `retry_count` stayed at the first
    // execution's 1 (the server's count includes it, as (vi-c) observed) —
    // the answer was data, not `Unconfirmed`.
    assert!(retries.max_retry_count <= 1, "{retries:?}");
    assert!(retries.failures.is_empty(), "{retries:?}");
    assert!(retries.failing_commands.is_empty(), "{retries:?}");
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.handler, "create_invoice");
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert!(
        invocation
            .completion_failure
            .as_deref()
            .is_some_and(|failure| failure.contains("unavailable")),
        "{invocation:?}"
    );
    let runs = h.runs(reply.invocation_id()).await;
    assert!(
        runs.contains(&"lookup-invoice".to_owned()) && runs.contains(&"create-invoice".to_owned()),
        "the create step ran and journaled the answer: {runs:?}"
    );
    assert_eq!(h.create_bodies().await.len(), 0, "nothing was created");
    eprintln!(
        "(xi-c') answered code on the create step's leading query → immediate unavailable{{szamlazz_code}}, nothing created: pass"
    );
}

/// (xi-d) `get` under the same fault injection: one of its four reads loses
/// its reply once, the read policy re-executes it, and the status completes
/// with what szamlazz.hu holds.
async fn flaky_get_read_is_retried_by_the_read_policy(h: &Harness) {
    h.reset().await;
    h.absent("E2E-29", &["prepayment", "final"]).await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-29:invoice"),
        ..Doc::new("SZ-29", "SZ", "E2E-29")
    })
    .await;
    h.loses_reply_once("acct:E2E-29:proforma").await;
    external_id_query("acct:E2E-29:proforma")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;

    let watch = h.watch("E2E-29");
    let reply = h.get_reply("E2E-29").await;
    let retries = watch.await.expect("watch");
    assert_eq!(reply.status, 200, "{}", reply.body);
    let status = &reply.body;
    assert_eq!(status["invoice"]["number"], "SZ-29", "{status}");
    assert_eq!(status["invoice"]["state"], "live");
    assert_eq!(status["proforma"], Value::Null);
    assert_eq!(status["prepayment"], Value::Null);
    assert_eq!(status["final"], Value::Null);

    assert!(retries.max_retry_count >= 1, "{retries:?}");
    assert_eq!(
        retries.failing_commands,
        ["get-proforma"],
        "the proforma read is the failing command: {retries:?}"
    );
    let runs = h.runs(reply.invocation_id()).await;
    assert_eq!(
        runs,
        [
            "namespace",
            "account",
            "get-proforma",
            "get-invoice",
            "get-prepayment",
            "get-final",
        ],
        "{runs:?}"
    );
    eprintln!("(xi-d) flaky get read → retried by the read policy, status complete: pass");
}

/// (xi-e) **run retries do not spend invocation attempts** — the fact every
/// retry budget of the worker rests on (ADR 0004, #87): a re-execution the
/// SDK asks for with a delay (`next_retry_delay`, a run retry policy) is
/// re-dispatched by the server without advancing the handler's
/// `invocation_retry_policy` iterator, whose attempts are spent only on
/// worker-side failures. Proved on `get`: all four of its reads lose their
/// reply once, so the read policy re-executes the invocation four times —
/// more re-executions than the handler's `max_attempts` (read from discovery)
/// would allow if they counted — and the status still completes with what
/// szamlazz.hu holds, instead of the invocation being killed.
/// `sys_invocation.retry_count` is the invoker's count of starts
/// (`start_count`; verified against 1.7.8), so it is seen past the handler's
/// `max_attempts` while the invocation is in flight.
async fn run_retries_do_not_spend_invocation_attempts(h: &Harness) {
    h.reset().await;
    // Each of the four reads loses its reply once — mounted before the steady
    // answers, which take over from the second query on.
    for kind in ["proforma", "invoice", "prepayment", "final"] {
        h.loses_reply_once(&format!("acct:E2E-30:{kind}")).await;
    }
    h.absent("E2E-30", &["proforma", "prepayment", "final"])
        .await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-30:invoice"),
        ..Doc::new("SZ-30", "SZ", "E2E-30")
    })
    .await;

    // Four re-executions must be more than the handler would tolerate as
    // attempts, or completing proves nothing.
    let get_max_attempts = discovered_max_attempts::<Order>("get");
    assert!(
        get_max_attempts < 4,
        "`get` allows {get_max_attempts} attempts; this scenario needs to re-execute more often than that"
    );

    let started = Instant::now();
    let watch = h.watch_for("E2E-30", Duration::from_secs(12));
    let reply = h.get_reply("E2E-30").await;
    let elapsed = started.elapsed();
    let retries = watch.await.expect("watch");
    assert_eq!(reply.status, 200, "{}", reply.body);
    let status = &reply.body;
    assert_eq!(status["invoice"]["number"], "SZ-30", "{status}");
    assert_eq!(status["invoice"]["state"], "live");
    assert_eq!(status["proforma"], Value::Null);
    assert!(
        elapsed < Duration::from_secs(60),
        "four run retries one second apart, not the handler's policy: {elapsed:?}"
    );

    // Four re-executions on top of the first start: the count is seen past
    // the handler's budget (each re-execution is visible for the 1 s back-off
    // before it), and the invocation completes rather than being killed —
    // run retries spent none of its attempts.
    assert!(
        retries.max_retry_count > get_max_attempts,
        "retry_count must exceed get's max_attempts ({get_max_attempts}): {retries:?}"
    );
    assert_eq!(
        retries.failing_commands,
        ["get-proforma", "get-invoice", "get-prepayment", "get-final"],
        "each read failed once, in order: {retries:?}"
    );
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert_eq!(invocation.completion_failure, None, "{invocation:?}");
    let runs = h.runs(reply.invocation_id()).await;
    assert_eq!(
        runs,
        [
            "namespace",
            "account",
            "get-proforma",
            "get-invoice",
            "get-prepayment",
            "get-final",
        ],
        "one journal entry per step, the retries invisible in the journal: {runs:?}"
    );
    eprintln!(
        "(xi-e) run retries do not spend invocation attempts: {} starts against max_attempts = {get_max_attempts}, completed: pass",
        retries.max_retry_count
    );
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
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account"]
    );
    assert_eq!(h.requests_seen().await, 0, "nothing reached szamlazz.hu");

    // Positive control: the sentinel travels through szamlazz.hu's rejection
    // message into the create run's journaled result.
    h.reset().await;
    h.absent("E2E-12", &["prepayment", "final", "proforma", "invoice"])
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

/// (xii-b) `check_account` on the single-account deployment: unscoped, the
/// probe answers the configured account with `scope: null` and
/// `credentials: ok` after exactly one szamlazz.hu request — the query of the
/// sentinel id, carrying the account's key — with `probe` as its one step
/// after the prologue's; a wrong key is `credentials: rejected` as data, not
/// a fault. `scope` is what the SDK saw: under a scoped call it is the
/// deploy-time signal that the server forwards the scope (protocol v7).
async fn check_account_names_the_account_and_reports_the_credentials(h: &Harness) {
    h.reset().await;
    probe_with_key(AGENT_KEY)
        .respond_with(not_found())
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h.check_account(None).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(
        reply.body,
        json!({
            "scope": null,
            "account": { "id": "acct" },
            "namespace": "acct",
            "credentials": { "state": "ok" },
        })
    );
    assert_eq!(h.requests_seen().await, 1, "one query, nothing else");
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account", "probe"]
    );
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert_eq!(invocation.handler, "check_account");
    assert_eq!(invocation.scope, None);

    // A wrong key: szamlazz.hu's code 3 on the probe is reported, not raised.
    h.reset().await;
    probe_with_key(AGENT_KEY)
        .respond_with(api_error("3", "Sikertelen bejelentkezés."))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h.check_account(None).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["account"]["id"], "acct");
    assert_eq!(
        reply.body["credentials"],
        json!({ "state": "rejected", "code": "3", "message": "Sikertelen bejelentkezés." })
    );
    assert_eq!(h.requests_seen().await, 1, "one query, nothing else");
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.status, "completed", "{invocation:?}");

    // A scoped probe on the single-account deployment: no account to probe,
    // nothing sent — the scope is refused by the `account` step (xii), and
    // the probe reports it the same way.
    h.reset().await;
    let reply = h.check_account(Some("acme-events")).await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    assert_eq!(reply.fault().code, "unknown_account", "{}", reply.body);
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account"]
    );
    assert_eq!(h.requests_seen().await, 0, "nothing reached szamlazz.hu");
    eprintln!(
        "(xii-b) check_account unscoped → the account, credentials ok | rejected as data; scoped → unknown_account: pass"
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
    h.absent("E2E-14", &["prepayment", "final", "proforma", "invoice"])
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
    h.absent("E2E-15", &["prepayment", "final", "proforma", "invoice"])
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
    // No response names the account, nor the store's reference (#65).
    assert!(!fault.message.contains("acct"), "{fault:?}");
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

    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account"]
    );
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
    h.absent("E2E-15", &["prepayment", "final", "proforma", "invoice"])
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

// ----- phase 2: the flag day and the multi-account deployment ------------------

/// (xvi) the single → multi flag day. While the services are private the
/// ingress refuses a call without creating an invocation; after the drain
/// and the switch — same namespace, the same szamlazz.hu account now under
/// scope `acme` — the first scoped create for an order the single-account
/// phase invoiced finds it under the unchanged external id
/// (`already_issued`); an unscoped call on the multi-account deployment is
/// `unknown_account` (400) with `namespace` and `account` journaled and
/// nothing else, and zero szamlazz.hu requests.
async fn flag_day_keeps_the_documents_and_refuses_unscoped_calls(h: &mut Harness) {
    h.reset().await;

    // Private: the ingress refuses the call itself; nothing reaches the
    // handler or szamlazz.hu.
    h.set_public(false).await;
    let reply = h
        .call(
            "E2E-16",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-16-private",
        )
        .await;
    assert_eq!(reply.status, 400, "a private service: {}", reply.body);
    assert_eq!(reply.invocation_id, None, "no invocation was created");
    assert_eq!(h.requests_seen().await, 0);

    h.switch_to_multi_account().await;

    // The document issued unscoped in phase 1 (E2E-1 → SZ-2) is found by the
    // first scoped create under `acme`: the external id did not change.
    h.absent("E2E-1", &["prepayment", "final", "proforma"])
        .await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-1:invoice"),
        ..Doc::new("SZ-2", "SZ", "E2E-1")
    })
    .await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_scoped(
            "acme",
            "E2E-1",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-1-scoped-k1",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "already_issued", "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "SZ-2");
    assert_eq!(reply.body["external_id"], "acct:E2E-1:invoice");
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.scope.as_deref(), Some("acme"), "{invocation:?}");
    let journal = h.journal(reply.invocation_id()).await;
    let account = run_result(&journal, "account").expect("the account result");
    assert!(
        account.raw_contains("\"id\":\"acme\""),
        "{:?}",
        String::from_utf8_lossy(&account.raw)
    );

    // Unscoped on the multi-account deployment: refused before anything is
    // issued, the resolution journaled as data.
    h.reset().await;
    let reply = h
        .call(
            "E2E-16",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-16-unscoped",
        )
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "unknown_account", "{fault:?}");
    assert!(fault.message.contains("unscoped"), "{fault:?}");
    assert!(
        fault.message.contains("/restate/scope/"),
        "the fault tells the caller how to address an account: {fault:?}"
    );
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account"]
    );
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.scope, None, "{invocation:?}");
    assert_eq!(h.requests_seen().await, 0, "nothing reached szamlazz.hu");

    // A scope no account is reachable by is unknown the same way.
    let reply = h
        .call_scoped(
            "gamma",
            "E2E-16",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-16-gamma",
        )
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "unknown_account", "{fault:?}");
    assert!(fault.message.contains("gamma"), "{fault:?}");
    assert_eq!(h.requests_seen().await, 0);
    eprintln!(
        "(xvi) flag day: private → drain → multi; scoped create finds the unscoped-phase document; unscoped → unknown_account: pass"
    );
}

/// (xvii) the same order key under scopes `acme` and `beta`, concurrently:
/// two Virtual Objects, two `issued`, each account's own agent key on the
/// create wire exactly once — Restate namespaces the Virtual Object key per
/// scope, and the prologue opens each execution's gateway on its own
/// account. (The lookup queries carry the key as well; the create bodies are
/// what identify *which account issued*.)
async fn same_order_key_under_two_scopes_issues_on_both_accounts(h: &Harness) {
    h.reset().await;
    h.absent("E2E-17", &["prepayment", "final", "proforma", "invoice"])
        .await;
    order_query("E2E-17")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create_with_key(AGENT_KEY)
        .respond_with(created("SZ-A17", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    create_with_key(KEY_B)
        .respond_with(created("SZ-B17", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;

    let body = create_body(dec!(1000), false);
    let (acme, beta) = tokio::join!(
        h.call_scoped("acme", "E2E-17", "create_invoice", &body, "e2e-17-acme"),
        h.call_scoped("beta", "E2E-17", "create_invoice", &body, "e2e-17-beta"),
    );
    assert_eq!(acme.status, 200, "{}", acme.body);
    assert_eq!(beta.status, 200, "{}", beta.body);
    assert_eq!(acme.body["outcome"], "issued", "{}", acme.body);
    assert_eq!(beta.body["outcome"], "issued", "{}", beta.body);
    assert_eq!(
        acme.body["invoice_number"], "SZ-A17",
        "acme's key issued acme's document"
    );
    assert_eq!(
        beta.body["invoice_number"], "SZ-B17",
        "beta's key issued beta's document"
    );
    assert_eq!(acme.body["external_id"], "acct:E2E-17:invoice");
    assert_eq!(
        beta.body["external_id"], "acct:E2E-17:invoice",
        "the same namespace and order: the same external id on two szamlazz.hu accounts"
    );

    let creates = h.create_bodies().await;
    assert_eq!(creates.len(), 2, "one create per account");
    for key in [AGENT_KEY, KEY_B] {
        assert_eq!(
            creates
                .iter()
                .filter(|body| body.contains(&agent_key_tag(key)))
                .count(),
            1,
            "{key} on the create wire exactly once"
        );
    }
    for (reply, scope, id) in [(&acme, "acme", "acme"), (&beta, "beta", "beta")] {
        let invocation = h.invocation(reply.invocation_id()).await;
        assert_eq!(invocation.scope.as_deref(), Some(scope), "{invocation:?}");
        assert_eq!(invocation.status, "completed");
        let journal = h.journal(reply.invocation_id()).await;
        let account = run_result(&journal, "account").expect("the account result");
        assert!(
            account.raw_contains(&format!("\"id\":\"{id}\"")),
            "{scope}: {:?}",
            String::from_utf8_lossy(&account.raw)
        );
    }
    eprintln!(
        "(xvii) same order key under two scopes concurrently → two issued, each key on the create wire once: pass"
    );
}

/// (xvii-b) the **same** `Idempotency-Key` under two scopes is two
/// invocations — two `x-restate-id`s, two documents — because Restate hashes
/// the scope into the idempotency identity; and the key replayed under
/// either scope returns that scope's own stored completion without a call.
async fn same_idempotency_key_under_two_scopes_is_two_invocations(h: &Harness) {
    h.reset().await;
    h.absent("E2E-17B", &["prepayment", "final", "proforma", "invoice"])
        .await;
    order_query("E2E-17B")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create_with_key(AGENT_KEY)
        .respond_with(created("SZ-A17B", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    create_with_key(KEY_B)
        .respond_with(created("SZ-B17B", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;

    let body = create_body(dec!(1000), false);
    let acme = h
        .call_scoped("acme", "E2E-17B", "create_invoice", &body, "e2e-17b-shared")
        .await;
    let beta = h
        .call_scoped("beta", "E2E-17B", "create_invoice", &body, "e2e-17b-shared")
        .await;
    assert_eq!(acme.status, 200, "{}", acme.body);
    assert_eq!(beta.status, 200, "{}", beta.body);
    assert_eq!(acme.body["outcome"], "issued", "{}", acme.body);
    assert_eq!(beta.body["outcome"], "issued", "{}", beta.body);
    assert_eq!(acme.body["invoice_number"], "SZ-A17B");
    assert_eq!(beta.body["invoice_number"], "SZ-B17B");
    assert_ne!(
        acme.invocation_id(),
        beta.invocation_id(),
        "the same Idempotency-Key under two scopes is two invocations"
    );
    assert_eq!(h.create_bodies().await.len(), 2, "two documents");

    // The key again under each scope replays that scope's own completion.
    let before = h.requests_seen().await;
    for (scope, original) in [("acme", &acme), ("beta", &beta)] {
        let replay = h
            .call_scoped(scope, "E2E-17B", "create_invoice", &body, "e2e-17b-shared")
            .await;
        assert_eq!(replay.status, 200, "{}", replay.body);
        assert_eq!(
            replay.body["invoice_number"], original.body["invoice_number"],
            "{scope}"
        );
        assert_eq!(replay.invocation_id(), original.invocation_id(), "{scope}");
    }
    assert_eq!(h.requests_seen().await, before, "replays, not calls");
    eprintln!(
        "(xvii-b) same Idempotency-Key under two scopes → two invocation ids, two documents; each replays its own: pass"
    );
}

/// (xvii-c) `check_account` under each scope of the multi-account deployment
/// names that scope's account with `credentials: ok` and `scope` as the SDK
/// saw it, and the probe on the wire carries that account's key and nothing
/// else — the deploy-pipeline proof that a scope reaches the worker, resolves
/// to the intended account and its key works. Unscoped it is
/// `unknown_account` with `namespace` and `account` journaled and no
/// szamlazz.hu request.
async fn check_account_under_each_scope_names_its_account(h: &Harness) {
    h.reset().await;
    probe_with_key(AGENT_KEY)
        .respond_with(not_found())
        .expect(1)
        .mount(&h.mock)
        .await;
    probe_with_key(KEY_B)
        .respond_with(not_found())
        .expect(1)
        .mount(&h.mock)
        .await;

    for (scope, id) in [("acme", "acme"), ("beta", "beta")] {
        let reply = h.check_account(Some(scope)).await;
        assert_eq!(reply.status, 200, "{scope}: {}", reply.body);
        assert_eq!(
            reply.body,
            json!({
                "scope": scope,
                "account": { "id": id },
                "namespace": "acct",
                "credentials": { "state": "ok" },
            }),
            "{scope}"
        );
        assert_eq!(
            h.runs(reply.invocation_id()).await,
            ["namespace", "account", "probe"],
            "{scope}"
        );
        let invocation = h.invocation(reply.invocation_id()).await;
        assert_eq!(invocation.scope.as_deref(), Some(scope), "{invocation:?}");
        assert_eq!(invocation.handler, "check_account");
    }
    assert_eq!(
        h.requests_seen().await,
        2,
        "one probe per account, each with its own key, nothing else"
    );

    // Unscoped on the multi-account deployment: no account to probe.
    h.reset().await;
    let reply = h.check_account(None).await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "unknown_account", "{fault:?}");
    assert!(fault.message.contains("unscoped"), "{fault:?}");
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account"]
    );
    assert_eq!(h.requests_seen().await, 0, "nothing reached szamlazz.hu");

    eprintln!(
        "(xvii-c) check_account under acme and beta → each its account with its key on the probe, credentials ok; unscoped → unknown_account: pass"
    );
}

/// (xviii) an order Restate has no memory of: `acme` issues an invoice, the
/// invocation is purged; `storno_invoice` → `reversed` (its lookup finds the
/// document by number on szamlazz.hu), purged; `create_invoice {reissue}` →
/// `issued` as the newest holder of the same external id. Nothing but the
/// order key and the scope was needed.
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: issue, purge, storno, purge, reissue, get"
)]
async fn purged_order_is_stornoed_and_reissued(h: &Harness) {
    h.reset().await;
    h.absent("E2E-18", &["prepayment", "final", "proforma", "invoice"])
        .await;
    order_query("E2E-18")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create_with_key(AGENT_KEY)
        .respond_with(created("SZ-18", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let issued = h
        .call_scoped(
            "acme",
            "E2E-18",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-18-k1",
        )
        .await;
    assert_eq!(issued.status, 200, "{}", issued.body);
    assert_eq!(issued.body["outcome"], "issued", "{}", issued.body);
    h.purge(issued.invocation_id()).await;

    // Storno: the invoice is verified by number and reversed.
    h.reset().await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-18:invoice"),
        ..Doc::new("SZ-18", "SZ", "E2E-18")
    })
    .await;
    external_id_query("acct:E2E-18:storno:SZ-18")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    storno_repeating_telj()
        .and(body_string_contains(agent_key_tag(AGENT_KEY)))
        .respond_with(created("SS-18", "-1000", "-1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reversed = h
        .call_scoped(
            "acme",
            "E2E-18",
            "storno_invoice",
            &json!({ "invoice_number": "SZ-18" }),
            "e2e-18-s1",
        )
        .await;
    assert_eq!(reversed.status, 200, "{}", reversed.body);
    assert_eq!(reversed.body["outcome"], "reversed", "{}", reversed.body);
    assert_eq!(reversed.body["storno_number"], "SS-18");
    h.purge(reversed.invocation_id()).await;
    assert!(
        h.journal(issued.invocation_id()).await.is_empty()
            && h.journal(reversed.invocation_id()).await.is_empty(),
        "Restate holds nothing of the order"
    );

    // Reissue: the lookup sees the reversed document and its storno; the
    // create step issues the next one under the same id.
    h.reset().await;
    h.absent("E2E-18", &["prepayment", "final", "proforma"])
        .await;
    external_id_query("acct:E2E-18:invoice")
        .respond_with(
            Doc {
                reversed: true,
                ..Doc::new("SZ-18", "SZ", "E2E-18")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    order_query("E2E-18")
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-18"),
                ..Doc::new("SS-18", "SS", "E2E-18")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    create_with_key(AGENT_KEY)
        .respond_with(created("SZ-18B", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reissued = h
        .call_scoped(
            "acme",
            "E2E-18",
            "create_invoice",
            &create_body(dec!(1000), true),
            "e2e-18-k2",
        )
        .await;
    assert_eq!(reissued.status, 200, "{}", reissued.body);
    assert_eq!(reissued.body["outcome"], "issued", "{}", reissued.body);
    assert_eq!(reissued.body["invoice_number"], "SZ-18B");
    assert_eq!(reissued.body["external_id"], "acct:E2E-18:invoice");

    // The scoped live view sees the new holder.
    h.reset().await;
    h.absent("E2E-18", &["proforma", "prepayment", "final"])
        .await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-18:invoice"),
        ..Doc::new("SZ-18B", "SZ", "E2E-18")
    })
    .await;
    let status = h.get_scoped("acme", "E2E-18").await;
    assert_eq!(status["invoice"]["number"], "SZ-18B", "{status}");
    assert_eq!(status["invoice"]["state"], "live");
    eprintln!("(xviii) purged order → storno → reversed; purged → reissue → issued: pass");
}

/// (xviii-b) `Szamlazz.Agent.storno` under a scope acts on what the verify
/// finds and compares it with nothing about the account (ADR 0006,
/// account-pin amendment): a document whose seller block carries another
/// `szallito/id` and whose `teszt` says a live account issued it is reversed
/// with `acme`'s key like any of the account's own; a document carrying an
/// order number is `managed_by_order` with nothing sent.
async fn agent_storno_acts_on_what_the_verify_finds(h: &Harness) {
    // A document whose `teszt` and seller record id are not what `acme`'s
    // documents carry: nothing compares them, the storno proceeds with
    // `acme`'s key.
    h.reset().await;
    h.holds(&Doc {
        test: false,
        supplier_id: SUPPLIER_B,
        ..Doc::unmanaged("SZ-22", "SZ")
    })
    .await;
    external_id_query("acct:by-number:SZ-22:storno")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    storno_repeating_telj()
        .and(body_string_contains(agent_key_tag(AGENT_KEY)))
        .respond_with(created("SS-22", "-1000", "-1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent_scoped("acme", "storno", &storno_of("SZ-22"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "reversed", "{}", reply.body);
    assert_eq!(reply.body["storno_number"], "SS-22", "{}", reply.body);

    // A document as `acme`'s own read: reversed, as before, through verify,
    // lookup and storno.
    h.reset().await;
    h.holds(&Doc::unmanaged("SZ-23", "SZ")).await;
    external_id_query("acct:by-number:SZ-23:storno")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    storno_repeating_telj()
        .and(body_string_contains(agent_key_tag(AGENT_KEY)))
        .respond_with(created("SS-23", "-1000", "-1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent_scoped("acme", "storno", &storno_of("SZ-23"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "reversed", "{}", reply.body);
    assert_eq!(reply.body["storno_number"], "SS-23", "{}", reply.body);
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "verify-SZ-23",
            "lookup-storno-SZ-23",
            "storno-SZ-23"
        ]
    );

    // A document carrying an order number is `managed_by_order`, nothing
    // sent.
    h.reset().await;
    number_query("SZ-25")
        .respond_with(Doc::new("SZ-25", "SZ", "E2E-25").response())
        .expect(1)
        .mount(&h.mock)
        .await;
    storno_never_sent(&h.mock).await;
    let reply = h
        .call_agent_scoped("acme", "storno", &storno_of("SZ-25"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "managed_by_order", "{}", reply.body);
    assert_eq!(reply.body["order_key"], "E2E-25", "{}", reply.body);
    assert_eq!(h.requests_seen().await, 1, "the verify, nothing else");
    eprintln!(
        "(xviii-b) Szamlazz.Agent.storno under a scope: another teszt / seller record id compared with nothing → reversed with acme's key; own → reversed; order-bearing → managed_by_order, nothing sent: pass"
    );
}

/// (xviii-c) `Szamlazz.Agent.query` under a scope answers the projection of
/// whatever it finds — `test` as szamlazz.hu reported it, compared with
/// nothing (ADR 0006, account-pin amendment: the go-live check reads it off
/// a known document here), no `supplier_id`; code 7 is 404 `not_found`.
async fn agent_query_projects_what_it_finds(h: &Harness) {
    let query_of = |number: &str| json!({ "selector": { "invoice_number": number } });

    // A document a live account issued: projected, `test: false` reported
    // as is.
    h.reset().await;
    number_query("SZ-25")
        .respond_with(
            Doc {
                test: false,
                ..Doc::unmanaged("SZ-25", "SZ")
            }
            .response(),
        )
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent_scoped("acme", "query", &query_of("SZ-25"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "SZ-25", "{}", reply.body);
    assert_eq!(reply.body["test"], false, "{}", reply.body);
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account", "query"]
    );

    h.reset().await;
    number_query("SZ-26")
        .respond_with(Doc::new("SZ-26", "SZ", "E2E-26").response())
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent_scoped("acme", "query", &query_of("SZ-26"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "SZ-26", "{}", reply.body);
    assert_eq!(reply.body["document_type"], "SZ", "{}", reply.body);
    assert_eq!(reply.body["order_number"], "E2E-26", "{}", reply.body);
    assert_eq!(reply.body["test"], true, "{}", reply.body);
    assert!(
        reply.body.get("supplier_id").is_none(),
        "the seller record's id is not projected: {}",
        reply.body
    );
    assert_eq!(reply.body["gross_total"], "1270", "{}", reply.body);

    h.reset().await;
    number_query("SZ-27")
        .respond_with(not_found())
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent_scoped("acme", "query", &query_of("SZ-27"))
        .await;
    assert_eq!(reply.status, 404, "{}", reply.body);
    assert_eq!(reply.fault().code, "not_found", "{}", reply.body);
    eprintln!(
        "(xviii-c) Szamlazz.Agent.query under a scope: the projection with test as reported, no supplier_id; 7 → not_found: pass"
    );
}

/// (xviii-c') Every fault either service raises carries a `TerminalCode` token
/// in `code`, and a szamlazz.hu code travels in `szamlazz_code` beside it,
/// never in `code` (#67). Pinned at the ingress, on both services: a
/// szamlazz.hu code on `Szamlazz.Agent.query` is 422 `szamlazz_error` with
/// `szamlazz_code`; a sixth credit entry on `set_payments` never reaches
/// szamlazz.hu and is 400 `invalid_input` without a `szamlazz_code`; a
/// credit entry szamlazz.hu refuses is 422 `szamlazz_error` naming the
/// invoice; an unknown invoice on `Szamlazz.Order.storno_invoice` is 404
/// `not_found` — the same token `Szamlazz.Agent` answers — attaching the
/// order, kind and storno external id.
async fn every_fault_carries_a_terminal_code_and_the_szamlazz_code_beside_it(h: &Harness) {
    let credit_entry = json!({ "date": "2026-09-05", "method": "transfer", "amount": "1270" });
    let set_payments_of = |number: &str, entries: usize| {
        json!({
            "invoice_number": number,
            "entries": vec![credit_entry.clone(); entries],
        })
    };

    // A szamlazz.hu code on `query`: the pass-through.
    h.reset().await;
    number_query("SZ-28")
        .respond_with(api_error("57", "Hibás számlaszám."))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent_scoped(
            "acme",
            "query",
            &json!({ "selector": { "invoice_number": "SZ-28" } }),
        )
        .await;
    assert_eq!(reply.status, 422, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "szamlazz_error", "{fault:?}");
    assert_eq!(fault.szamlazz_code.as_deref(), Some("57"), "{fault:?}");
    assert!(fault.message.contains("Hibás számlaszám."), "{fault:?}");
    assert_eq!(fault.order, None, "{fault:?}");

    // A sixth credit entry: the caller's request, nothing sent.
    h.reset().await;
    op("action-szamla_agent_kifiz")
        .respond_with(api_error("999", "never"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent_scoped("acme", "set_payments", &set_payments_of("SZ-29", 6))
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "invalid_input", "{fault:?}");
    assert_eq!(fault.szamlazz_code, None, "{fault:?}");
    assert!(fault.message.contains("at most five"), "{fault:?}");
    assert_eq!(h.requests_seen().await, 0, "nothing reached szamlazz.hu");

    // A refused credit entry: szamlazz.hu's answer, passed through.
    h.reset().await;
    op("action-szamla_agent_kifiz")
        .and(body_string_contains("SZ-30"))
        .respond_with(api_error(
            "463",
            "Sztornózó vagy sztornózott számlához nem tartozhat kifizetettségi információ.",
        ))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent_scoped("acme", "set_payments", &set_payments_of("SZ-30", 1))
        .await;
    assert_eq!(reply.status, 422, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "szamlazz_error", "{fault:?}");
    assert_eq!(fault.szamlazz_code.as_deref(), Some("463"), "{fault:?}");
    assert!(fault.message.contains("SZ-30"), "{fault:?}");
    assert!(fault.message.contains("Sztornózó"), "{fault:?}");

    // An unknown invoice on the order's storno: `not_found`, like the agent's.
    h.reset().await;
    number_query("SZ-34")
        .respond_with(not_found())
        .expect(1)
        .mount(&h.mock)
        .await;
    storno_never_sent(&h.mock).await;
    let reply = h
        .call_scoped(
            "acme",
            "E2E-34",
            "storno_invoice",
            &storno_of("SZ-34"),
            "e2e-34-s1",
        )
        .await;
    assert_eq!(reply.status, 404, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "not_found", "{fault:?}");
    assert_eq!(fault.szamlazz_code, None, "{fault:?}");
    assert!(fault.message.contains("SZ-34"), "{fault:?}");
    assert_eq!(fault.order.as_deref(), Some("E2E-34"), "{fault:?}");
    assert_eq!(fault.kind, None, "{fault:?}");
    assert_eq!(
        fault.external_id.as_deref(),
        Some("acct:E2E-34:storno:SZ-34"),
        "{fault:?}"
    );
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account", "verify-storno-SZ-34"],
        "the verify is the only step journaled"
    );
    eprintln!(
        "(xviii-c') faults: query Api → 422 szamlazz_error{{szamlazz_code}}; sixth entry → 400 invalid_input, nothing sent; refused entry → 422; order storno on 7 → 404 not_found with identity: pass"
    );
}

/// (xviii-d) `Szamlazz.Agent.query_taxpayer` under a scope asks NAV through
/// that scope's account: the `xmltaxpayer` request on the wire carries that
/// account's key and nothing else reaches szamlazz.hu; the full tax number
/// and its bare stem are one step, `taxpayer-{prefix}`; `valid: false` is
/// data; a malformed tax number is `invalid_input` before the prologue —
/// nothing journaled, nothing sent. The run-wide leak scan (xxi) covers
/// these invocations too.
async fn agent_query_taxpayer_runs_on_the_scoped_account(h: &Harness) {
    let request = |tax_number: &str| json!({ "tax_number": tax_number });

    // `acme` with its key, the full tax number; `beta` with its key, the
    // bare stem — the same prefix, the same step name on both.
    h.reset().await;
    taxpayer_query_with_key("12345678", AGENT_KEY)
        .respond_with(taxpayer_known())
        .expect(1)
        .mount(&h.mock)
        .await;
    taxpayer_query_with_key("12345678", KEY_B)
        .respond_with(taxpayer_unknown())
        .expect(1)
        .mount(&h.mock)
        .await;

    let reply = h
        .call_agent_scoped("acme", "query_taxpayer", &request("12345678-2-42"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["valid"], true, "{}", reply.body);
    assert_eq!(
        reply.body["name"], "SYNTHETIC SOFTWARE KFT.",
        "{}",
        reply.body
    );
    assert_eq!(reply.body["tax_number"], "12345678", "{}", reply.body);
    assert_eq!(reply.body["vat_code"], "2", "{}", reply.body);
    assert_eq!(reply.body["addresses"][0]["kind"], "HQ", "{}", reply.body);
    assert_eq!(
        reply.body["addresses"][0]["city"], "TESTVAROS",
        "{}",
        reply.body
    );
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account", "taxpayer-12345678"]
    );
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.scope.as_deref(), Some("acme"), "{invocation:?}");
    assert_eq!(invocation.handler, "query_taxpayer");

    let reply = h
        .call_agent_scoped("beta", "query_taxpayer", &request("12345678"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(
        reply.body,
        json!({ "valid": false, "name": null, "tax_number": null, "vat_code": null, "addresses": [] }),
        "valid: false is data"
    );
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account", "taxpayer-12345678"]
    );
    assert_eq!(
        h.requests_seen().await,
        2,
        "one taxpayer query per account, each with its own key, nothing else"
    );

    // A malformed tax number: refused before the prologue.
    h.reset().await;
    let reply = h
        .call_agent_scoped("acme", "query_taxpayer", &request("12345678-2"))
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "invalid_input", "{fault:?}");
    assert!(fault.message.contains("\"12345678-2\""), "{fault:?}");
    assert!(fault.message.contains("12345678-2-42"), "{fault:?}");
    assert!(
        h.runs(reply.invocation_id()).await.is_empty(),
        "nothing journaled before the refusal"
    );
    assert_eq!(h.requests_seen().await, 0, "nothing reached szamlazz.hu");

    eprintln!(
        "(xviii-d) Szamlazz.Agent.query_taxpayer under acme and beta → each asks NAV with its own key, one step taxpayer-{{prefix}} for the full number and the stem, valid: false as data; malformed → invalid_input before the prologue: pass"
    );
}

/// (xviii-e) `Szamlazz.Agent.storno` repeats the original's `telj` too (ADR
/// 0007), under a scope: a document is reversed with the storno carrying
/// `teljesitesDatum` and `acme`'s key; one without a `telj` is 503
/// `unavailable` naming the invoice — without `order`, `kind` or
/// `external_id`, as this handler's other faults — with only the verify
/// journaled and nothing sent; and the fault comes after the answers that
/// need no send: a `telj`-less order-bearing document is still
/// `managed_by_order`, a reversed one still `reversed`.
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the date on the wire, the fault and its two predecessors"
)]
async fn agent_storno_repeats_the_originals_fulfillment_date_or_refuses(h: &Harness) {
    let without_telj = |number: &'static str| Doc {
        fulfillment_date: None,
        ..Doc::unmanaged(number, "SZ")
    };

    // The date on the wire.
    h.reset().await;
    number_query("SZ-31")
        .respond_with(Doc::unmanaged("SZ-31", "SZ").response())
        .mount(&h.mock)
        .await;
    external_id_query("acct:by-number:SZ-31:storno")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    storno_repeating_telj()
        .and(body_string_contains(agent_key_tag(AGENT_KEY)))
        .respond_with(created("SS-31", "-1000", "-1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent_scoped("acme", "storno", &storno_of("SZ-31"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "reversed", "{}", reply.body);
    assert_eq!(reply.body["storno_number"], "SS-31", "{}", reply.body);
    let stornos = h.storno_bodies().await;
    assert_eq!(stornos.len(), 1);
    assert!(!stornos[0].contains("<keltDatum>"), "{}", stornos[0]);

    // The fault, without an order identity.
    h.reset().await;
    number_query("SZ-32")
        .respond_with(without_telj("SZ-32").response())
        .expect(1)
        .mount(&h.mock)
        .await;
    storno_never_sent(&h.mock).await;
    let reply = h
        .call_agent_scoped("acme", "storno", &storno_of("SZ-32"))
        .await;
    assert_eq!(reply.status, 503, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "unavailable", "{fault:?}");
    assert!(fault.message.contains("SZ-32"), "{fault:?}");
    assert!(fault.message.contains("fulfillment date"), "{fault:?}");
    assert_eq!(fault.order, None, "{fault:?}");
    assert_eq!(fault.kind, None, "{fault:?}");
    assert_eq!(fault.external_id, None, "{fault:?}");
    for key in AGENT_KEYS {
        assert!(!fault.message.contains(key), "{fault:?}");
    }
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account", "verify-SZ-32"],
        "the verify is the only step journaled"
    );
    assert_eq!(h.requests_seen().await, 1, "the verify, nothing else");

    // Before the fault: an order-bearing document is `managed_by_order`.
    h.reset().await;
    number_query("SZ-34")
        .respond_with(
            Doc {
                fulfillment_date: None,
                ..Doc::new("SZ-34", "SZ", "E2E-34")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    storno_never_sent(&h.mock).await;
    let reply = h
        .call_agent_scoped("acme", "storno", &storno_of("SZ-34"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "managed_by_order", "{}", reply.body);
    assert_eq!(reply.body["order_key"], "E2E-34", "{}", reply.body);
    assert_eq!(h.requests_seen().await, 1);

    // Before the fault: a reversed document is `reversed`, with the storno
    // number the by-number storno lookup names — nothing under the id (a
    // reversal from the UI) leaves it unknown; the verify and the lookup are
    // the only requests, nothing is sent (J25, #65).
    h.reset().await;
    number_query("SZ-35")
        .respond_with(
            Doc {
                reversed: true,
                ..without_telj("SZ-35")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    external_id_query("acct:by-number:SZ-35:storno")
        .respond_with(not_found())
        .expect(1)
        .mount(&h.mock)
        .await;
    storno_never_sent(&h.mock).await;
    let reply = h
        .call_agent_scoped("acme", "storno", &storno_of("SZ-35"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "reversed", "{}", reply.body);
    assert_eq!(reply.body["storno_number"], Value::Null, "{}", reply.body);
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "verify-SZ-35",
            "lookup-storno-SZ-35"
        ]
    );
    assert_eq!(h.requests_seen().await, 2, "the verify and the lookup");

    // Reversed by a storno of ours (a lost reply, a retry with a new key):
    // the lookup names it.
    h.reset().await;
    number_query("SZ-36")
        .respond_with(
            Doc {
                reversed: true,
                ..without_telj("SZ-36")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    external_id_query("acct:by-number:SZ-36:storno")
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-36"),
                ..Doc::unmanaged("SS-36", "SS")
            }
            .response(),
        )
        .expect(1)
        .mount(&h.mock)
        .await;
    storno_never_sent(&h.mock).await;
    let reply = h
        .call_agent_scoped("acme", "storno", &storno_of("SZ-36"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "reversed", "{}", reply.body);
    assert_eq!(reply.body["storno_number"], "SS-36", "{}", reply.body);
    assert_eq!(h.requests_seen().await, 2, "the verify and the lookup");
    eprintln!(
        "(xviii-e) Szamlazz.Agent.storno: teljesitesDatum on the wire; telj-less → unavailable without an order identity, after managed_by_order / reversed (storno number from the by-number lookup): pass"
    );
}

/// (xviii-f) The storno's `eszamla` is the verified original's appearance,
/// not the account default — on both storno handlers. szamlazz.hu accepts a
/// mismatch silently and issues the storno in the *request's* form (P73), so
/// the derivation is the only thing keeping a reversal in its original's
/// form. `acme` is switched to issuing e-invoices by default; a **paper**
/// original (`<eszamla>1</eszamla>`) is still reversed with
/// `<eszamla>false</eszamla>` by `Szamlazz.Order.storno_invoice`, and — the
/// default switched back to paper — an **e-invoice** original (`3`, the code
/// szamlazz.hu was observed to report) is reversed with
/// `<eszamla>true</eszamla>` by `Szamlazz.Agent.storno`.
async fn storno_is_issued_in_the_originals_form_not_the_accounts_default(h: &Harness) {
    // A paper original under an account that defaults to e-invoices.
    h.reset().await;
    h.multi()
        .update("acme", |account| account.defaults.e_invoice = true);
    h.holds(&Doc {
        eszamla: Some(1),
        ..Doc::new("SZ-73", "SZ", "E2E-73")
    })
    .await;
    external_id_query("acct:E2E-73:storno:SZ-73")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    storno_repeating_telj()
        .and(body_string_contains("<eszamla>false</eszamla>"))
        .respond_with(created("SS-73", "-1000", "-1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    storno()
        .and(body_string_contains("<eszamla>true</eszamla>"))
        .respond_with(created("SS-X", "-1000", "-1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_scoped(
            "acme",
            "E2E-73",
            "storno_invoice",
            &storno_of("SZ-73"),
            "e2e-73-k1",
        )
        .await;
    h.multi()
        .update("acme", |account| account.defaults.e_invoice = false);
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "reversed", "{}", reply.body);
    assert_eq!(reply.body["storno_number"], "SS-73", "{}", reply.body);
    let stornos = h.storno_bodies().await;
    assert_eq!(stornos.len(), 1, "{stornos:?}");
    assert!(
        stornos[0].contains("<eszamla>false</eszamla>"),
        "a paper original is reversed on paper under an e-invoice default: {}",
        stornos[0]
    );

    // An e-invoice original (`3`) under the paper default, by number.
    h.reset().await;
    h.holds(&Doc {
        eszamla: Some(3),
        ..Doc::unmanaged("SZ-74", "SZ")
    })
    .await;
    external_id_query("acct:by-number:SZ-74:storno")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    storno_repeating_telj()
        .and(body_string_contains("<eszamla>true</eszamla>"))
        .respond_with(created("SS-74", "-1000", "-1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    storno()
        .and(body_string_contains("<eszamla>false</eszamla>"))
        .respond_with(created("SS-X", "-1000", "-1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent_scoped("acme", "storno", &storno_of("SZ-74"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "reversed", "{}", reply.body);
    assert_eq!(reply.body["storno_number"], "SS-74", "{}", reply.body);
    let stornos = h.storno_bodies().await;
    assert_eq!(stornos.len(), 1, "{stornos:?}");
    assert!(
        stornos[0].contains("<eszamla>true</eszamla>"),
        "an e-invoice original is reversed as an e-invoice under a paper default: {}",
        stornos[0]
    );
    eprintln!(
        "(xviii-f) storno eszamla is the verified original's (1 → false under an e-invoice default; 3 → true under a paper default), on both handlers: pass"
    );
}

/// (xix) `acme`'s seller bank account changes between two executions of a
/// create step (the first loses its reply): the second execution's create
/// carries the **journaled** account's bank account — the invocation
/// finishes on the account it started on — and only new invocations see the
/// change.
async fn account_change_between_executions_does_not_reach_the_invocation(h: &Harness) {
    h.reset().await;
    h.absent("E2E-19", &["prepayment", "final", "proforma", "invoice"])
        .await;
    order_query("E2E-19")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    // The first create with the journaled bank account loses its reply; the
    // second, still with it, lands. The changed one never reaches the wire.
    create_with_bank_account(BANK_ACCOUNT)
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .expect(1)
        .mount(&h.mock)
        .await;
    create_with_bank_account(BANK_ACCOUNT)
        .respond_with(created("SZ-19", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    create_with_bank_account(BANK_ACCOUNT_CHANGED)
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;

    let body = create_body(dec!(1000), false);
    let call = h.call_scoped("acme", "E2E-19", "create_invoice", &body, "e2e-19-k1");
    let change = async {
        h.wait_for_creates(1).await;
        h.multi().update("acme", |account| {
            account.seller.bank_account = Some(BANK_ACCOUNT_CHANGED.to_owned());
        });
    };
    let (reply, ()) = tokio::join!(call, change);
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "issued", "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "SZ-19");
    let creates = h.create_bodies().await;
    assert_eq!(creates.len(), 2, "two executions of the create step");
    assert!(
        creates
            .iter()
            .all(|body| body.contains(&format!("<bankszamlaszam>{BANK_ACCOUNT}</bankszamlaszam>"))),
        "both executions carry the journaled seller"
    );
    let journal = h.journal(reply.invocation_id()).await;
    let account = run_result(&journal, "account").expect("the account result");
    assert!(
        account.raw_contains(BANK_ACCOUNT) && !account.raw_contains(BANK_ACCOUNT_CHANGED),
        "{:?}",
        String::from_utf8_lossy(&account.raw)
    );
    assert_eq!(
        journal
            .iter()
            .filter(|entry| entry.is_run() && entry.name.as_deref() == Some("account"))
            .count(),
        1,
        "the re-execution replayed the account, it did not resolve again"
    );

    // A new invocation resolves the changed account.
    h.reset().await;
    h.absent("E2E-19B", &["prepayment", "final", "proforma", "invoice"])
        .await;
    order_query("E2E-19B")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create_with_bank_account(BANK_ACCOUNT_CHANGED)
        .respond_with(created("SZ-19B", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_scoped(
            "acme",
            "E2E-19B",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-19b-k1",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "issued", "{}", reply.body);
    eprintln!("(xix) account change between executions → the journaled account wins: pass");
}

/// (xx) `beta`'s agent key is rotated between two executions of a create
/// step (the first loses its reply): the second execution fetches the
/// credentials again and carries the new key, while the journaled `account`
/// entry is byte-identical before and after — credentials are never in it.
async fn credential_rotation_between_executions_is_picked_up(h: &Harness) {
    h.reset().await;
    h.absent("E2E-20", &["prepayment", "final", "proforma", "invoice"])
        .await;
    order_query("E2E-20")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create_with_key(KEY_B)
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&h.mock)
        .await;
    create_with_key(KEY_B_V2)
        .respond_with(created("SZ-20", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;

    let body = create_body(dec!(1000), false);
    let call = h.call_scoped("beta", "E2E-20", "create_invoice", &body, "e2e-20-k1");
    let rotate = async {
        // The first execution's create is on the wire: the `account` entry is
        // journaled and the re-execution is a second away. Read the entry as
        // journaled *before* the rotation, then rotate. (Journal entries are
        // immutable, so the comparison below proves the rotation left the
        // second execution's account as journaled — the credentials are not
        // part of it.)
        h.wait_for_creates(1).await;
        let invocations = h.all_invocations().await;
        let (id, _) = invocations
            .iter()
            .find(|(_, invocation)| {
                invocation.scope.as_deref() == Some("beta")
                    && invocation.handler == "create_invoice"
                    && invocation.status != "completed"
            })
            .expect("the in-flight invocation under beta");
        let before = run_result(&h.journal(id).await, "account")
            .expect("the account result while in flight")
            .raw
            .clone();
        h.multi().rotate("beta", KEY_B_V2);
        (id.clone(), before)
    };
    let (reply, (id, before)) = tokio::join!(call, rotate);
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "issued", "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "SZ-20");
    assert_eq!(reply.invocation_id(), id);

    let creates = h.create_bodies().await;
    assert_eq!(creates.len(), 2, "two executions of the create step");
    assert!(
        creates[0].contains(&agent_key_tag(KEY_B)),
        "execution one carried the old key"
    );
    assert!(
        creates[1].contains(&agent_key_tag(KEY_B_V2)),
        "execution two carried the rotated key"
    );
    let journal = h.journal(reply.invocation_id()).await;
    let account = run_result(&journal, "account").expect("the account result");
    assert_eq!(
        account.raw, before,
        "the journaled account is byte-identical"
    );
    assert!(account.raw_contains("\"id\":\"beta\""));
    assert!(
        !account.raw_contains(KEY_B) && !account.raw_contains(KEY_B_V2),
        "neither key is in the account entry"
    );
    assert_eq!(
        journal
            .iter()
            .filter(|entry| entry.is_run() && entry.name.as_deref() == Some("account"))
            .count(),
        1,
        "the re-execution replayed the account, it did not resolve again"
    );
    eprintln!(
        "(xx) credential rotation between executions → new key on the wire, account entry unchanged: pass"
    );
}

/// (xxi) the leak check over the whole run: the hex-decoded `raw` of every
/// journal entry of every invocation the server holds, and every
/// `completion_failure`, contain none of the agent keys the run put on the
/// wire — while the scan does find the positive control's sentinel from
/// (xii), so it reads real bytes.
async fn no_agent_key_in_any_journal_of_the_run(h: &Harness) {
    const POSITIVE_CONTROL: &str = "SENTINEL-8f3a2c-LEAK-CONTROL";
    let journals = h.all_journals().await;
    let invocations = h.all_invocations().await;
    assert!(
        journals.len() >= 20 && invocations.len() >= journals.len(),
        "the scan covers the run: {} journals, {} invocations",
        journals.len(),
        invocations.len()
    );
    let entries = journals.values().map(Vec::len).sum::<usize>();
    assert!(entries >= 100, "{entries} journal entries");

    let mut leaks = Vec::new();
    for (id, journal) in &journals {
        for entry in journal {
            for key in AGENT_KEYS {
                if entry.raw_contains(key) {
                    leaks.push(format!(
                        "{id} entry {} ({}, {:?}) contains {key}",
                        entry.index, entry.entry_type, entry.name
                    ));
                }
            }
        }
    }
    for (id, invocation) in &invocations {
        if let Some(failure) = &invocation.completion_failure {
            for key in AGENT_KEYS {
                if failure.contains(key) {
                    leaks.push(format!("{id} completion_failure contains {key}"));
                }
            }
        }
    }
    assert!(leaks.is_empty(), "agent keys in Restate: {leaks:#?}");

    assert!(
        journals
            .values()
            .flatten()
            .any(|entry| entry.raw_contains(POSITIVE_CONTROL)),
        "the positive control's sentinel is found by the same scan"
    );
    let scoped = invocations
        .iter()
        .filter(|(_, invocation)| invocation.scope.is_some())
        .count();
    assert!(scoped >= 8, "{scoped} scoped invocations were scanned");
    eprintln!(
        "(xxi) no agent key in {entries} journal entries of {} invocations ({scoped} scoped); positive control found: pass",
        invocations.len()
    );
}

/// (xxii) the run-name pin over the whole run ([`RUN_NAMES`]): for every
/// invocation the server still holds, the `ctx.run` names in journal order
/// are a prefix of one of its handler's pinned paths, every handler seen is
/// pinned, and every pinned path was walked in full by at least one
/// invocation — so a renamed, inserted, reordered or dropped step, on any
/// handler of either service, fails here rather than stranding an in-flight
/// invocation on the next deploy.
async fn every_handler_journals_its_pinned_run_names(h: &Harness) {
    let journals = h.all_journals().await;
    let invocations = h.all_invocations().await;
    let mut unpinned = BTreeSet::new();
    let mut unexplained = Vec::new();
    let mut walked = BTreeSet::new();
    for (id, invocation) in &invocations {
        let observed: Vec<String> = journals
            .get(id)
            .map(|journal| {
                journal
                    .iter()
                    .filter(|entry| entry.is_run())
                    .filter_map(|entry| entry.name.as_deref())
                    .map(run_pattern)
                    .collect()
            })
            .unwrap_or_default();
        let target = format!("{}.{}", invocation.service, invocation.handler);
        let paths: Vec<(usize, &[&str])> = RUN_NAMES
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                row.service == invocation.service && row.handler == invocation.handler
            })
            .map(|(index, row)| (index, row.path))
            .collect();
        if paths.is_empty() {
            unpinned.insert(target);
            continue;
        }
        if !paths
            .iter()
            .any(|(_, path)| is_prefix_of_path(&observed, path))
        {
            unexplained.push(format!("{id} {target}: {observed:?}"));
        }
        for (row, path) in paths {
            if observed.len() == path.len() && is_prefix_of_path(&observed, path) {
                walked.insert(row);
            }
        }
    }
    assert!(
        unpinned.is_empty(),
        "handlers with no pinned run names: {unpinned:?} — pin their steps in RUN_NAMES"
    );
    assert!(
        unexplained.is_empty(),
        "run sequences no pinned path of their handler explains:\n  {}\n\n\
         A renamed, inserted, reordered or dropped step strands every in-flight invocation of the \
         previous deployment on replay (ADR 0005). Keep the names and their order; a step that must \
         change is a new row in RUN_NAMES and a deploy that drains first, never an edited row.",
        unexplained.join("\n  ")
    );
    let not_walked: Vec<String> = RUN_NAMES
        .iter()
        .enumerate()
        .filter(|(row, _)| !walked.contains(row))
        .map(|(_, row)| format!("{}.{}: {:?}", row.service, row.handler, row.path))
        .collect();
    assert!(
        not_walked.is_empty(),
        "pinned paths no invocation of the run walked in full:\n  {}\n\n\
         Either a scenario must exercise the path or its last step was dropped from the handler.",
        not_walked.join("\n  ")
    );
    eprintln!(
        "(xxii) every run sequence of {} invocations is a prefix of its handler's pinned path; all {} paths walked in full: pass",
        invocations.len(),
        RUN_NAMES.len()
    );
}

// ----- the harness's stub helpers, against wiremock alone -----------------------

/// A query as the Számla Agent client puts it on the wire, reduced to what
/// the selector matchers read: the operation's field name and the one
/// selector element.
async fn query_by(mock: &MockServer, selector: &str) -> (u16, String) {
    let response = reqwest::Client::new()
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
/// order two, without either one — an unmounted selector is wiremock's 404.
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
/// selectors are not mounted — the document is absent before the misses.
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
/// code 7 until the create request is received — however many queries precede
/// it — and with the document from that moment on; the create itself is a
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
    let response = reqwest::Client::new()
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

// ----- the harness's server gate, without a server ------------------------------

/// The gate reads the environment once: a reusable server wins, then a
/// `restate-server` binary, then docker; with none of the three the suite
/// skips on a developer machine and **fails** under `CI`, naming every way to
/// provide a server — a suite that passes by skipping proves nothing.
#[test]
fn the_server_gate_prefers_a_reused_server_then_the_binary_then_docker() {
    let reuse = Some(("http://a:9070".to_owned(), "http://a:8080".to_owned()));
    let binary = Some(PathBuf::from("/opt/restate-server"));
    assert_eq!(
        server_gate(reuse.clone(), binary.clone(), || true, None),
        Ok(Some(Launcher::Reuse {
            admin: "http://a:9070".to_owned(),
            ingress: "http://a:8080".to_owned(),
        }))
    );
    assert_eq!(
        server_gate(None, binary.clone(), || true, None),
        Ok(Some(Launcher::Binary(PathBuf::from("/opt/restate-server"))))
    );
    assert_eq!(
        server_gate(None, binary, || false, Some(OsStr::new("true"))),
        Ok(Some(Launcher::Binary(PathBuf::from("/opt/restate-server")))),
        "the binary needs no docker, under CI too"
    );
    assert_eq!(
        server_gate(None, None, || true, None),
        Ok(Some(Launcher::Docker))
    );
}

#[test]
fn the_server_gate_skips_without_a_server_and_fails_under_ci() {
    assert_eq!(
        server_gate(None, None, || false, None),
        Ok(None),
        "no CI: skip"
    );
    assert_eq!(
        server_gate(None, None, || false, Some(OsStr::new(""))),
        Ok(None),
        "an empty CI is unset"
    );
    for ci in ["true", "1", "yes"] {
        let message = server_gate(None, None, || false, Some(OsStr::new(ci)))
            .expect_err("CI is set and there is no server: a failure, never a skip");
        for named in ["CI", "docker", "RESTATE_SERVER_BIN", "RESTATE_ADMIN_URL"] {
            assert!(message.contains(named), "CI={ci}: {message}");
        }
    }
}

// ----- the run-name pin's matching, without a server ----------------------------

/// A parametrized run name is read as its pattern by its prefix, the longest
/// prefix first: `verify-storno-SZ-1` is `verify-storno-{number}`, never
/// `verify-{number}`; a fixed name is itself.
#[test]
fn run_patterns_read_a_parametrized_name_by_its_longest_prefix() {
    for (name, pattern) in [
        ("namespace", "namespace"),
        ("lookup-invoice", "lookup-invoice"),
        ("lookup-storno-SZ-1", "lookup-storno-{number}"),
        ("storno-SZ-1", "storno-{number}"),
        ("verify-SZ-1", "verify-{number}"),
        ("verify-storno-E-TST-2026-1", "verify-storno-{number}"),
        ("verify-proforma-D-1", "verify-proforma-{number}"),
        ("verify-base-SZ-1", "verify-base-{number}"),
        ("hint-storno-SZ-1", "hint-storno-{number}"),
        ("delete-proforma-D-1", "delete-proforma-{number}"),
        ("set-payments-SZ-30", "set-payments-{number}"),
        ("taxpayer-12345678", "taxpayer-{prefix}"),
        ("proforma-for-delete", "proforma-for-delete"),
    ] {
        assert_eq!(run_pattern(name), pattern, "{name}");
    }
}

/// An observed sequence is explained by a path when it is a prefix of it — a
/// handler that answers early journals the first steps only; a renamed step,
/// an inserted one or one out of order is explained by none.
#[test]
fn an_observed_run_sequence_is_a_prefix_of_one_of_its_handlers_paths_or_unexplained() {
    let paths: &[&[&str]] = &[
        &[
            "namespace",
            "account",
            "verify-storno-{number}",
            "lookup-storno-{number}",
            "storno-{number}",
        ],
        &[
            "namespace",
            "account",
            "verify-storno-{number}",
            "hint-storno-{number}",
        ],
    ];
    let explained = |observed: &[&str]| {
        paths.iter().any(|path| {
            is_prefix_of_path(
                &observed
                    .iter()
                    .map(|name| run_pattern(name))
                    .collect::<Vec<_>>(),
                path,
            )
        })
    };
    assert!(
        explained(&[]),
        "nothing journaled (refused before the prologue)"
    );
    assert!(explained(&["namespace", "account"]), "unknown_account");
    assert!(
        explained(&["namespace", "account", "verify-storno-SZ-1"]),
        "not_managed"
    );
    assert!(explained(&[
        "namespace",
        "account",
        "verify-storno-SZ-1",
        "hint-storno-SZ-1"
    ]));
    assert!(explained(&[
        "namespace",
        "account",
        "verify-storno-SZ-1",
        "lookup-storno-SZ-1",
        "storno-SZ-1"
    ]));
    assert!(
        !explained(&["namespace", "account", "check-storno-SZ-1"]),
        "a renamed step"
    );
    assert!(
        !explained(&[
            "namespace",
            "account",
            "verify-storno-SZ-1",
            "lookup-storno-SZ-1",
            "confirm-SZ-1",
            "storno-SZ-1"
        ]),
        "an inserted step"
    );
    assert!(
        !explained(&["namespace", "account", "verify-storno-SZ-1", "storno-SZ-1"]),
        "a removed step"
    );
    assert!(!explained(&["account", "namespace"]), "out of order");
    assert!(
        !explained(&[
            "namespace",
            "account",
            "verify-storno-SZ-1",
            "lookup-storno-SZ-1",
            "storno-SZ-1",
            "storno-SZ-1"
        ]),
        "a step past the path's end"
    );
}
