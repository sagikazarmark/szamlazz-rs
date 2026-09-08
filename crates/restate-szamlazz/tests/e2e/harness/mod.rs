//! The harness the scenarios drive: the Restate server (through the
//! `restate-e2e-harness` crate), the wiremock standing in for szamlazz.hu,
//! and the ingress, SQL and stub helpers ([`Harness`]).
//!
//! What is szamlazz's stays here; what is Restate's is the crate's:
//!
//! - the server specs of the two suites ([`MAIN_SERVER`],
//!   [`WITHOUT_PROTOCOL_V7`]) over the crate's server gate, launcher and
//!   [`Restate`] handle (`restate_e2e_harness::{gate, server}`);
//! - [`accounts`]: the scripted and mutable resolver and store the two
//!   deployments run over, and the deployments themselves;
//! - [`szamlazz`]: what szamlazz.hu holds (the document fixture, the
//!   selector matchers and the response templates);
//! - [`ingress`]: an ingress reply with the worker's [`Fault`] decoded out
//!   of the crate's envelope check;
//! - [`run_names`]: the run-name table ([`run_names::RUN_NAMES`]), the
//!   *run-name pin*, over the crate's matcher.
//!
//! The harness's own tests (the fetch hold, the stub helpers against wiremock
//! alone) live beside what they test and need no server; the server gate's,
//! the sampler's and the matcher's are the crate's.
//!
//! [`Fault`]: restate_szamlazz::contract::Fault

pub(crate) mod accounts;
pub(crate) mod ingress;
pub(crate) mod run_names;
pub(crate) mod szamlazz;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use jiff::civil::date;
use restate_e2e_harness::gate::{FLAG_PROTOCOL_V7, FLAG_SCOPED_VIRTUAL_OBJECTS, FLAG_VQUEUES};
pub(crate) use restate_e2e_harness::gate::{Reuse, launcher_or_skip};
pub(crate) use restate_e2e_harness::plain_http;
use restate_e2e_harness::{Invocation, JournalEntry, Restate, ServerSpec, Target, Watch};
use restate_sdk::prelude::Endpoint;
use restate_szamlazz::contract::{BuyerInput, DocumentInput, LineItemInput, PaymentMethod};
use restate_szamlazz::{Agent, Order};
use rust_decimal::{Decimal, dec};
use serde_json::{Value, json};
use wiremock::{MockServer, ResponseTemplate};

use crate::harness::accounts::{
    MutableAccounts, ScriptedAccounts, multi_account_services, services,
};
use crate::harness::ingress::Reply;
use crate::harness::szamlazz::{
    Doc, Sends, create_lands_but_reply_lost, create_lands_on_the_second_send, create_lands_slowly,
    external_id_query, holds, holds_after_misses, loses_reply_once, not_found,
};

// ----- the servers ---------------------------------------------------------------

/// The main suite's server: the three experimental flags multi-account mode
/// depends on (vqueues, protocol v7 (below it the SDK sees no scope) and
/// scoped Virtual Objects). Set on the server the harness starts and expected
/// of one reused through the environment.
pub(crate) const MAIN_SERVER: ServerSpec = ServerSpec {
    name: "main",
    flags: &[FLAG_VQUEUES, FLAG_PROTOCOL_V7, FLAG_SCOPED_VIRTUAL_OBJECTS],
};

/// The protocol-v7 canary's server: vqueues and scoped Virtual Objects on,
/// protocol v7 off (a deployment that forgot the one flag the scope needs to
/// reach the SDK).
pub(crate) const WITHOUT_PROTOCOL_V7: ServerSpec = ServerSpec {
    name: "canary",
    flags: &[FLAG_VQUEUES, FLAG_SCOPED_VIRTUAL_OBJECTS],
};

/// The two Restate services of the worker, as the admin API names them.
const SERVICES: [&str; 2] = ["Szamlazz.Order", "Szamlazz.Agent"];

