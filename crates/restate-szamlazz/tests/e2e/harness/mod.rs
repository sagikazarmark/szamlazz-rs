//! The harness the scenarios drive: the Restate server, the wiremock standing
//! in for szamlazz.hu, and the ingress, SQL and stub helpers ([`Harness`]).
//!
//! - [`gate`]: where the server comes from (the *server gate*), the launcher
//!   and the server process or container.
//! - [`accounts`]: the scripted and mutable resolver and store the two
//!   deployments run over, and the deployments themselves.
//! - [`szamlazz`]: what szamlazz.hu holds (the document fixture, the
//!   selector matchers and the response templates).
//! - [`ingress`]: an ingress reply and the fault inside its error envelope.
//! - [`introspection`]: `sys_journal` and `sys_invocation` rows.
//! - [`run_names`]: the run-name table ([`run_names::RUN_NAMES`]) and its
//!   matching, the *run-name pin*.
//!
//! The harness's own tests (the server gate, the run-pattern matching, the
//! stub helpers against wiremock alone) live beside what they test and need
//! no server.

pub(crate) mod accounts;
pub(crate) mod gate;
mod ingress;
pub(crate) mod introspection;
pub(crate) mod run_names;
pub(crate) mod szamlazz;

use std::collections::BTreeMap;
use std::net::TcpListener;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jiff::civil::date;
use restate_sdk::prelude::{Endpoint, HttpServer};
use restate_szamlazz::contract::{BuyerInput, DocumentInput, LineItemInput, PaymentMethod};
use restate_szamlazz::{Agent, Order};
use rust_decimal::{Decimal, dec};
use serde_json::{Value, json};
use wiremock::MockServer;

use crate::harness::accounts::{
    MutableAccounts, ScriptedAccounts, multi_account_services, services,
};
use crate::harness::gate::{FEATURES, Restate};
use crate::harness::ingress::Reply;
use crate::harness::introspection::{Invocation, JournalEntry, Retries};
use crate::harness::szamlazz::{
    Doc, create_lands_but_reply_lost, external_id_query, holds, holds_after_misses,
    loses_reply_once, not_found,
};

// ----- the harness's own HTTP client -----------------------------------------------

/// A [`reqwest::ClientBuilder`] for the harness's own traffic, all of it plain
/// `http://` on the loopback (the Restate admin and ingress APIs, a raw post at
/// the wiremock): **no root certificates**, so building it never parses the
/// system CA store (#136). The gateways the deployment's prologue opens are
/// the production `Gateway::open`, untouched.
pub(crate) fn plain_http() -> reqwest::ClientBuilder {
    reqwest::Client::builder().tls_certs_only(std::iter::empty())
}

// ----- request bodies ----------------------------------------------------------

