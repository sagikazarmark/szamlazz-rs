//! Every handler of both services driven **offline**: the real handler
//! bodies (`entry`, through `Order::over` / `Agent::over`, the very fns the
//! `#[restate_sdk]` handlers are one line over) run against a wiremock
//! szamlazz.hu over a [`FakeRunner`] in place of Restate, with the prologue,
//! the durable steps and the gateway all real, and the gateway opened over a
//! client that loads no root certificates (#136).
//!
//! What this suite is about is **sequence and mapping**, not truth tables
//! (those are the pure decisions' unit tests, #124): which read follows
//! which, that an early answer stops before the next step, what a step's
//! exhaustion becomes (`unavailable` / `outcome_unknown`, about the document
//! the step knew), that a cancellation propagates through the best-effort
//! reads, how a replayed entry is read, and, on every path, the **offline
//! run-name pin**: the names the fake journaled are a prefix of one of the
//! handler's paths in [`RUN_NAMES`], and every path is walked in full by one
//! scenario of the suite ([`every_pinned_path_is_walked_in_full_offline`]),
//! so a renamed, reordered or dropped step fails `cargo test` without a
//! server; the e2e pin against a live `sys_journal` stays the authority.
//!
//! Each scenario is one `#[tokio::test]` of its own (`each`) and one row of
//! the table the coverage test walks ([`SCENARIOS`]).

use std::collections::BTreeSet;
use std::time::Duration;

use bytes::Bytes;
use restate_sdk::errors::HandlerError;
use restate_sdk::serde::Deserialize as _;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use wiremock::MockServer;

use super::entry::{AgentHandlers, OrderHandlers};
use super::prologue::Opener;
use super::run_names::{RUN_NAMES, is_prefix_of_path, run_pattern};
use super::runner::{BoxFuture, FakeRunner, Recorded, RunRecord};
use super::{Agent, Body, Order};
use crate::account::{Accounts, StaticConfig, StaticResolver};
use crate::config::{Namespace, StepPolicy, WorkerConfig};
use crate::contract::document::tests::sample_document;
use crate::gateway::Gateway;
use crate::test_support::stubs::{
    absent, create, created, credit, credited, delete_of, external_id_query, holds, not_found,
    number_query, order_query, proforma_deleted, storno, taxpayer_known, taxpayer_query,
};
use crate::test_support::{Doc, ORIGINAL_TELJ, http_client};

const KEY: &str = "fake-agent-key-0f1e2d";
const ORDER: &str = "ORD-1";

fn namespace() -> Namespace {
    "acct".parse().expect("namespace")
}

// ----- the deployment ------------------------------------------------------------

/// A single-account deployment over a wiremock szamlazz.hu: both services,
/// the default policies (the fake's clock is simulated, so the real delays
/// cost nothing and are what the records show), and every execution's
/// gateway opened over [`http_client`].
struct Offline {
    mock: MockServer,
    order: Order,
    agent: Agent,
}

impl Offline {
    async fn start() -> Self {
        Self::start_as("acct").await
    }

    /// The deployment whose one account has `id` (and, through the static
    /// resolver, the credential reference `id`).
    async fn start_as(id: &str) -> Self {
        let mock = MockServer::start().await;
        let config: StaticConfig = serde_json::from_value(json!({
            "account": { "id": id, "agent_key": KEY, "endpoint": mock.uri() },
        }))
        .expect("config");
        let accounts = Accounts::from(StaticResolver::try_from(config).expect("resolver"));
        let opener = Opener::new(|account, credentials| {
            Gateway::open_with_http(account, credentials, http_client())
        });
        let worker = WorkerConfig::new(namespace());
        Self {
            order: Order::from_parts(accounts.clone(), worker.clone()).with_opener(opener),
            agent: Agent::from_parts(accounts, worker).with_opener(opener),
            mock,
        }
    }
}

