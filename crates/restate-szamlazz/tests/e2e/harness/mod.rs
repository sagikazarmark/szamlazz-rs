//! The harness the scenarios drive: the Restate server (through the
//! `restate-e2e-harness` crate), the wiremock standing in for szamlazz.hu,
//! and the ingress under the worker's service names ([`Harness`]).
//!
//! What is szamlazz's stays here; what is Restate's is the crate's:
//!
//! - the server specs of the two suites ([`MAIN_SERVER`],
//!   [`WITHOUT_PROTOCOL_V7`]) over the crate's server gate, launcher and
//!   [`Restate`] handle (`restate_e2e_harness::{gate, server}`);
//! - [`accounts`]: the resolver and store the two deployments run over, and
//!   the deployments themselves;
//! - [`szamlazz`]: what szamlazz.hu holds (the shared document fixture and
//!   matchers of `tests/common`, and the document-centric mount helpers);
//! - [`ingress`]: an ingress reply with the worker's [`Fault`] decoded out
//!   of the crate's envelope check;
//! - [`run_names`]: the *step-name table* ([`run_names::RUN_NAMES`], held as
//!   the crate's [`Table`](restate_e2e_harness::Table)).
//!
//! The harness's own tests (the fetch hold and the resolution script, the
//! stub helpers against wiremock alone) live beside what they test and need
//! no server; the server gate's, the sampler's and the table check's are the
//! crate's.
//!
//! [`Fault`]: restate_szamlazz::contract::Fault

pub(crate) mod accounts;
pub(crate) mod ingress;
pub(crate) mod run_names;
pub(crate) mod szamlazz;

use std::sync::Arc;

use jiff::civil::date;
use restate_e2e_harness::gate::{PROTOCOL_V7, SCOPED_VIRTUAL_OBJECTS, VQUEUES};
pub(crate) use restate_e2e_harness::gate::{ReusePolicy, launcher_or_skip};
use restate_e2e_harness::{Admin, Call, Restate, ServerSpec, Target, Watch};
use restate_sdk::prelude::Endpoint;
use restate_szamlazz::contract::{BuyerInput, DocumentInput, LineItemInput, PaymentMethod};
use restate_szamlazz::{Agent, Order};
use rust_decimal::{Decimal, dec};
use serde_json::{Value, json};
use wiremock::MockServer;

use crate::harness::accounts::{MutableAccounts, multi_account_services, services};
use crate::harness::ingress::Reply;
use crate::harness::szamlazz::{external_id_query, not_found};

// ----- the servers ---------------------------------------------------------------

/// The main suite's server: the three experimental features multi-account
/// mode depends on (vqueues, protocol v7 (below it the SDK sees no scope) and
/// scoped Virtual Objects), on. Set on the server the harness starts and
/// expected of one reused through the environment.
pub(crate) const MAIN_SERVER: ServerSpec = ServerSpec {
    name: "main",
    features: &[
        (VQUEUES, true),
        (PROTOCOL_V7, true),
        (SCOPED_VIRTUAL_OBJECTS, true),
    ],
    env: &[],
};

/// The protocol-v7 canary's server: vqueues and scoped Virtual Objects on,
/// protocol v7 **off**, explicitly (a deployment that forgot the one feature
/// the scope needs to reach the SDK); `/version` is checked to report it off.
pub(crate) const WITHOUT_PROTOCOL_V7: ServerSpec = ServerSpec {
    name: "canary",
    features: &[
        (VQUEUES, true),
        (PROTOCOL_V7, false),
        (SCOPED_VIRTUAL_OBJECTS, true),
    ],
    env: &[],
};

/// The two Restate services of the worker, as the admin API names them.
pub(crate) const SERVICES: [&str; 2] = ["Szamlazz.Order", "Szamlazz.Agent"];

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