/// The `Szamlazz.Order` object `key`, in whatever scope: the suite's keys are
/// unique across the run, so no scenario watches one key under two scopes.
fn order(key: &str) -> Target<'_> {
    Target::object(SERVICES[0], key)
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
/// - [`Harness::create_lands_slowly`]: the same transition at the create's
///   receipt, but the create is answered `created` after a delay: the window
///   a second caller or a cancellation arrives in while the first send's
///   reply is in flight, opened to the scenario by the returned [`Sends`].
/// - [`Harness::create_lands_on_the_second_send`]: the first create answered
///   without landing (`szlahu_down`, a 500), the second landing: what a
///   create step meets when it re-executes after an *Unconfirmed* send.
/// - The raw builders (`number_query`, `order_query`, `external_id_query`,
///   `create`, `storno`), `expect(n)` and `up_to_n_times(n)`: a stub the
///   scenario asserts on (`expect`), a non-document answer (7, 500, an API
///   code) or an ordering-dependent shape stays explicit, byte for byte.
///
/// The five document helpers are checked against wiremock alone by the
/// non-ignored tests in [`szamlazz`].
pub(crate) struct Harness {
    restate: Restate,
    pub(crate) mock: MockServer,
    pub(crate) script: Arc<ScriptedAccounts>,
    /// The multi-account phase's resolver and store, once the flag day ran.
    multi: Option<Arc<MutableAccounts>>,
}

impl Harness {
    /// The harness on `restate` (launched and ready: its admin API answering
    /// and `/version` reporting its spec's features): serves and registers
    /// the single-account deployment.
    pub(crate) async fn start(restate: Restate) -> Self {
        let mock = MockServer::start().await;
        let (scripted, order, agent) = services(&mock.uri());
        let harness = Self {
            restate,
            mock,
            script: scripted,
            multi: None,
        };
        harness.deploy(order, agent).await;
        harness
    }

    /// Serves `order` and `agent` on a free port of this host and registers
    /// the deployment with the server: a new URI is a new revision of both
    /// services, and new invocations route to it.
    async fn deploy(&self, order: Order, agent: Agent) {
        self.restate
            .deploy(Endpoint::builder().bind(order).bind(agent).build())
            .await;
    }