impl Offline {
    /// Drives one `Szamlazz.Order` handler (`call`, over the handlers of this
    /// deployment) as Restate would, over `runner`: executed once, and again
    /// from its first line after each retryable failure, until it answers.
    async fn order<T>(
        &self,
        runner: &FakeRunner,
        call: impl AsyncFn(OrderHandlers<'_>) -> Result<T, HandlerError>,
    ) -> Result<T, HandlerError> {
        runner.drive(|| call(self.order.over(runner))).await
    }

    /// Drives one `Szamlazz.Agent` handler; see [`Offline::order`].
    async fn agent<T>(
        &self,
        runner: &FakeRunner,
        call: impl AsyncFn(AgentHandlers<'_>) -> Result<T, HandlerError>,
    ) -> Result<T, HandlerError> {
        runner.drive(|| call(self.agent.over(runner))).await
    }

    /// szamlazz.hu holds nothing of `ORDER`: code 7 under every one of the
    /// four kinds' external ids and under the order number.
    async fn fresh_order(&self) {
        absent(
            &self.mock,
            &[
                "acct:ORD-1:proforma",
                "acct:ORD-1:invoice",
                "acct:ORD-1:prepayment",
                "acct:ORD-1:final",
            ],
            ORDER,
        )
        .await;
    }

    /// Nothing reached szamlazz.hu.
    async fn nothing_sent(&self) {
        assert!(
            self.mock
                .received_requests()
                .await
                .expect("requests")
                .is_empty(),
            "nothing sent"
        );
    }
}

/// A request body as the SDK hands it to the handler: the JSON bytes, decoded
/// by `Body<T>`'s never-failing SDK `Deserialize`.
fn body<T: DeserializeOwned>(value: &Value) -> Body<T> {
    raw_body(&serde_json::to_vec(value).expect("json"))
}

fn raw_body<T: DeserializeOwned>(bytes: &[u8]) -> Body<T> {
    Body::<T>::deserialize(&mut Bytes::copy_from_slice(bytes)).expect("infallible")
}

fn create_body() -> Value {
    json!({ "document": sample_document(), "options": { "reissue": false } })
}

fn create_body_with(options: &Value) -> Value {
    json!({ "document": sample_document(), "options": options })
}

fn as_json(response: impl Serialize) -> Value {
    serde_json::to_value(response).expect("json")
}

/// How a handler ended short of an answer: a fault (the `TerminalError`'s
/// status and its JSON body) or the retryable error the SDK would re-execute
/// the handler on.
#[derive(Debug)]
enum Ended {
    Fault { status: u16, body: Value },
    Retryable(String),
}

impl Ended {
    fn of(error: &HandlerError) -> Self {
        // `HandlerError` exposes its inner error through `AsRef<dyn Error>`
        // alone: `Terminal error [<code>]: <message>` or `Retryable error: …`.
        let text = (error.as_ref() as &dyn std::error::Error).to_string();
        let Some(rest) = text.strip_prefix("Terminal error [") else {
            return Self::Retryable(text);
        };
        let (status, message) = rest.split_once("]: ").expect("the SDK's terminal display");
        Self::Fault {
            status: status.parse().expect("a status"),
            body: serde_json::from_str(message).expect("a fault body"),
        }
    }

    fn fault(self) -> (u16, Value) {
        match self {
            Self::Fault { status, body } => (status, body),
            Self::Retryable(text) => {
                panic!("a fault was expected, the handler failed retryably: {text}")
            }
        }
    }

    /// The fault `error` is, asserted to carry `status` and the `code` token;
    /// its body, for what else a scenario asserts.
    fn expect_fault(error: &HandlerError, status: u16, code: &str) -> Value {
        let (actual, fault) = Self::of(error).fault();
        assert_eq!(actual, status, "{fault}");
        assert_eq!(fault["code"], code, "{fault}");
        fault
    }
}

/// `message` contains `needle`, on a fault body.
fn message_contains(fault: &Value, needle: &str) {
    assert!(
        fault["message"]
            .as_str()
            .is_some_and(|message| message.contains(needle)),
        "{fault}"
    );
}

// ----- the offline run-name pin -----------------------------------------------------

/// One driven handler: what it journaled, for the pin.
struct Walked {
    service: &'static str,
    handler: &'static str,
    names: Vec<String>,
}

impl Walked {
    fn order(handler: &'static str, runner: &FakeRunner) -> Self {
        Self {
            service: "Szamlazz.Order",
            handler,
            names: runner.journaled_names(),
        }
    }

    fn agent(handler: &'static str, runner: &FakeRunner) -> Self {
        Self {
            service: "Szamlazz.Agent",
            handler,
            names: runner.journaled_names(),
        }
    }

    /// The offline pin on this handler's journal: the patterns of what it
    /// journaled are a prefix of one of its pinned paths; the row it walked in
    /// full, when one.
    fn pin(&self) -> Option<usize> {
        let observed: Vec<String> = self.names.iter().map(|name| run_pattern(name)).collect();
        let paths: Vec<(usize, &[&str])> = RUN_NAMES
            .iter()
            .enumerate()
            .filter(|(_, row)| row.service == self.service && row.handler == self.handler)
            .map(|(index, row)| (index, row.path))
            .collect();
        assert!(
            !paths.is_empty(),
            "{}.{} has no pinned run names; pin its steps in RUN_NAMES",
            self.service,
            self.handler
        );
        assert!(
            paths
                .iter()
                .any(|(_, path)| is_prefix_of_path(&observed, path)),
            "{}.{} journaled {observed:?}, a prefix of none of its pinned paths {:?}.\n\
             A renamed, inserted, reordered or dropped step strands every in-flight invocation of \
             the previous deployment on replay (ADR 0005). Keep the names and their order; a step \
             that must change is a new row in RUN_NAMES and a deploy that drains first, never an \
             edited row.",
            self.service,
            self.handler,
            paths.iter().map(|(_, path)| path).collect::<Vec<_>>()
        );
        paths
            .into_iter()
            .find(|(_, path)| observed.len() == path.len() && is_prefix_of_path(&observed, path))
            .map(|(row, _)| row)
    }
}

/// The records of the runs named `name`.
fn runs_of(records: &[RunRecord], name: &str) -> Vec<RunRecord> {
    records
        .iter()
        .filter(|record| record.name == name)
        .cloned()
        .collect()
}

/// The `Recorded` of every run, in order, with the execution it was in.
fn ends(records: &[RunRecord]) -> Vec<(u32, &str, &Recorded)> {
    records
        .iter()
        .map(|record| (record.execution, record.name.as_str(), &record.recorded))
        .collect()
}

// ----- Szamlazz.Order: the creates ---------------------------------------------------

/// `create_proforma` on a fresh order: three exclusivity reads, the lookup
/// (external id and hint), the create, `issued`, in one execution.
async fn create_proforma_issues_after_the_exclusivity_reads() -> Walked {
    let offline = Offline::start().await;
    offline.fresh_order().await;
    create()
        .respond_with(created("D-1", "20000", "25400"))
        .expect(1)
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::object(ORDER);
    let response = offline
        .order(&runner, async |order| {
            order.create_proforma(body(&create_body())).await
        })
        .await
        .expect("issued");
    let response = as_json(response);
    assert_eq!(response["outcome"], "issued", "{response}");
    assert_eq!(response["invoice_number"], "D-1");
    assert_eq!(response["kind"], "proforma");
    assert_eq!(response["external_id"], "acct:ORD-1:proforma");
    assert_eq!(runner.executions(), 1);
    assert_eq!(
        runner.journaled_names(),
        [
            "namespace",
            "account",
            "exclusivity-invoice",
            "exclusivity-prepayment",
            "exclusivity-final",
            "lookup-proforma",
            "create-proforma",
        ]
    );
    Walked::order("create_proforma", &runner)
}

/// `create_proforma` on an order whose own invoice is live: the first
/// exclusivity read answers `conflict{order_invoiced}` and nothing after it
/// runs.
async fn create_proforma_stops_at_the_first_exclusivity_read() -> Walked {
    let offline = Offline::start().await;
    holds(
        &offline.mock,
        &Doc::new("SZ-1", "SZ"),
        Some("acct:ORD-1:invoice"),
    )
    .await;
    create()
        .respond_with(created("D-1", "1", "1"))
        .expect(0)
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::object(ORDER);
    let response = offline
        .order(&runner, async |order| {
            order.create_proforma(body(&create_body())).await
        })
        .await
        .expect("a conflict is an outcome");
    let response = as_json(response);
    assert_eq!(response["outcome"], "conflict", "{response}");
    assert_eq!(response["conflict_reason"], "order_invoiced");
    assert_eq!(response["existing_number"], "SZ-1");
    assert_eq!(
        runner.journaled_names(),
        ["namespace", "account", "exclusivity-invoice"]
    );
    Walked::order("create_proforma", &runner)
}

/// `create_invoice` under `proforma: auto` on a fresh order: the
/// `proforma-link` lookup, the lookup step, the create; `issued`.
async fn create_invoice_issues_through_the_proforma_link() -> Walked {
    let offline = Offline::start().await;
    offline.fresh_order().await;
    create()
        .respond_with(created("SZ-1", "20000", "25400"))
        .expect(1)
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::object(ORDER);
    let response = offline
        .order(&runner, async |order| {
            order.create_invoice(body(&create_body())).await
        })
        .await
        .expect("issued");
    let response = as_json(response);
    assert_eq!(response["outcome"], "issued", "{response}");
    assert_eq!(response["invoice_number"], "SZ-1");
    assert_eq!(response["gross_total"], "25400");
    assert_eq!(
        runner.journaled_names(),
        [
            "namespace",
            "account",
            "exclusivity-prepayment",
            "exclusivity-final",
            "proforma-link",
            "lookup-invoice",
            "create-invoice",
        ]
    );
    Walked::order("create_invoice", &runner)
}

/// `create_invoice` under `proforma: {number}`: the proforma is verified by
/// number (`verify-proforma-{number}`) in place of the link lookup, and the
/// create carries it (`dijbekeroSzamlaszam`).
async fn create_invoice_verifies_the_named_proforma() -> Walked {
    use wiremock::matchers::body_string_contains;

    let offline = Offline::start().await;
    absent(
        &offline.mock,
        &[
            "acct:ORD-1:prepayment",
            "acct:ORD-1:final",
            "acct:ORD-1:invoice",
        ],
        ORDER,
    )
    .await;
    // Held by number only: the order query above answers 7 first (wiremock
    // takes the first mounted match), so the hint sees nothing.
    number_query("D-1")
        .respond_with(Doc::new("D-1", "D").response())
        .mount(&offline.mock)
        .await;
    create()
        .and(body_string_contains(
            "<dijbekeroSzamlaszam>D-1</dijbekeroSzamlaszam>",
        ))
        .respond_with(created("SZ-1", "20000", "25400"))
        .expect(1)
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::object(ORDER);
    let response = offline
        .order(&runner, async |order| {
            order
                .create_invoice(body(&create_body_with(
                    &json!({ "proforma": { "number": "D-1" } }),
                )))
                .await
        })
        .await
        .expect("issued");
    assert_eq!(as_json(response)["outcome"], "issued");
    assert_eq!(
        runner.journaled_names(),
        [
            "namespace",
            "account",
            "exclusivity-prepayment",
            "exclusivity-final",
            "verify-proforma-D-1",
            "lookup-invoice",
            "create-invoice",
        ]
    );
    Walked::order("create_invoice", &runner)
}

/// A malformed body is the `invalid_input` fault naming the field before the
/// prologue: nothing journaled, nothing sent.
async fn create_invoice_refuses_a_malformed_body_before_the_prologue() -> Walked {
    let offline = Offline::start().await;
    let runner = FakeRunner::object(ORDER);
    let error = offline
        .order(&runner, async |order| {
            order
                .create_invoice(body(&create_body_with(&json!({ "resissue": true }))))
                .await
        })
        .await
        .expect_err("refused");
    let fault = Ended::expect_fault(&error, 400, "invalid_input");
    message_contains(&fault, "unknown field `resissue`");
    assert!(runner.journaled_names().is_empty());
    offline.nothing_sent().await;
    Walked::order("create_invoice", &runner)
}

/// An untrimmed key is `invalid_input` naming the rule before the prologue:
/// nothing journaled.
async fn create_invoice_refuses_an_untrimmed_key_before_the_prologue() -> Walked {
    let offline = Offline::start().await;
    let runner = FakeRunner::object(" ORD-1");
    let error = offline
        .order(&runner, async |order| {
            order.create_invoice(body(&create_body())).await
        })
        .await
        .expect_err("refused");
    let fault = Ended::expect_fault(&error, 400, "invalid_input");
    message_contains(&fault, "leading or trailing whitespace");
    assert!(runner.journaled_names().is_empty());
    Walked::order("create_invoice", &runner)
}

/// The lookup step finds the order's live invoice: `already_issued`, and the
/// create step never runs.
async fn create_invoice_stops_at_the_lookup_when_already_issued() -> Walked {
    let offline = Offline::start().await;
    absent(
        &offline.mock,
        &[
            "acct:ORD-1:prepayment",
            "acct:ORD-1:final",
            "acct:ORD-1:proforma",
        ],
        ORDER,
    )
    .await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(Doc::new("SZ-1", "SZ").response())
        .mount(&offline.mock)
        .await;
    create()
        .respond_with(created("SZ-2", "1", "1"))
        .expect(0)
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::object(ORDER);
    let response = offline
        .order(&runner, async |order| {
            order.create_invoice(body(&create_body())).await
        })
        .await
        .expect("an outcome");
    let response = as_json(response);
    assert_eq!(response["outcome"], "already_issued", "{response}");
    assert_eq!(response["invoice_number"], "SZ-1");
    assert_eq!(
        runner.journaled_names(),
        [
            "namespace",
            "account",
            "exclusivity-prepayment",
            "exclusivity-final",
            "proforma-link",
            "lookup-invoice",
        ]
    );
    Walked::order("create_invoice", &runner)
}

/// A create step that never settles spends the issue policy (five executions,
/// 2 m → 4 m → 8 m → 10 m apart) and is the `outcome_unknown` fault about
/// the document, repeating the last failure; every earlier step is replayed,
/// not re-run.
async fn an_exhausted_create_step_is_outcome_unknown_about_the_document() -> Walked {
    let offline = Offline::start().await;
    offline.fresh_order().await;

    let runner = FakeRunner::object(ORDER).exhaust("create-invoice", "the send's reply was lost");
    let error = offline
        .order(&runner, async |order| {
            order.create_invoice(body(&create_body())).await
        })
        .await
        .expect_err("exhausted");
    let fault = Ended::expect_fault(&error, 500, "outcome_unknown");
    assert_eq!(fault["order"], "ORD-1");
    assert_eq!(fault["kind"], "invoice");
    assert_eq!(fault["external_id"], "acct:ORD-1:invoice");
    message_contains(&fault, "the send's reply was lost");

    assert_eq!(runner.executions(), 5);
    assert_eq!(runner.clock(), Duration::from_mins(24));
    let creates = runs_of(&runner.record(), "create-invoice");
    let delays: Vec<_> = creates
        .iter()
        .filter_map(|record| match &record.recorded {
            Recorded::Retried { delay, .. } => Some(*delay),
            _ => None,
        })
        .collect();
    assert_eq!(
        delays,
        [
            Duration::from_mins(2),
            Duration::from_mins(4),
            Duration::from_mins(8),
            Duration::from_mins(10)
        ]
    );
    assert!(matches!(creates[4].recorded, Recorded::Exhausted { .. }));
    // The lookup ran once and was replayed four times.
    let lookups = runs_of(&runner.record(), "lookup-invoice");
    assert_eq!(lookups.len(), 5);
    assert!(matches!(lookups[0].recorded, Recorded::Journaled(_)));
    assert!(
        lookups[1..]
            .iter()
            .all(|record| matches!(record.recorded, Recorded::Replayed(Ok(_))))
    );
    Walked::order("create_invoice", &runner)
}

/// A lookup step that never answers spends the read policy (five
/// executions, 5 → 10 → 20 → 40 s apart) and is the `unavailable` fault
/// naming the step, about the document; the create never runs.
async fn an_exhausted_lookup_is_unavailable_about_the_document() -> Walked {
    let offline = Offline::start().await;
    absent(
        &offline.mock,
        &[
            "acct:ORD-1:prepayment",
            "acct:ORD-1:final",
            "acct:ORD-1:proforma",
        ],
        ORDER,
    )
    .await;
    create()
        .respond_with(created("SZ-1", "1", "1"))
        .expect(0)
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::object(ORDER).exhaust("lookup-invoice", "szlahu_down");
    let error = offline
        .order(&runner, async |order| {
            order.create_invoice(body(&create_body())).await
        })
        .await
        .expect_err("exhausted");
    let fault = Ended::expect_fault(&error, 503, "unavailable");
    assert_eq!(fault["order"], "ORD-1");
    assert_eq!(fault["kind"], "invoice");
    assert_eq!(fault["external_id"], "acct:ORD-1:invoice");
    let message = fault["message"].as_str().expect("message");
    assert!(
        message.contains("the lookup-invoice read ended without an answer")
            && message.contains("szlahu_down"),
        "{message}"
    );
    assert_eq!(runner.executions(), 5);
    assert_eq!(runner.clock(), Duration::from_secs(75));
    assert_eq!(
        runner.journaled_names(),
        [
            "namespace",
            "account",
            "exclusivity-prepayment",
            "exclusivity-final",
            "proforma-link",
            "lookup-invoice",
        ]
    );
    Walked::order("create_invoice", &runner)
}

/// A lookup that fails once is re-executed after the read policy's first
/// delay: `issued` in two executions, one create on the wire, the steps
/// before it replayed.
async fn a_flaky_lookup_is_re_executed_and_the_invoice_issues() -> Walked {
    let offline = Offline::start().await;
    offline.fresh_order().await;
    create()
        .respond_with(created("SZ-1", "20000", "25400"))
        .expect(1)
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::object(ORDER).fail_first("lookup-invoice", 1, "a 500");
    let response = offline
        .order(&runner, async |order| {
            order.create_invoice(body(&create_body())).await
        })
        .await
        .expect("issued");
    assert_eq!(as_json(response)["outcome"], "issued");
    assert_eq!(runner.executions(), 2);
    assert_eq!(runner.clock(), Duration::from_secs(5));
    let record = runner.record();
    let sequence: Vec<(u32, &str, bool)> = ends(&record)
        .into_iter()
        .map(|(execution, name, recorded)| {
            (execution, name, matches!(recorded, Recorded::Replayed(_)))
        })
        .collect();
    assert_eq!(
        sequence,
        [
            (1, "namespace", false),
            (1, "account", false),
            (1, "exclusivity-prepayment", false),
            (1, "exclusivity-final", false),
            (1, "proforma-link", false),
            (1, "lookup-invoice", false),
            (2, "namespace", true),
            (2, "account", true),
            (2, "exclusivity-prepayment", true),
            (2, "exclusivity-final", true),
            (2, "proforma-link", true),
            (2, "lookup-invoice", false),
            (2, "create-invoice", false),
        ]
    );
    Walked::order("create_invoice", &runner)
}

/// `create_prepayment` under `auto` on a fresh order.
async fn create_prepayment_issues_through_the_proforma_link() -> Walked {
    let offline = Offline::start().await;
    offline.fresh_order().await;
    create()
        .respond_with(created("ES-1", "20000", "25400"))
        .expect(1)
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::object(ORDER);
    let response = offline
        .order(&runner, async |order| {
            order.create_prepayment(body(&create_body())).await
        })
        .await
        .expect("issued");
    let response = as_json(response);
    assert_eq!(response["outcome"], "issued", "{response}");
    assert_eq!(response["kind"], "prepayment");
    assert_eq!(
        runner.journaled_names(),
        [
            "namespace",
            "account",
            "exclusivity-invoice",
            "exclusivity-final",
            "proforma-link",
            "lookup-prepayment",
            "create-prepayment",
        ]
    );
    Walked::order("create_prepayment", &runner)
}

/// `create_prepayment` on an order whose own invoice is live: the first
/// exclusivity read answers `conflict{prepaid_chain}` and nothing after it
/// runs.
async fn create_prepayment_stops_at_the_first_exclusivity_read() -> Walked {
    let offline = Offline::start().await;
    holds(
        &offline.mock,
        &Doc::new("SZ-1", "SZ"),
        Some("acct:ORD-1:invoice"),
    )
    .await;
    create()
        .respond_with(created("ES-1", "1", "1"))
        .expect(0)
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::object(ORDER);
    let response = offline
        .order(&runner, async |order| {
            order.create_prepayment(body(&create_body())).await
        })
        .await
        .expect("a conflict is an outcome");
    let response = as_json(response);
    assert_eq!(response["outcome"], "conflict", "{response}");
    assert_eq!(response["conflict_reason"], "prepaid_chain");
    assert_eq!(response["existing_number"], "SZ-1");
    assert_eq!(
        runner.journaled_names(),
        ["namespace", "account", "exclusivity-invoice"]
    );
    Walked::order("create_prepayment", &runner)
}

/// `create_prepayment` under `proforma: {number}`.
async fn create_prepayment_verifies_the_named_proforma() -> Walked {
    let offline = Offline::start().await;
    absent(
        &offline.mock,
        &[
            "acct:ORD-1:invoice",
            "acct:ORD-1:final",
            "acct:ORD-1:prepayment",
        ],
        ORDER,
    )
    .await;
    number_query("D-1")
        .respond_with(Doc::new("D-1", "D").response())
        .mount(&offline.mock)
        .await;
    create()
        .respond_with(created("ES-1", "20000", "25400"))
        .expect(1)
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::object(ORDER);
    let response = offline
        .order(&runner, async |order| {
            order
                .create_prepayment(body(&create_body_with(
                    &json!({ "proforma": { "number": "D-1" } }),
                )))
                .await
        })
        .await
        .expect("issued");
    assert_eq!(as_json(response)["outcome"], "issued");
    assert_eq!(
        runner.journaled_names(),
        [
            "namespace",
            "account",
            "exclusivity-invoice",
            "exclusivity-final",
            "verify-proforma-D-1",
            "lookup-prepayment",
            "create-prepayment",
        ]
    );
    Walked::order("create_prepayment", &runner)
}

/// `create_final` after a live prepayment invoice: `prepayment-for-final`,
/// the lookup, the create.
async fn create_final_issues_after_the_live_prepayment() -> Walked {
    let offline = Offline::start().await;
    holds(
        &offline.mock,
        &Doc::new("ES-1", "ES"),
        Some("acct:ORD-1:prepayment"),
    )
    .await;
    external_id_query("acct:ORD-1:final")
        .respond_with(not_found())
        .mount(&offline.mock)
        .await;
    create()
        .respond_with(created("VS-1", "20000", "25400"))
        .expect(1)
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::object(ORDER);
    let response = offline
        .order(&runner, async |order| {
            order.create_final(body(&create_body())).await
        })
        .await
        .expect("issued");
    let response = as_json(response);
    assert_eq!(response["outcome"], "issued", "{response}");
    assert_eq!(response["kind"], "final");
    assert_eq!(
        runner.journaled_names(),
        [
            "namespace",
            "account",
            "prepayment-for-final",
            "lookup-final",
            "create-final",
        ]
    );
    Walked::order("create_final", &runner)
}

/// `create_final` with nothing under `…:prepayment`:
/// `conflict{prepayment_missing}` after the one read.
async fn create_final_stops_when_the_prepayment_is_missing() -> Walked {
    let offline = Offline::start().await;
    absent(&offline.mock, &["acct:ORD-1:prepayment"], ORDER).await;

    let runner = FakeRunner::object(ORDER);
    let response = offline
        .order(&runner, async |order| {
            order.create_final(body(&create_body())).await
        })
        .await
        .expect("an outcome");
    let response = as_json(response);
    assert_eq!(response["outcome"], "conflict", "{response}");
    assert_eq!(response["conflict_reason"], "prepayment_missing");
    assert_eq!(
        runner.journaled_names(),
        ["namespace", "account", "prepayment-for-final"]
    );
    Walked::order("create_final", &runner)
}

/// `correct_invoice` on a live invoice of the order: the base verified by
/// number, the corrective's lookup (no hint), the create.
async fn correct_invoice_issues_after_verifying_the_base() -> Walked {
    let offline = Offline::start().await;
    holds(&offline.mock, &Doc::new("SZ-1", "SZ"), None).await;
    external_id_query("acct:ORD-1:corrective:fix-1")
        .respond_with(not_found())
        .mount(&offline.mock)
        .await;
    create()
        .respond_with(created("HS-1", "-20000", "-25400"))
        .expect(1)
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::object(ORDER);
    let response = offline
        .order(&runner, async |order| order.correct_invoice(body(&json!({ "invoice_number": "SZ-1", "correction_id": "fix-1", "document": sample_document(), }))).await)
        .await
        .expect("issued");
    let response = as_json(response);
    assert_eq!(response["outcome"], "issued", "{response}");
    assert_eq!(response["kind"], "corrective");
    assert_eq!(response["external_id"], "acct:ORD-1:corrective:fix-1");
    assert_eq!(
        runner.journaled_names(),
        [
            "namespace",
            "account",
            "verify-base-SZ-1",
            "lookup-corrective",
            "create-corrective",
        ]
    );
    Walked::order("correct_invoice", &runner)
}

/// `correct_invoice` on a base szamlazz.hu does not know: 404 `not_found`
/// carrying the corrective's identity, after the verify alone.
async fn correct_invoice_stops_on_an_unknown_base() -> Walked {
    let offline = Offline::start().await;
    number_query("SZ-9")
        .respond_with(not_found())
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::object(ORDER);
    let error = offline
        .order(&runner, async |order| order.correct_invoice(body(&json!({ "invoice_number": "SZ-9", "correction_id": "fix-1", "document": sample_document(), }))).await)
        .await
        .expect_err("not found");
    let fault = Ended::expect_fault(&error, 404, "not_found");
    assert_eq!(fault["kind"], "corrective");
    assert_eq!(fault["external_id"], "acct:ORD-1:corrective:fix-1");
    assert_eq!(
        runner.journaled_names(),
        ["namespace", "account", "verify-base-SZ-9"]
    );
    Walked::order("correct_invoice", &runner)
}

// ----- Szamlazz.Order: storno, delete, get ---------------------------------------------

/// `storno_invoice` on a live invoice of the order: the verify, the storno
/// lookup, the storno step (its send repeating the original's `telj`);
/// `reversed`.
async fn storno_invoice_reverses_a_live_invoice() -> Walked {
    use wiremock::matchers::body_string_contains;

    let offline = Offline::start().await;
    holds(&offline.mock, &Doc::new("SZ-1", "SZ"), None).await;
    external_id_query("acct:ORD-1:storno:SZ-1")
        .respond_with(not_found())
        .mount(&offline.mock)
        .await;
    storno()
        .and(body_string_contains(format!(
            "<teljesitesDatum>{ORIGINAL_TELJ}</teljesitesDatum>"
        )))
        .respond_with(created("SS-1", "-20000", "-25400"))
        .expect(1)
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::object(ORDER);
    let response = offline
        .order(&runner, async |order| {
            order
                .storno_invoice(body(&json!({ "invoice_number": "SZ-1" })))
                .await
        })
        .await
        .expect("reversed");
    let response = as_json(response);
    assert_eq!(response["outcome"], "reversed", "{response}");
    assert_eq!(response["storno_number"], "SS-1");
    assert_eq!(
        runner.journaled_names(),
        [
            "namespace",
            "account",
            "verify-storno-SZ-1",
            "lookup-storno-SZ-1",
            "storno-SZ-1",
        ]
    );
    Walked::order("storno_invoice", &runner)
}

/// The verify sees the invoice already reversed: the best-effort hint names
/// the storno; `reversed{storno_number}`, nothing sent.
async fn storno_invoice_reads_the_hint_when_already_reversed() -> Walked {
    let offline = Offline::start().await;
    number_query("SZ-1")
        .respond_with(
            Doc {
                reversed: true,
                ..Doc::new("SZ-1", "SZ")
            }
            .response(),
        )
        .mount(&offline.mock)
        .await;
    order_query(ORDER)
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-1"),
                ..Doc::new("SS-1", "SS")
            }
            .response(),
        )
        .mount(&offline.mock)
        .await;
    storno()
        .respond_with(created("SS-9", "0", "0"))
        .expect(0)
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::object(ORDER);
    let response = offline
        .order(&runner, async |order| {
            order
                .storno_invoice(body(&json!({ "invoice_number": "SZ-1" })))
                .await
        })
        .await
        .expect("reversed");
    let response = as_json(response);
    assert_eq!(response["outcome"], "reversed", "{response}");
    assert_eq!(response["storno_number"], "SS-1");
    assert_eq!(
        runner.journaled_names(),
        [
            "namespace",
            "account",
            "verify-storno-SZ-1",
            "hint-storno-SZ-1"
        ]
    );
    Walked::order("storno_invoice", &runner)
}