/// The Restate server, the wiremock standing in for szamlazz.hu, and what the
/// scenarios drive them through: the ingress under the worker's two service
/// names ([`Harness::call`] and its scoped, agent, `get` and `send` forms), the
/// admin API as it is ([`Harness::admin`]) plus the order-key naming over it
/// ([`Harness::in_flight_on`], [`Harness::watch`]), the mock as it is
/// (`mock`) plus what a scenario counts on it per order or per number.
///
/// A scenario states what szamlazz.hu holds through the document-centric
/// helpers of [`szamlazz`] on `h.mock` (one call per document, the same body
/// on every selector the document is reachable by, so the stubs cannot
/// disagree), and through the raw selector builders where it is about a
/// specific wire sequence:
///
/// - [`Harness::absent`]: code 7 on the external ids of `kinds` under `order`
///   (the one document helper on the harness, since it composes the external
///   ids of the namespace).
/// - [`szamlazz::holds`]: `doc` on `number_query`, on `order_query` when it
///   carries an order, on `external_id_query` when it states an external id.
///   When two held documents carry one order, the one held first answers the
///   order query (wiremock answers with the first mounted match): a scenario
///   whose order's newest document is not the one it holds keeps the raw
///   `order_query` builder.
/// - [`szamlazz::holds_after_misses`]: the external-id selector alone, code 7
///   for `misses` queries, then `doc`, the document appearing after a
///   hand-counted number of queries (the lookup step's, the create step's
///   leading query and re-query).
/// - [`szamlazz::create_lands_slowly`]: the order's create is answered
///   `created` after a delay, and `doc` holds its external id from the moment
///   the create request is received (code 7 before): the transition is the
///   create stub being matched, not a query count, and the delay is the
///   window a second caller or a cancellation arrives in while the first
///   send's reply is in flight, opened to the scenario by the returned
///   [`szamlazz::Sends`]. (Its sibling `create_lands_but_reply_lost`, the 500
///   instead of the delayed answer, is the helper module's; no scenario needs
///   it, the gateway's table covers the immediate re-query.)
/// - [`szamlazz::create_lands_on_the_second_send`]: the first create answered
///   without landing (`szlahu_down`, a 500), the second landing: what a
///   create step meets when it re-executes after an *Unconfirmed* send.
/// - The raw builders (`number_query`, `order_query`, `external_id_query`,
///   `create_for`, `storno_of_number`), `expect(n)` and `up_to_n_times(n)`: a
///   stub the scenario asserts on (`expect`), a non-document answer (7, 500,
///   an API code) or an ordering-dependent shape stays explicit, byte for
///   byte.
///
/// **Every stub is discriminated by what it is about** (the order key on a
/// create, the number on a storno, a credit entry or a delete, the external id
/// or the order on a query), never by its place in time: phase 1 mounts once
/// and runs its scenarios concurrently, every scenario owns its order keys and
/// numbers, and nothing is reset between them; what a scenario counts it
/// counts **per order or per number** ([`Harness::create_bodies_of`] and its
/// siblings, [`Harness::requests_mentioning`]), never over the whole mock.
/// Every mock's `expect(n)` is verified at the first [`Harness::reset`] after
/// it (phase 2's scenarios run in sequence and reset between them; the last
/// one's mocks are checked when the harness is dropped). The document helpers
/// are checked against wiremock alone by the non-ignored tests in
/// [`szamlazz`].
pub(crate) struct Harness {
    restate: Restate,
    pub(crate) mock: MockServer,
    /// The multi-account phase's resolver and store, once the flag day ran.
    multi: Option<Arc<MutableAccounts>>,
}