    /// The single → multi flag day, as ADR 0006 and design §9 script it: make
    /// both services private, poll `sys_invocation` until nothing is in
    /// flight, register the multi-account deployment (the same namespace, the
    /// same szamlazz.hu account now under scope `acme` plus a second one under
    /// `beta`), make the services public again. Callers then use scoped paths.
    pub(crate) async fn switch_to_multi_account(&mut self) {
        self.set_public(false).await;
        self.restate.drain().await;
        let (mutable, order, agent) = multi_account_services(&self.mock.uri()).await;
        self.deploy(order, agent).await;
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
        for service in SERVICES {
            self.restate.set_public(service, public).await;
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

    /// `POST {ingress}{path}` through the crate's ingress, the reply with the
    /// worker's fault decodable ([`Reply::fault`]).
    pub(crate) async fn invoke(
        &self,
        path: &str,
        body: Option<&Value>,
        idempotency: Option<&str>,
    ) -> Reply {
        Reply(self.restate.invoke(path, body, idempotency).await)
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
        reply.0.body
    }

    /// `Szamlazz.Order.get`: no input, no idempotency key.
    pub(crate) async fn get(&self, key: &str) -> Value {
        let reply = self.get_reply(key).await;
        assert_eq!(reply.status, 200, "get on {key}: {}", reply.body);
        reply.0.body
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
        reply.0.body
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
        self.restate.admin().kill(invocation_id).await;
    }

    /// Cancels an invocation (`PATCH /invocations/{id}/cancel`): the
    /// cooperative stop. The server signals the running handler, whose next
    /// awaited step ends with the SDK's 409; the handler answers as it sees
    /// fit (a write step's 409 is `outcome_unknown`) and the invocation
    /// completes with that answer. A kill ends it without one.
    pub(crate) async fn cancel(&self, invocation_id: &str) {
        self.restate.admin().cancel(invocation_id).await;
    }

    /// The one invocation in flight on Virtual Object `key`: its id, from
    /// `sys_invocation`; panics on none or more than one. How a scenario
    /// names an invocation the ingress has not answered yet (`call` returns
    /// its id only with its answer): to cancel it, or to check that a retry
    /// attached to it.
    pub(crate) async fn in_flight_on(&self, key: &str) -> String {
        self.restate.admin().in_flight_on(&order(key)).await
    }

    /// Waits until `sys_invocation` holds `count` invocations in flight on
    /// Virtual Object `key` (accepted by the server, not completed): the
    /// server-side moment a call made while the key is held is queued behind
    /// it, which the ingress reports only with the call's answer. The ids.
    pub(crate) async fn await_in_flight_on(&self, key: &str, count: usize) -> Vec<String> {
        self.restate
            .admin()
            .await_in_flight_on(&order(key), count)
            .await
    }

    /// Waits until `sys_invocation` reports the invocation in one of
    /// `statuses`; the status it reached.
    pub(crate) async fn await_status(&self, invocation_id: &str, statuses: &[&str]) -> String {
        self.restate
            .admin()
            .await_status(invocation_id, statuses)
            .await
    }

    /// Runs a SQL query against the introspection API (`POST :9070/query`);
    /// a scenario's read, so an exchange without rows is a failure of the
    /// scenario (the sampler, [`Self::watch`], retries instead).
    pub(crate) async fn sql(&self, query: &str) -> Vec<Value> {
        self.restate.admin().sql_or_panic(query).await
    }

    /// The names of the `ctx.run` commands of an invocation, in journal
    /// order: which durable steps ran.
    pub(crate) async fn runs(&self, invocation_id: &str) -> Vec<String> {
        self.restate.admin().runs(invocation_id).await
    }

    /// The journal of an invocation, in index order.
    pub(crate) async fn journal(&self, invocation_id: &str) -> Vec<JournalEntry> {
        self.restate.admin().journal(invocation_id).await
    }

    /// The `sys_invocation` row of an invocation.
    pub(crate) async fn invocation(&self, invocation_id: &str) -> Invocation {
        self.restate.admin().invocation(invocation_id).await
    }

    /// Watches the invocations on Virtual Object `key` and records what
    /// `sys_invocation` reports **while they are in flight**: `retry_count`
    /// (the invoker's count of starts), `last_failure` and
    /// `last_failure_related_command_name` are in-flight columns, cleared once
    /// the invocation completes; a completed row shows neither the count nor
    /// the failing command (verified against 1.7.8). Start it before the
    /// call, [`finish`](Watch::finish) it after: the sampler ends as soon as
    /// it observes the invocation completed, and `finish` ends one whose call
    /// was answered between two samples.
    pub(crate) fn watch(&self, key: &str) -> Watch {
        Watch::start(self.restate.admin().clone(), &order(key))
    }

    /// Purges a completed invocation (`PATCH /invocations/{id}/purge`), so a
    /// later call runs against an order Restate has no memory of.
    pub(crate) async fn purge(&self, invocation_id: &str) {
        self.restate.admin().purge(invocation_id).await;
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

    /// Every journal entry of every invocation the server still holds, with
    /// `raw` hex-decoded, keyed by invocation id.
    pub(crate) async fn all_journals(&self) -> BTreeMap<String, Vec<JournalEntry>> {
        self.restate.admin().all_journals().await
    }

    /// Every `sys_invocation` row the server still holds.
    pub(crate) async fn all_invocations(&self) -> Vec<(String, Invocation)> {
        self.restate.admin().all_invocations().await
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

    /// The create lands at once but its reply takes `delay`, and `doc` is the
    /// holder of its external id from the request's receipt: see
    /// [`create_lands_slowly`]. The [`Sends`] signals the receipt.
    pub(crate) async fn create_lands_slowly(&self, doc: &Doc<'_>, delay: Duration) -> Sends {
        create_lands_slowly(&self.mock, doc, delay).await
    }

    /// The first create is answered `first` without landing, the second lands
    /// and `doc` is the holder from then on: see
    /// [`create_lands_on_the_second_send`]. The [`Sends`] counts both.
    pub(crate) async fn create_lands_on_the_second_send(
        &self,
        doc: &Doc<'_>,
        first: ResponseTemplate,
    ) -> Sends {
        create_lands_on_the_second_send(&self.mock, doc, first).await
    }
}