/// A cancellation at the best-effort hint propagates as the SDK's 409: a
/// cancelled invocation does not complete as `reversed`.
async fn a_cancellation_propagates_through_the_best_effort_hint() -> Walked {
    let offline = Offline::start().await;
    number_query("SZ-1")
        .respond_with(
            Doc {
                reversed: true,
                ..Doc::new("SZ-1", "SZ")
            }
            .response(),
        )
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::object(ORDER).cancel_at("hint-storno-SZ-1");
    let error = offline
        .order(&runner, async |order| {
            order
                .storno_invoice(body(&json!({ "invoice_number": "SZ-1" })))
                .await
        })
        .await
        .expect_err("cancelled");
    let text = (error.as_ref() as &dyn std::error::Error).to_string();
    assert_eq!(text, "Terminal error [409]: cancelled");
    assert_eq!(
        runner.journaled_names(),
        [
            "namespace",
            "account",
            "verify-storno-SZ-1",
            "hint-storno-SZ-1"
        ]
    );
    Walked::order("storno_invoice", &runner)
}

/// The best-effort hint's read policy exhausted is swallowed: `reversed`
/// without the storno number, after five executions.
async fn an_exhausted_best_effort_hint_reports_the_reversal_without_the_number() -> Walked {
    let offline = Offline::start().await;
    number_query("SZ-1")
        .respond_with(
            Doc {
                reversed: true,
                ..Doc::new("SZ-1", "SZ")
            }
            .response(),
        )
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::object(ORDER).exhaust("hint-storno-SZ-1", "a 500");
    let response = offline
        .order(&runner, async |order| {
            order
                .storno_invoice(body(&json!({ "invoice_number": "SZ-1" })))
                .await
        })
        .await
        .expect("reversed");
    let response = as_json(response);
    assert_eq!(response["outcome"], "reversed", "{response}");
    assert_eq!(response["storno_number"], Value::Null, "{response}");
    assert_eq!(runner.executions(), 5);
    Walked::order("storno_invoice", &runner)
}