impl Harness {
    /// The harness on `restate` (launched and ready: its admin API answering
    /// and `/version` reporting its spec's features): serves and registers
    /// the single-account deployment.
    ///
    /// **One run per server.** The suite's `Idempotency-Key`s and order keys
    /// are literals, the run-wide checks count over every invocation the
    /// server holds, and a scenario asserts on the invocations of its order
    /// (`concurrency`: one on `E2E-L3`), so a server that already holds an
    /// earlier run's `Szamlazz.*` invocations is refused here, before
    /// anything is deployed, rather than met as a failed `expect(1)` or a
    /// replayed completion halfway through. A spawned server is fresh by
    /// construction; a reused one (`RESTATE_ADMIN_URL`) is fresh once per
    /// `docker compose down -v && docker compose up -d`.
    pub(crate) async fn start(restate: Restate) -> Self {
        let earlier: Vec<String> = restate
            .admin()
            .all_invocations()
            .await
            .into_iter()
            .filter(|(_, invocation)| SERVICES.contains(&invocation.service.as_str()))
            .map(|(id, invocation)| format!("{id} {}.{}", invocation.service, invocation.handler))
            .collect();
        assert!(
            earlier.is_empty(),
            "the Restate server at {} already holds {} invocation(s) of {} from an earlier run; the \
             suite is one run per server (its keys are literals and its checks count over every \
             invocation the server holds). Reuse a fresh one: `docker compose down -v && docker \
             compose up -d`, or unset RESTATE_ADMIN_URL / RESTATE_INGRESS_URL and let \
             RESTATE_SERVER_BIN spawn one. The first few: {:?}",
            restate.admin_url(),
            earlier.len(),
            SERVICES.join(" / "),
            &earlier[..earlier.len().min(5)]
        );
        let mock = MockServer::start().await;
        let (order, agent) = services(&mock.uri());
        let harness = Self {
            restate,
            mock,
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
    /// This suite has ingress-only producers and no pending delayed sends;
    /// private services would still accept internal SDK calls.
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

    /// The Restate admin API of the server: SQL introspection, journals,
    /// `sys_invocation` rows, kill / cancel / purge, `await_status`, the
    /// registered handlers. A scenario reads and operates on invocations
    /// through it directly; what the harness adds on top is the key → object
    /// naming ([`Self::in_flight_on`], [`Self::await_in_flight_on`],
    /// [`Self::watch`]).
    pub(crate) fn admin(&self) -> &Admin {
        self.restate.admin()
    }

    /// `Szamlazz.Order.{handler}` on `key`, unscoped, waited for; the
    /// scenario scopes or sends it ([`Call::scoped`], [`Call::send`]).
    fn order_call<'a>(key: &'a str, handler: &'a str) -> Call<'a> {
        Call::object(SERVICES[0], key, handler)
    }

    /// `Szamlazz.Agent.{handler}`, unscoped, waited for.
    fn agent_call(handler: &str) -> Call<'_> {
        Call::service(SERVICES[1], handler)
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
            &Self::order_call(key, handler),
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
            &Self::order_call(key, handler).scoped(scope),
            Some(body),
            Some(idempotency),
        )
        .await
    }

    /// Calls `Szamlazz.Agent.{handler}` unscoped.
    pub(crate) async fn call_agent(&self, handler: &str, body: &Value) -> Reply {
        self.invoke(&Self::agent_call(handler), Some(body), None)
            .await
    }

    /// Calls `Szamlazz.Agent.{handler}` under `scope`.
    pub(crate) async fn call_agent_scoped(
        &self,
        scope: &str,
        handler: &str,
        body: &Value,
    ) -> Reply {
        self.invoke(&Self::agent_call(handler).scoped(scope), Some(body), None)
            .await
    }

    /// `Szamlazz.Agent.check_account`: no input, no idempotency key; unscoped
    /// or under `scope`.
    pub(crate) async fn check_account(&self, scope: Option<&str>) -> Reply {
        let mut call = Self::agent_call("check_account");
        if let Some(scope) = scope {
            call = call.scoped(scope);
        }
        self.invoke(&call, None, None).await
    }