pub(crate) fn document(unit_price: Decimal) -> DocumentInput {
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

pub(crate) fn create_body(unit_price: Decimal, reissue: bool) -> Value {
    json!({
        "document": document(unit_price),
        "options": { "reissue": reissue },
    })
}

// ----- the harness -----------------------------------------------------------

/// The Restate server, the wiremock standing in for szamlazz.hu, and the
/// ingress, SQL and stub helpers the scenarios drive them through.
///
/// A scenario states what szamlazz.hu holds through the document-centric
/// helpers (one call per document, the same body on every selector the
/// document is reachable by, so the stubs cannot disagree), and through the
/// raw selector builders where it is about a specific wire sequence:
///
/// - [`Harness::absent`]: code 7 on the external ids of `kinds` under `order`.
/// - [`Harness::holds`]: `doc` on `number_query`, on `order_query` when it
///   carries an order, on `external_id_query` when it states an external id.
///   When two held documents carry one order, the one held first answers the
///   order query (wiremock answers with the first mounted match): a scenario
///   whose order's newest document is not the one it holds keeps the raw
///   `order_query` builder.
/// - [`Harness::holds_after_misses`]: the external-id selector alone, code 7
///   for `misses` queries, then `doc`, the document appearing after a
///   hand-counted number of queries (the lookup step's, the create step's
///   leading query and re-query).
/// - [`Harness::create_lands_but_reply_lost`]: `create()` answers 500,
///   `expect(1)`, and `doc` holds its external id from the moment the create
///   request is received; the transition is the create stub being matched,
///   not a query count. Code 7 on the external id before.
/// - The raw builders (`number_query`, `order_query`, `external_id_query`,
///   `create`, `storno`), `expect(n)` and `up_to_n_times(n)`: a stub the
///   scenario asserts on (`expect`), a non-document answer (7, 500, an API
///   code) or an ordering-dependent shape stays explicit, byte for byte.
///
/// The three document helpers are checked against wiremock alone by the
/// non-ignored tests in [`szamlazz`].
pub(crate) struct Harness {
    restate: Restate,
    pub(crate) mock: MockServer,
    http: reqwest::Client,
    pub(crate) script: Arc<ScriptedAccounts>,
    /// The multi-account phase's resolver and store, once the flag day ran.
    multi: Option<Arc<MutableAccounts>>,
}

impl Harness {
    /// The harness on `restate`: waits for its admin API, checks that
    /// `/version` reports exactly the features the server's flags enable, and
    /// serves and registers the single-account deployment.
    pub(crate) async fn start(restate: Restate) -> Self {
        let mock = MockServer::start().await;
        let http = plain_http()
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
    pub(crate) async fn switch_to_multi_account(&mut self) {
        self.set_public(false).await;
        self.drain().await;
        let (mutable, order, agent) = multi_account_services(&self.mock.uri()).await;
        self.serve_and_register(order, agent).await;
        self.multi = Some(mutable);
        self.set_public(true).await;
    }

    /// The multi-account phase's resolver and store.
    pub(crate) fn multi(&self) -> &MutableAccounts {
        self.multi
            .as_deref()
            .expect("the flag day has run (switch_to_multi_account)")
    }

    /// `PATCH /services/{service}` with `public` for both services.
    pub(crate) async fn set_public(&self, public: bool) {
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
    pub(crate) async fn call(
        &self,
        key: &str,
        handler: &str,
        body: &Value,
        idempotency: &str,
    ) -> Reply {
        self.invoke(
            &format!("/restate/call/Szamlazz.Order/{key}/{handler}"),
            Some(body),
            Some(idempotency),
        )
        .await
    }

    /// Calls `Szamlazz.Order.{handler}` on `key` under `scope`
    /// (`/restate/scope/{scope}/call/…`).
    pub(crate) async fn call_scoped(
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
    pub(crate) async fn call_agent_scoped(
        &self,
        scope: &str,
        handler: &str,
        body: &Value,
    ) -> Reply {
        self.invoke(
            &format!("/restate/scope/{scope}/call/Szamlazz.Agent/{handler}"),
            Some(body),
            None,
        )
        .await
    }

    /// `Szamlazz.Agent.check_account`: no input, no idempotency key; unscoped
    /// or under `scope`.
    pub(crate) async fn check_account(&self, scope: Option<&str>) -> Reply {
        let path = match scope {
            Some(scope) => format!("/restate/scope/{scope}/call/Szamlazz.Agent/check_account"),
            None => "/restate/call/Szamlazz.Agent/check_account".to_owned(),
        };
        self.invoke(&path, None, None).await
    }

    pub(crate) async fn invoke(
        &self,
        path: &str,
        body: Option<&Value>,
        idempotency: Option<&str>,
    ) -> Reply {
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

    pub(crate) async fn ok(
        &self,
        key: &str,
        handler: &str,
        body: &Value,
        idempotency: &str,
    ) -> Value {
        let reply = self.call(key, handler, body, idempotency).await;
        assert_eq!(reply.status, 200, "{handler} on {key}: {}", reply.body);
        reply.body
    }

    /// `Szamlazz.Order.get`: no input, no idempotency key.
    pub(crate) async fn get(&self, key: &str) -> Value {
        let reply = self.get_reply(key).await;
        assert_eq!(reply.status, 200, "get on {key}: {}", reply.body);
        reply.body
    }

    /// `Szamlazz.Order.get` as the raw reply, for the invocation id.
    pub(crate) async fn get_reply(&self, key: &str) -> Reply {
        self.invoke(
            &format!("/restate/call/Szamlazz.Order/{key}/get"),
            None,
            None,
        )
        .await
    }

    /// `Szamlazz.Order.get` under `scope`.
    pub(crate) async fn get_scoped(&self, scope: &str, key: &str) -> Value {
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

    /// Submits `Szamlazz.Order.{handler}` on `key` without waiting for it
    /// (`/restate/send/…`): the invocation id of the accepted invocation.
    /// ("Send" alone is a szamlazz.hu request in this suite.)
    pub(crate) async fn submit(&self, key: &str, handler: &str, body: &Value) -> String {
        let reply = self
            .invoke(
                &format!("/restate/send/Szamlazz.Order/{key}/{handler}"),
                Some(body),
                None,
            )
            .await;
        assert_eq!(reply.status, 202, "send {handler} on {key}: {}", reply.body);
        reply.invocation_id().to_owned()
    }

    /// Kills an invocation (`PATCH /invocations/{id}/kill`): what an operator
    /// does to one that will not finish, and what `on_max_attempts = kill`
    /// does after the handler's attempts are spent.
    pub(crate) async fn kill(&self, invocation_id: &str) {
        self.patch_invocation(invocation_id, "kill").await;
    }

    /// `PATCH /invocations/{id}/{action}` on the admin API, asserting success.
    async fn patch_invocation(&self, invocation_id: &str, action: &str) {
        let response = self
            .http
            .patch(format!(
                "{}/invocations/{invocation_id}/{action}",
                self.restate.admin
            ))
            .send()
            .await
            .expect(action);
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        assert!(
            (200..300).contains(&status),
            "{action} of {invocation_id} failed ({status}): {body}"
        );
    }

    /// Waits until `sys_invocation` reports the invocation in one of
    /// `statuses`; the status it reached.
    pub(crate) async fn await_status(&self, invocation_id: &str, statuses: &[&str]) -> String {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let rows = self
                .sql(&format!(
                    "SELECT status FROM sys_invocation WHERE id = '{invocation_id}'"
                ))
                .await;
            let status = rows
                .first()
                .and_then(|row| row["status"].as_str())
                .unwrap_or_default()
                .to_owned();
            if statuses.contains(&status.as_str()) {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "invocation {invocation_id} is {status:?}, not one of {statuses:?}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Runs a SQL query against the introspection API (`POST :9070/query`).
    pub(crate) async fn sql(&self, query: &str) -> Vec<Value> {
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
    pub(crate) async fn runs(&self, invocation_id: &str) -> Vec<String> {
        self.journal(invocation_id)
            .await
            .into_iter()
            .filter(JournalEntry::is_run)
            .filter_map(|entry| entry.name)
            .collect()
    }

    /// The journal of an invocation, in index order.
    pub(crate) async fn journal(&self, invocation_id: &str) -> Vec<JournalEntry> {
        let rows = self
            .sql(&format!(
                "SELECT index, entry_type, name, raw FROM sys_journal WHERE id = '{invocation_id}' ORDER BY index"
            ))
            .await;
        rows.iter().map(JournalEntry::from_row).collect()
    }

    /// The `sys_invocation` row of an invocation.
    pub(crate) async fn invocation(&self, invocation_id: &str) -> Invocation {
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

    /// [`watch_for`](Self::watch_for) over four seconds, enough for the one
    /// or two run retries a scenario provokes under the 1 s test policies.
    pub(crate) fn watch(&self, key: &str) -> tokio::task::JoinHandle<Retries> {
        self.watch_for(key, Duration::from_secs(4))
    }

    /// Watches the invocations on Virtual Object `key` for `window` (a
    /// detached task polling every [`Self::WATCH_POLL`], so it carries its own
    /// copy of the query; `sql` borrows the harness) and records what
    /// `sys_invocation` reports **while they are in flight**: `retry_count`
    /// (the invoker's count of starts), `last_failure` and
    /// `last_failure_related_command_name` are attempt state, cleared once the
    /// invocation completes; a completed row shows neither the count nor the
    /// failing command (verified against 1.7.8). Start it before the call,
    /// await it after.
    pub(crate) fn watch_for(
        &self,
        key: &str,
        window: Duration,
    ) -> tokio::task::JoinHandle<Retries> {
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
    pub(crate) async fn purge(&self, invocation_id: &str) {
        self.patch_invocation(invocation_id, "purge").await;
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

    /// Verifies the previous scenario's `expect(n)` counts (wiremock checks
    /// them on `verify` and on drop, never on `reset`), then forgets every
    /// mock and every recorded request. A mock that saw more or fewer
    /// requests than it expected fails here, at the start of the next
    /// scenario, naming the mock and the requests received; the last
    /// scenario's mocks are checked when the harness is dropped.
    pub(crate) async fn reset(&self) {
        self.mock.verify().await;
        self.mock.reset().await;
    }

    pub(crate) async fn requests_seen(&self) -> usize {
        self.mock.received_requests().await.expect("requests").len()
    }

    /// The bodies of the create requests szamlazz.hu has seen so far.
    pub(crate) async fn create_bodies(&self) -> Vec<String> {
        self.bodies_of("action-xmlagentxmlfile").await
    }

    /// The bodies of the storno requests szamlazz.hu has seen so far.
    pub(crate) async fn storno_bodies(&self) -> Vec<String> {
        self.bodies_of("action-szamla_agent_st").await
    }

    /// The bodies of the proforma deletions szamlazz.hu has seen so far.
    pub(crate) async fn delete_bodies(&self) -> Vec<String> {
        self.bodies_of("action-szamla_agent_dijbekero_torlese")
            .await
    }

    /// The bodies of the credit-entry requests szamlazz.hu has seen so far.
    pub(crate) async fn credit_bodies(&self) -> Vec<String> {
        self.bodies_of("action-szamla_agent_kifiz").await
    }

    /// The bodies of the requests of `action` szamlazz.hu has seen so far.
    pub(crate) async fn bodies_of(&self, action: &str) -> Vec<String> {
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
    pub(crate) async fn wait_for_creates(&self, count: usize) {
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
    pub(crate) async fn all_journals(&self) -> BTreeMap<String, Vec<JournalEntry>> {
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
    pub(crate) async fn all_invocations(&self) -> Vec<(String, Invocation)> {
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
    pub(crate) async fn absent(&self, order: &str, kinds: &[&str]) {
        for kind in kinds {
            external_id_query(&format!("acct:{order}:{kind}"))
                .respond_with(not_found())
                .mount(&self.mock)
                .await;
        }
    }

    /// szamlazz.hu holds `doc`: see [`holds`].
    pub(crate) async fn holds(&self, doc: &Doc<'_>) {
        holds(&self.mock, doc).await;
    }

    /// szamlazz.hu holds `doc` under its external id after `misses` code-7
    /// answers: see [`holds_after_misses`].
    pub(crate) async fn holds_after_misses(&self, misses: u64, doc: &Doc<'_>) {
        holds_after_misses(&self.mock, misses, doc).await;
    }

    /// The next external-id query for `id` loses its reply once: see
    /// [`loses_reply_once`]. Mount before the steady answers.
    pub(crate) async fn loses_reply_once(&self, id: &str) {
        loses_reply_once(&self.mock, id).await;
    }

    /// The create lands but its reply is lost, and `doc` is the holder of its
    /// external id from that moment on: see [`create_lands_but_reply_lost`].
    pub(crate) async fn create_lands_but_reply_lost(&self, doc: &Doc<'_>) {
        create_lands_but_reply_lost(&self.mock, doc).await;
    }
}