/// A storno step that never settles is `outcome_unknown` about the storno.
async fn an_exhausted_storno_step_is_outcome_unknown_about_the_storno() -> Walked {
    let offline = Offline::start().await;
    holds(&offline.mock, &Doc::new("SZ-1", "SZ"), None).await;
    external_id_query("acct:ORD-1:storno:SZ-1")
        .respond_with(not_found())
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::object(ORDER).exhaust("storno-SZ-1", "reply lost");
    let error = offline
        .order(&runner, async |order| {
            order
                .storno_invoice(body(&json!({ "invoice_number": "SZ-1" })))
                .await
        })
        .await
        .expect_err("exhausted");
    let fault = Ended::expect_fault(&error, 500, "outcome_unknown");
    assert_eq!(fault["order"], "ORD-1");
    assert_eq!(fault["kind"], "invoice");
    assert_eq!(fault["external_id"], "acct:ORD-1:storno:SZ-1");
    assert_eq!(runner.executions(), 5);
    Walked::order("storno_invoice", &runner)
}

/// Another order's invoice: `conflict{not_managed}` after the verify alone.
async fn storno_invoice_stops_on_another_orders_invoice() -> Walked {
    let offline = Offline::start().await;
    number_query("SZ-2")
        .respond_with(
            Doc {
                order: Some("ORD-2"),
                ..Doc::new("SZ-2", "SZ")
            }
            .response(),
        )
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::object(ORDER);
    let response = offline
        .order(&runner, async |order| {
            order
                .storno_invoice(body(&json!({ "invoice_number": "SZ-2" })))
                .await
        })
        .await
        .expect("an outcome");
    let response = as_json(response);
    assert_eq!(response["outcome"], "conflict", "{response}");
    assert_eq!(response["conflict_reason"], "not_managed");
    assert_eq!(
        runner.journaled_names(),
        ["namespace", "account", "verify-storno-SZ-2"]
    );
    Walked::order("storno_invoice", &runner)
}