    /// `call` through the crate's ingress, the reply with the worker's fault
    /// decodable ([`Reply::fault`]).
    pub(crate) async fn invoke(
        &self,
        call: &Call<'_>,
        body: Option<&Value>,
        idempotency: Option<&str>,
    ) -> Reply {
        Reply(self.restate.invoke(call, body, idempotency).await)
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
    pub(crate) async fn get_reply(&self, key: &str) -> Reply {
        self.invoke(&Self::order_call(key, "get"), None, None).await
    }

    /// `Szamlazz.Order.get` under `scope`.
    pub(crate) async fn get_scoped(&self, scope: &str, key: &str) -> Value {
        let reply = self
            .invoke(&Self::order_call(key, "get").scoped(scope), None, None)
            .await;
        assert_eq!(
            reply.status, 200,
            "get on {key} under {scope}: {}",
            reply.body
        );
        reply.0.body
    }

    /// Submits `Szamlazz.Order.{handler}` on `key` under `scope` without
    /// waiting for it (`/restate/send/…`): the invocation id of the accepted
    /// invocation. ("Send" alone is a szamlazz.hu request in this suite.)
    pub(crate) async fn submit_scoped(
        &self,
        scope: &str,
        key: &str,
        handler: &str,
        body: &Value,
    ) -> String {
        let reply = self
            .invoke(
                &Self::order_call(key, handler).scoped(scope).send(),
                Some(body),
                None,
            )
            .await;
        assert_eq!(reply.status, 202, "send {handler} on {key}: {}", reply.body);
        reply.invocation_id().to_owned()
    }

    /// The one invocation in flight on Virtual Object `key`: its id, from
    /// `sys_invocation`; panics on none or more than one. How a scenario
    /// names an invocation the ingress has not answered yet (`call` returns
    /// its id only with its answer): to cancel it, or to check that a retry
    /// attached to it.
    pub(crate) async fn in_flight_on(&self, key: &str) -> String {
        self.admin().in_flight_on(&order(key)).await
    }

    /// Waits until `sys_invocation` holds `count` invocations in flight on
    /// Virtual Object `key` (accepted by the server, not completed): the
    /// server-side moment a call made while the key is held is queued behind
    /// it, which the ingress reports only with the call's answer. The ids.
    pub(crate) async fn await_in_flight_on(&self, key: &str, count: usize) -> Vec<String> {
        self.admin().await_in_flight_on(&order(key), count).await
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
        Watch::start(self.admin().clone(), &order(key))
    }

    /// Verifies every mounted mock's `expect(n)` (wiremock checks them on
    /// `verify` and on drop, never on `reset`), then forgets every mock and
    /// every recorded request. A mock that saw more or fewer requests than it
    /// expected fails here, naming the mock and the requests received: at
    /// the end of phase 1 for every mock its concurrent scenarios mounted, and
    /// at the start of each phase-2 scenario for the previous one's; the last
    /// scenario's mocks are checked when the harness is dropped.
    pub(crate) async fn reset(&self) {
        self.mock.verify().await;
        self.mock.reset().await;
    }

    /// The bodies of the requests szamlazz.hu has seen so far that mention
    /// `needle`, a **delimited** marker (`<torzsszam>12345678</torzsszam>`,
    /// `acct:check-account`, `acct:E2E-K:proforma`): how a scenario says
    /// "nothing of mine reached szamlazz.hu" beside scenarios whose requests
    /// it does not count. A bare order key is not a marker: `E2E-1` is a
    /// prefix of `E2E-11`; an order's requests are
    /// [`Self::requests_of_order`].
    pub(crate) async fn requests_mentioning(&self, needle: &str) -> Vec<String> {
        self.bodies(None, needle).await
    }

    /// The bodies of the requests of `order` szamlazz.hu has seen so far: the
    /// creates and the order-number queries (`<rendelesSzam>{order}</rendelesSzam>`)
    /// and the external-id queries of its documents (`acct:{order}:…`), each
    /// matched with its delimiter, so an order whose key is a prefix of
    /// another's (`E2E-1`, `E2E-11`) counts only its own. A query by number
    /// names no order and is not counted.
    pub(crate) async fn requests_of_order(&self, order: &str) -> Vec<String> {
        let by_order = format!("<rendelesSzam>{order}</rendelesSzam>");
        let by_external_id = format!("<szamlaKulsoAzon>acct:{order}:");
        self.mock
            .received_requests()
            .await
            .expect("requests")
            .iter()
            .map(|request| String::from_utf8_lossy(&request.body).into_owned())
            .filter(|body| body.contains(&by_order) || body.contains(&by_external_id))
            .collect()
    }

    /// The bodies of the create requests of `order` szamlazz.hu has seen so
    /// far.
    pub(crate) async fn create_bodies_of(&self, order: &str) -> Vec<String> {
        self.bodies(
            Some("action-xmlagentxmlfile"),
            &format!("<rendelesSzam>{order}</rendelesSzam>"),
        )
        .await
    }

    /// The bodies of the storno requests of `number` szamlazz.hu has seen so
    /// far.
    pub(crate) async fn storno_bodies_of(&self, number: &str) -> Vec<String> {
        self.bodies(
            Some("action-szamla_agent_st"),
            &format!("<szamlaszam>{number}</szamlaszam>"),
        )
        .await
    }

    /// The bodies of the proforma deletions of `number` szamlazz.hu has seen
    /// so far.
    pub(crate) async fn delete_bodies_of(&self, number: &str) -> Vec<String> {
        self.bodies(
            Some("action-szamla_agent_dijbekero_torlese"),
            &format!("<szamlaszam>{number}</szamlaszam>"),
        )
        .await
    }

    /// The bodies of the requests of `action` (every action when `None`)
    /// szamlazz.hu has seen so far that mention `needle`.
    async fn bodies(&self, action: Option<&str>, needle: &str) -> Vec<String> {
        let marker = action.map(|action| format!("name=\"{action}\""));
        self.mock
            .received_requests()
            .await
            .expect("requests")
            .iter()
            .map(|request| String::from_utf8_lossy(&request.body).into_owned())
            .filter(|body| {
                marker.as_ref().is_none_or(|marker| body.contains(marker)) && body.contains(needle)
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
}