/// `delete_proforma` on the order's live proforma: the lookup, the one-shot
/// delete step; `deleted`.
async fn delete_proforma_deletes_the_live_proforma() -> Walked {
    let offline = Offline::start().await;
    holds(
        &offline.mock,
        &Doc::new("D-1", "D"),
        Some("acct:ORD-1:proforma"),
    )
    .await;
    delete_of("D-1")
        .respond_with(proforma_deleted())
        .expect(1)
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::object(ORDER);
    let response = offline
        .order(&runner, async |order| {
            order.delete_proforma(body(&json!({}))).await
        })
        .await
        .expect("deleted");
    let response = as_json(response);
    assert_eq!(response["deleted"], true, "{response}");
    assert_eq!(
        runner.journaled_names(),
        [
            "namespace",
            "account",
            "proforma-for-delete",
            "delete-proforma-D-1"
        ]
    );
    Walked::order("delete_proforma", &runner)
}

/// Nothing under `…:proforma`: `deleted{reason: absent}` after the lookup
/// alone, nothing sent.
async fn delete_proforma_stops_when_absent() -> Walked {
    let offline = Offline::start().await;
    absent(&offline.mock, &["acct:ORD-1:proforma"], ORDER).await;

    let runner = FakeRunner::object(ORDER);
    let response = offline
        .order(&runner, async |order| {
            order.delete_proforma(body(&json!({}))).await
        })
        .await
        .expect("an outcome");
    let response = as_json(response);
    assert_eq!(response["deleted"], true, "{response}");
    assert_eq!(response["reason"], "absent");
    assert_eq!(
        runner.journaled_names(),
        ["namespace", "account", "proforma-for-delete"]
    );
    Walked::order("delete_proforma", &runner)
}

/// `get`: four reads in kind order, the status.
async fn get_reads_the_four_slots() -> Walked {
    let offline = Offline::start().await;
    holds(
        &offline.mock,
        &Doc::new("SZ-1", "SZ"),
        Some("acct:ORD-1:invoice"),
    )
    .await;
    absent(
        &offline.mock,
        &[
            "acct:ORD-1:proforma",
            "acct:ORD-1:prepayment",
            "acct:ORD-1:final",
        ],
        ORDER,
    )
    .await;

    let runner = FakeRunner::object(ORDER);
    let status = offline
        .order(&runner, async |order| order.get().await)
        .await
        .expect("a status");
    let status = as_json(status);
    assert_eq!(status["invoice"]["state"], "live", "{status}");
    assert_eq!(status["invoice"]["number"], "SZ-1");
    assert_eq!(status["proforma"], Value::Null);
    assert_eq!(
        runner.journaled_names(),
        [
            "namespace",
            "account",
            "get-proforma",
            "get-invoice",
            "get-prepayment",
            "get-final",
        ]
    );
    Walked::order("get", &runner)
}

/// `get` with every read failing once: each failure ends the execution, and
/// the next one replays what settled and re-runs the failed read, so the
/// status comes in the fifth execution.
async fn get_with_every_read_failing_once_takes_five_executions() -> Walked {
    let offline = Offline::start().await;
    offline.fresh_order().await;

    let runner = FakeRunner::object(ORDER)
        .fail_first("get-proforma", 1, "a 500")
        .fail_first("get-invoice", 1, "a 500")
        .fail_first("get-prepayment", 1, "a 500")
        .fail_first("get-final", 1, "a 500");
    let status = offline
        .order(&runner, async |order| order.get().await)
        .await
        .expect("a status");
    assert_eq!(
        as_json(status),
        json!({ "proforma": null, "invoice": null, "prepayment": null, "final": null })
    );
    assert_eq!(runner.executions(), 5);
    assert_eq!(runner.clock(), Duration::from_secs(20), "4 × 5 s");
    Walked::order("get", &runner)
}

/// `get` with a read that never answers: the read policy exhausted is the
/// `unavailable` fault about that slot's document (`get` is a read that
/// must answer, so it is not best effort), and the reads after it never run.
async fn get_stops_at_a_read_that_never_answers() -> Walked {
    let offline = Offline::start().await;
    offline.fresh_order().await;

    let runner = FakeRunner::object(ORDER).exhaust("get-invoice", "szlahu_down");
    let error = offline
        .order(&runner, async |order| order.get().await)
        .await
        .expect_err("exhausted");
    let fault = Ended::expect_fault(&error, 503, "unavailable");
    assert_eq!(fault["order"], "ORD-1");
    assert_eq!(fault["kind"], "invoice");
    assert_eq!(fault["external_id"], "acct:ORD-1:invoice");
    message_contains(&fault, "the get-invoice read ended without an answer");
    assert_eq!(runner.executions(), 5);
    assert_eq!(
        runner.journaled_names(),
        ["namespace", "account", "get-proforma", "get-invoice"]
    );
    Walked::order("get", &runner)
}

// ----- Szamlazz.Agent -------------------------------------------------------------------

/// `check_account`: the probe; `ok` with the scope the runner saw.
async fn check_account_probes_the_sentinel_id() -> Walked {
    let offline = Offline::start().await;
    external_id_query("acct:check-account")
        .respond_with(not_found())
        .expect(1)
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::service();
    let response = offline
        .agent(&runner, async |agent| agent.check_account().await)
        .await
        .expect("ok");
    let response = as_json(response);
    assert_eq!(response["credentials"]["state"], "ok", "{response}");
    assert_eq!(response["account"]["id"], "acct");
    assert_eq!(response["namespace"], "acct");
    assert_eq!(response["scope"], Value::Null);
    assert_eq!(runner.journaled_names(), ["namespace", "account", "probe"]);
    Walked::agent("check_account", &runner)
}

/// A scoped request on the single-account deployment: `unknown_account`
/// after the prologue's two steps, nothing sent.
async fn a_scoped_request_on_a_single_account_deployment_is_unknown_account() -> Walked {
    let offline = Offline::start().await;
    let runner = FakeRunner::service().scoped("acme");
    let error = offline
        .agent(&runner, async |agent| agent.check_account().await)
        .await
        .expect_err("unknown account");
    let fault = Ended::expect_fault(&error, 400, "unknown_account");
    assert!(
        fault["message"]
            .as_str()
            .is_some_and(|m| m.contains("\"acme\"")),
        "{fault}"
    );
    assert_eq!(runner.journaled_names(), ["namespace", "account"]);
    offline.nothing_sent().await;
    Walked::agent("check_account", &runner)
}

/// `query` by number: the one read, the projection.
async fn query_projects_the_found_document() -> Walked {
    let offline = Offline::start().await;
    holds(&offline.mock, &Doc::new("SZ-1", "SZ"), None).await;

    let runner = FakeRunner::service();
    let response = offline
        .agent(&runner, async |agent| {
            agent
                .query(body(&json!({ "selector": { "invoice_number": "SZ-1" } })))
                .await
        })
        .await
        .expect("found");
    let response = as_json(response);
    assert_eq!(response["invoice_number"], "SZ-1", "{response}");
    assert_eq!(response["test"], true);
    assert_eq!(runner.journaled_names(), ["namespace", "account", "query"]);
    Walked::agent("query", &runner)
}

/// `query` on code 7: 404 `not_found`.
async fn query_answers_not_found_on_code_7() -> Walked {
    let offline = Offline::start().await;
    number_query("SZ-9")
        .respond_with(not_found())
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::service();
    let error = offline
        .agent(&runner, async |agent| {
            agent
                .query(body(&json!({ "selector": { "invoice_number": "SZ-9" } })))
                .await
        })
        .await
        .expect_err("not found");
    Ended::expect_fault(&error, 404, "not_found");
    assert_eq!(runner.journaled_names(), ["namespace", "account", "query"]);
    Walked::agent("query", &runner)
}

/// `query_taxpayer`: the one read named by the prefix, NAV's record.
async fn query_taxpayer_reads_nav_by_the_prefix() -> Walked {
    let offline = Offline::start().await;
    taxpayer_query("12345678")
        .respond_with(taxpayer_known())
        .expect(1)
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::service();
    let response = offline
        .agent(&runner, async |agent| {
            agent
                .query_taxpayer(body(&json!({ "tax_number": "12345678-2-42" })))
                .await
        })
        .await
        .expect("found");
    let response = as_json(response);
    assert_eq!(response["valid"], true, "{response}");
    assert_eq!(response["name"], "SYNTHETIC SOFTWARE KFT.");
    assert_eq!(
        runner.journaled_names(),
        ["namespace", "account", "taxpayer-12345678"]
    );
    Walked::agent("query_taxpayer", &runner)
}

/// A tax number in neither accepted form: `invalid_input` before the
/// prologue, nothing journaled.
async fn query_taxpayer_refuses_a_malformed_tax_number_before_the_prologue() -> Walked {
    let offline = Offline::start().await;
    let runner = FakeRunner::service();
    let error = offline
        .agent(&runner, async |agent| {
            agent
                .query_taxpayer(body(&json!({ "tax_number": "1234" })))
                .await
        })
        .await
        .expect_err("refused");
    Ended::expect_fault(&error, 400, "invalid_input");
    assert!(runner.journaled_names().is_empty());
    Walked::agent("query_taxpayer", &runner)
}

/// `set_payments`: the one-shot write, the totals.
async fn set_payments_registers_the_entries_in_one_step() -> Walked {
    let offline = Offline::start().await;
    credit()
        .respond_with(credited("SZ-1", "25400", "24400"))
        .expect(1)
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::service();
    let response = offline
        .agent(&runner, async |agent| agent.set_payments(body(&json!({ "invoice_number": "SZ-1", "entries": [{ "date": "2026-09-05", "method": "transfer", "amount": "1000", }], "additive": false, }))).await)
        .await
        .expect("done");
    let response = as_json(response);
    assert_eq!(response["outstanding"], "24400", "{response}");
    assert_eq!(response["gross_total"], "25400");
    assert_eq!(
        runner.journaled_names(),
        ["namespace", "account", "set-payments-SZ-1"]
    );
    Walked::agent("set_payments", &runner)
}

/// `set_payments` with a malformed body (a padded invoice number, refused by
/// the bounded contract type): `invalid_input` before the prologue, nothing
/// journaled, nothing sent.
async fn set_payments_refuses_a_malformed_body_before_the_prologue() -> Walked {
    let offline = Offline::start().await;
    let runner = FakeRunner::service();
    let error = offline
        .agent(&runner, async |agent| {
            agent
                .set_payments(body(&json!({
                    "invoice_number": " SZ-1",
                    "entries": [],
                    "additive": false,
                })))
                .await
        })
        .await
        .expect_err("refused");
    let fault = Ended::expect_fault(&error, 400, "invalid_input");
    message_contains(&fault, "malformed request body");
    assert!(runner.journaled_names().is_empty());
    offline.nothing_sent().await;
    Walked::agent("set_payments", &runner)
}

/// `set_payments` replacing with no entries: the one step journals the wire
/// contract's refusal as data (the gateway's `request` pseudo-code, nothing
/// sent), and the handler answers it as `invalid_input`.
async fn set_payments_answers_the_wire_contracts_refusal_as_invalid_input() -> Walked {
    let offline = Offline::start().await;
    let runner = FakeRunner::service();
    let error = offline
        .agent(&runner, async |agent| {
            agent
                .set_payments(body(&json!({
                    "invoice_number": "SZ-1",
                    "entries": [],
                    "additive": false,
                })))
                .await
        })
        .await
        .expect_err("refused");
    let fault = Ended::expect_fault(&error, 400, "invalid_input");
    message_contains(&fault, "nothing was sent");
    assert_eq!(
        runner.journaled_names(),
        ["namespace", "account", "set-payments-SZ-1"]
    );
    offline.nothing_sent().await;
    Walked::agent("set_payments", &runner)
}

/// `Szamlazz.Agent.storno` on a live document carrying no order number: the
/// verify, the by-number storno lookup, the storno step; `reversed`.
async fn agent_storno_reverses_an_unmanaged_invoice() -> Walked {
    let offline = Offline::start().await;
    holds(
        &offline.mock,
        &Doc {
            order: None,
            ..Doc::new("SZ-1", "SZ")
        },
        None,
    )
    .await;
    external_id_query("acct:by-number:SZ-1:storno")
        .respond_with(not_found())
        .mount(&offline.mock)
        .await;
    storno()
        .respond_with(created("SS-1", "-20000", "-25400"))
        .expect(1)
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::service();
    let response = offline
        .agent(&runner, async |agent| {
            agent
                .storno(body(&json!({ "invoice_number": "SZ-1" })))
                .await
        })
        .await
        .expect("reversed");
    let response = as_json(response);
    assert_eq!(response["outcome"], "reversed", "{response}");
    assert_eq!(response["storno_number"], "SS-1");
    assert_eq!(
        runner.journaled_names(),
        [
            "namespace",
            "account",
            "verify-SZ-1",
            "lookup-storno-SZ-1",
            "storno-SZ-1"
        ]
    );
    Walked::agent("storno", &runner)
}

/// An order-bearing document: `managed_by_order` after the verify alone.
async fn agent_storno_stops_on_an_orders_document() -> Walked {
    let offline = Offline::start().await;
    holds(&offline.mock, &Doc::new("SZ-1", "SZ"), None).await;

    let runner = FakeRunner::service();
    let response = offline
        .agent(&runner, async |agent| {
            agent
                .storno(body(&json!({ "invoice_number": "SZ-1" })))
                .await
        })
        .await
        .expect("an outcome");
    let response = as_json(response);
    assert_eq!(response["outcome"], "managed_by_order", "{response}");
    assert_eq!(response["order_key"], "ORD-1");
    assert_eq!(
        runner.journaled_names(),
        ["namespace", "account", "verify-SZ-1"]
    );
    Walked::agent("storno", &runner)
}

/// A reversed unmanaged document: the best-effort by-number storno lookup
/// names our storno; `reversed{storno_number}`.
async fn agent_storno_reads_the_lookup_when_already_reversed() -> Walked {
    let offline = Offline::start().await;
    number_query("SZ-1")
        .respond_with(
            Doc {
                order: None,
                reversed: true,
                ..Doc::new("SZ-1", "SZ")
            }
            .response(),
        )
        .mount(&offline.mock)
        .await;
    external_id_query("acct:by-number:SZ-1:storno")
        .respond_with(
            Doc {
                order: None,
                referenced_invoice: Some("SZ-1"),
                ..Doc::new("SS-1", "SS")
            }
            .response(),
        )
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::service();
    let response = offline
        .agent(&runner, async |agent| {
            agent
                .storno(body(&json!({ "invoice_number": "SZ-1" })))
                .await
        })
        .await
        .expect("reversed");
    let response = as_json(response);
    assert_eq!(response["outcome"], "reversed", "{response}");
    assert_eq!(response["storno_number"], "SS-1");
    assert_eq!(
        runner.journaled_names(),
        ["namespace", "account", "verify-SZ-1", "lookup-storno-SZ-1"]
    );
    Walked::agent("storno", &runner)
}

// ----- replay: a previous deployment's entries ----------------------------------------------

/// The committed `account` fixture (`tests/journal/resolution/account.json`,
/// a previous deployment's entry, its endpoint pointed at the mock) and the
/// `namespace` fixture seeded into the journal: `check_account` replays both
/// and answers from the **journaled** account, whose id is the fixture's, not
/// the configuration's, and whose credentials are fetched by the fixture's
/// reference. What ADR 0005's additive-only rule buys, seen through a whole
/// handler.
async fn a_previous_deployments_account_entry_replays_through_check_account() -> Walked {
    let offline = Offline::start_as("acme-credentials").await;
    external_id_query("acct:check-account")
        .respond_with(not_found())
        .expect(1)
        .mount(&offline.mock)
        .await;
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/journal");
    let namespace = std::fs::read(fixtures.join("namespace/value.json")).expect("fixture");
    let mut account: Value = serde_json::from_slice(
        &std::fs::read(fixtures.join("resolution/account.json")).expect("fixture"),
    )
    .expect("json");
    account["Account"]["endpoint"] = Value::String(format!("{}/", offline.mock.uri()));
    assert_eq!(account["Account"]["id"], "acme", "the fixture's account");
    assert_eq!(account["Account"]["credential_ref"], "acme-credentials");

    let runner = FakeRunner::service()
        .seed("namespace", namespace)
        .seed("account", serde_json::to_vec(&account).expect("json"));
    let response = offline
        .agent(&runner, async |agent| agent.check_account().await)
        .await
        .expect("ok");
    let response = as_json(response);
    assert_eq!(response["credentials"]["state"], "ok", "{response}");
    assert_eq!(response["account"]["id"], "acme", "the journaled account");
    let record = runner.record();
    assert!(matches!(record[0].recorded, Recorded::Replayed(Ok(_))));
    assert!(matches!(record[1].recorded, Recorded::Replayed(Ok(_))));
    assert!(matches!(record[2].recorded, Recorded::Journaled(_)));
    Walked::agent("check_account", &runner)
}

/// An `account` entry the current types cannot decode is the **retryable**
/// error the SDK's own deserialisation failure is, never a fault: the
/// invocation would replay into it until its attempts are spent, leaving a
/// rollback its window (ADR 0005). Nothing after it runs.
async fn an_undecodable_entry_fails_the_execution_retryably() -> Walked {
    let offline = Offline::start().await;
    let runner = FakeRunner::service()
        .seed("namespace", br#""acct""#.to_vec())
        .seed("account", br#"{"Account":{"id":1}}"#.to_vec());
    let error = offline
        .agent(&runner, async |agent| agent.check_account().await)
        .await
        .expect_err("undecodable");
    match Ended::of(&error) {
        Ended::Retryable(text) => assert!(
            text.contains("invalid type: integer `1`, expected a string"),
            "{text}"
        ),
        Ended::Fault { status, body } => panic!("a fault ({status}): {body}"),
    }
    assert_eq!(runner.journaled_names(), ["namespace", "account"]);
    offline.nothing_sent().await;
    Walked::agent("check_account", &runner)
}

// ----- the policies, as the runs carry them ------------------------------------------------

/// Which policy each step runs under, read off the record: `namespace` once,
/// `account` under the resolve policy, every read under the read policy, the
/// create and storno steps under the issue policy, the one-shot writes once.
#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "one test: the three handlers whose records together name every policy"
)]
async fn every_step_runs_under_its_policy() {
    let offline = Offline::start().await;
    let worker = WorkerConfig::new(namespace());
    let (once, read, issue, resolve) = (
        StepPolicy::ONCE,
        worker.read.step_policy(),
        worker.issue.step_policy(),
        worker.resolve.step_policy(),
    );

    absent(
        &offline.mock,
        &[
            "acct:ORD-1:prepayment",
            "acct:ORD-1:final",
            "acct:ORD-1:proforma",
            "acct:ORD-1:invoice",
            "acct:ORD-1:storno:SZ-1",
        ],
        ORDER,
    )
    .await;
    number_query("SZ-1")
        .respond_with(Doc::new("SZ-1", "SZ").response())
        .mount(&offline.mock)
        .await;
    create()
        .respond_with(created("SZ-1", "20000", "25400"))
        .mount(&offline.mock)
        .await;
    storno()
        .respond_with(created("SS-1", "-20000", "-25400"))
        .mount(&offline.mock)
        .await;
    credit()
        .respond_with(credited("SZ-1", "25400", "24400"))
        .mount(&offline.mock)
        .await;

    let runner = FakeRunner::object(ORDER);
    offline
        .order(&runner, async |order| {
            order.create_invoice(body(&create_body())).await
        })
        .await
        .expect("issued");
    let policies: Vec<(String, StepPolicy)> = runner
        .record()
        .into_iter()
        .map(|record| (record.name, record.policy))
        .collect();
    assert_eq!(
        policies,
        [
            ("namespace".to_owned(), once),
            ("account".to_owned(), resolve),
            ("exclusivity-prepayment".to_owned(), read),
            ("exclusivity-final".to_owned(), read),
            ("proforma-link".to_owned(), read),
            ("lookup-invoice".to_owned(), read),
            ("create-invoice".to_owned(), issue),
        ]
    );

    let runner = FakeRunner::object(ORDER);
    offline
        .order(&runner, async |order| {
            order
                .storno_invoice(body(&json!({ "invoice_number": "SZ-1" })))
                .await
        })
        .await
        .expect("reversed");
    let policies: Vec<(String, StepPolicy)> = runner
        .record()
        .into_iter()
        .map(|record| (record.name, record.policy))
        .collect();
    assert_eq!(
        policies,
        [
            ("namespace".to_owned(), once),
            ("account".to_owned(), resolve),
            ("verify-storno-SZ-1".to_owned(), read),
            ("lookup-storno-SZ-1".to_owned(), read),
            ("storno-SZ-1".to_owned(), issue),
        ]
    );

    let runner = FakeRunner::service();
    offline
        .agent(&runner, async |agent| agent.set_payments(body(&json!({ "invoice_number": "SZ-1", "entries": [{ "date": "2026-09-05", "method": "transfer", "amount": "1000" }], "additive": false, }))).await)
        .await
        .expect("done");
    assert_eq!(
        runs_of(&runner.record(), "set-payments-SZ-1")[0].policy,
        once
    );
}

/// The bytes a step journals are what the SDK's `Json<T>` writes: the
/// `namespace` entry is the committed fixture byte for byte, and the
/// `account` entry decodes as the journaled `Resolution` and re-encodes to
/// itself.
#[tokio::test]
async fn the_journaled_bytes_are_serde_json_to_vec_of_the_journaled_type() {
    use super::prologue::Resolution;

    let offline = Offline::start().await;
    external_id_query("acct:check-account")
        .respond_with(not_found())
        .mount(&offline.mock)
        .await;
    let runner = FakeRunner::service();
    offline
        .agent(&runner, async |agent| agent.check_account().await)
        .await
        .expect("ok");
    let fixture = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/journal/namespace/value.json"),
    )
    .expect("fixture");
    // The generator writes the fixture with a trailing newline.
    assert_eq!(
        runner.journaled("namespace").expect("journaled"),
        fixture.trim_ascii_end()
    );
    let account = runner.journaled("account").expect("journaled");
    let resolution: Resolution = serde_json::from_slice(&account).expect("a Resolution");
    assert_eq!(serde_json::to_vec(&resolution).expect("json"), account);
    assert!(
        !String::from_utf8_lossy(&account).contains(KEY),
        "the agent key is in no entry"
    );
}

// ----- the table ---------------------------------------------------------------------------

/// Declares the scenarios: one `#[tokio::test]` each (under `each`, asserting
/// the offline pin on what the scenario journaled) and one row of
/// [`SCENARIOS`] for the coverage test.
macro_rules! scenarios {
    ($($name:ident),* $(,)?) => {
        const SCENARIOS: &[(&str, fn() -> BoxFuture<'static, Walked>)] =
            &[$((stringify!($name), || Box::pin($name())),)*];

        mod each {
            $(
                #[tokio::test]
                async fn $name() {
                    super::$name().await.pin();
                }
            )*
        }
    };
}

scenarios! {
    create_proforma_issues_after_the_exclusivity_reads,
    create_proforma_stops_at_the_first_exclusivity_read,
    create_invoice_issues_through_the_proforma_link,
    create_invoice_verifies_the_named_proforma,
    create_invoice_refuses_a_malformed_body_before_the_prologue,
    create_invoice_refuses_an_untrimmed_key_before_the_prologue,
    create_invoice_stops_at_the_lookup_when_already_issued,
    an_exhausted_create_step_is_outcome_unknown_about_the_document,
    an_exhausted_lookup_is_unavailable_about_the_document,
    a_flaky_lookup_is_re_executed_and_the_invoice_issues,
    create_prepayment_issues_through_the_proforma_link,
    create_prepayment_stops_at_the_first_exclusivity_read,
    create_prepayment_verifies_the_named_proforma,
    create_final_issues_after_the_live_prepayment,
    create_final_stops_when_the_prepayment_is_missing,
    correct_invoice_issues_after_verifying_the_base,
    correct_invoice_stops_on_an_unknown_base,
    storno_invoice_reverses_a_live_invoice,
    storno_invoice_reads_the_hint_when_already_reversed,
    a_cancellation_propagates_through_the_best_effort_hint,
    an_exhausted_best_effort_hint_reports_the_reversal_without_the_number,
    an_exhausted_storno_step_is_outcome_unknown_about_the_storno,
    storno_invoice_stops_on_another_orders_invoice,
    delete_proforma_deletes_the_live_proforma,
    delete_proforma_stops_when_absent,
    get_reads_the_four_slots,
    get_with_every_read_failing_once_takes_five_executions,
    get_stops_at_a_read_that_never_answers,
    check_account_probes_the_sentinel_id,
    a_scoped_request_on_a_single_account_deployment_is_unknown_account,
    query_projects_the_found_document,
    query_answers_not_found_on_code_7,
    query_taxpayer_reads_nav_by_the_prefix,
    query_taxpayer_refuses_a_malformed_tax_number_before_the_prologue,
    set_payments_registers_the_entries_in_one_step,
    set_payments_refuses_a_malformed_body_before_the_prologue,
    set_payments_answers_the_wire_contracts_refusal_as_invalid_input,
    agent_storno_reverses_an_unmanaged_invoice,
    agent_storno_stops_on_an_orders_document,
    agent_storno_reads_the_lookup_when_already_reversed,
    a_previous_deployments_account_entry_replays_through_check_account,
    an_undecodable_entry_fails_the_execution_retryably,
}

/// The offline run-name pin over the suite: every path of [`RUN_NAMES`] is
/// walked in full by at least one scenario, so a step dropped from a path's
/// end, or a row the code no longer produces, fails here without a server.
/// (Each scenario asserts the prefix rule on its own journal.)
#[tokio::test]
async fn every_pinned_path_is_walked_in_full_offline() {
    let mut walked = BTreeSet::new();
    for (name, scenario) in SCENARIOS {
        if let Some(row) = scenario().await.pin() {
            walked.insert(row);
        }
        eprintln!("{name}: pinned");
    }
    let not_walked: Vec<String> = RUN_NAMES
        .iter()
        .enumerate()
        .filter(|(row, _)| !walked.contains(row))
        .map(|(_, row)| format!("{}.{}: {:?}", row.service, row.handler, row.path))
        .collect();
    assert!(
        not_walked.is_empty(),
        "pinned paths no scenario of the offline suite walked in full:\n  {}\n\n\
         Either a scenario must exercise the path or its last step was dropped from the handler.",
        not_walked.join("\n  ")
    );
}
