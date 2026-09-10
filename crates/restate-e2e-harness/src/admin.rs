//! The admin API as the harness speaks to it ([`Admin`]): the SQL
//! introspection endpoint ([`Admin::sql`]) and what is read through it
//! (journals, runs, `sys_invocation` rows, the registered handlers), the
//! invocation operations (kill, cancel, purge) and the deployment writes
//! (register, `set_public`). The object-wide sampler of run retries is
//! [`Watch`](crate::watch::Watch).

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::time::Instant;

use crate::introspection::{Handler, Invocation, JournalEntry};

/// How long the polls ([`Admin::await_status`], [`Admin::purge`],
/// [`Admin::await_in_flight_on`]) wait.
const POLL_DEADLINE: Duration = Duration::from_secs(30);

/// How long [`Admin::drain`] waits for the in-flight invocations.
const DRAIN_DEADLINE: Duration = Duration::from_secs(60);

/// How long [`Admin::register`] retries the registration.
const REGISTER_DEADLINE: Duration = Duration::from_secs(60);

/// `text` as a SQL string literal, quotes included: a `'` doubled. Every
/// value the harness interpolates into a query (a Virtual Object key, which
/// is the caller's arbitrary data; an invocation id) goes through this, so a
/// key such as `O'Brien` is a valid key and never a broken predicate.
#[must_use]
pub fn sql_literal(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// Polls `probe` every `interval` until it answers `Ok`, or panics with
/// `describe` of the last `Err` once `deadline` has passed: the one shape of
/// every wait on the server (a row to appear or go, a status to be reached,
/// an endpoint to register). The deadline bounds the wait as a whole: a probe
/// still running at it is cut (the HTTP client's own timeout, 120 s, would
/// otherwise outlast a 30 s wait on one stalled request) and reported as the
/// probe that did not answer; a failed probe whose retry sleep would cross
/// the deadline is the last, reported with `describe` of what it saw, so a
/// server's own refusal (a rejected registration) is what the panic carries,
/// never a generic message from a probe started at the deadline.
pub(crate) async fn poll_until<T, E, Fut>(
    deadline: Duration,
    interval: Duration,
    mut probe: impl FnMut() -> Fut,
    describe: impl Fn(&E) -> String,
) -> T
where
    Fut: Future<Output = Result<T, E>>,
{
    let deadline = Instant::now() + deadline;
    loop {
        let Ok(answer) = tokio::time::timeout_at(deadline, probe()).await else {
            panic!("the wait's deadline passed while a probe was in flight: {deadline:?}")
        };
        match answer {
            Ok(answer) => return answer,
            Err(last) => assert!(Instant::now() + interval < deadline, "{}", describe(&last)),
        }
        tokio::time::sleep(interval).await;
    }
}

/// Which scopes an object selector includes. The default is the unscoped
/// object, matching [`Call::object`](crate::Call::object).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ScopeSelection<'a> {
    /// Only the unscoped object (`scope IS NULL`).
    #[default]
    Unscoped,
    /// Only the object under this named scope.
    Named(&'a str),
    /// The unscoped object and every named scope's object.
    All,
}

/// Virtual Objects selected by service, key and scope selection. A key alone
/// is not an identity: two services, or two scopes, may hold the same key.
/// [`Self::object`] selects one unscoped object; [`Self::scoped`] selects one
/// named scope, and [`Self::all_scopes`] deliberately aggregates across them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target<'a> {
    /// `target_service_name`.
    pub service: &'a str,
    /// `target_service_key`.
    pub key: &'a str,
    /// Which scopes to include; service and key are always matched.
    pub scope: ScopeSelection<'a>,
}

impl<'a> Target<'a> {
    /// The unscoped object `key` of `service` (`scope IS NULL`).
    #[must_use]
    pub const fn object(service: &'a str, key: &'a str) -> Self {
        Self {
            service,
            key,
            scope: ScopeSelection::Unscoped,
        }
    }

    /// The same object under `scope` alone.
    #[must_use]
    pub const fn scoped(self, scope: &'a str) -> Self {
        Self {
            scope: ScopeSelection::Named(scope),
            ..self
        }
    }

    /// The same service/key in every scope, including the unscoped object.
    #[must_use]
    pub const fn all_scopes(self) -> Self {
        Self {
            scope: ScopeSelection::All,
            ..self
        }
    }

    /// The SQL predicate selecting this object's invocations.
    pub(crate) fn predicate(&self) -> String {
        let object = format!(
            "target_service_name = {} AND target_service_key = {}",
            sql_literal(self.service),
            sql_literal(self.key)
        );
        match self.scope {
            ScopeSelection::Unscoped => format!("{object} AND scope IS NULL"),
            ScopeSelection::Named(scope) => format!("{object} AND scope = {}", sql_literal(scope)),
            ScopeSelection::All => object,
        }
    }
}

/// The admin API of one Restate server, owned (a detached task carries its
/// own copy).
#[derive(Debug, Clone)]
pub struct Admin {
    base: String,
    http: reqwest::Client,
}

impl Admin {
    /// The admin API at `base` (`http://127.0.0.1:9070`), spoken to through
    /// `http`.
    #[must_use]
    pub fn new(base: String, http: reqwest::Client) -> Self {
        Self { base, http }
    }

    /// The base URL.
    #[must_use]
    pub fn base(&self) -> &str {
        &self.base
    }

    /// Runs `query` against the introspection API (`POST /query`): the rows,
    /// or why the exchange produced none (a transport failure, a non-200, a
    /// body that is not the rows). The caller decides whether that is fatal;
    /// a sampler retries, a scenario panics ([`Self::sql_or_panic`]).
    ///
    /// # Errors
    ///
    /// The exchange's failure, as text: the transport's, the status and body
    /// of a non-200, or a body without `rows`.
    pub async fn sql(&self, query: &str) -> Result<Vec<Value>, String> {
        let response = self
            .http
            .post(format!("{}/query", self.base))
            .header("accept", "application/json")
            .json(&json!({ "query": query }))
            .send()
            .await
            .map_err(|error| format!("sql query: {error}"))?;
        let status = response.status().as_u16();
        let body: Value = response
            .json()
            .await
            .map_err(|error| format!("sql json: {error}"))?;
        if status != 200 {
            return Err(format!("sql failed ({status}): {body}"));
        }
        body["rows"]
            .as_array()
            .cloned()
            .ok_or_else(|| format!("rows: {body}"))
    }

    /// [`Self::sql`] as a scenario's read: an exchange without rows is a
    /// failure of the scenario.
    pub async fn sql_or_panic(&self, query: &str) -> Vec<Value> {
        self.sql(query)
            .await
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// `PATCH {path}` with an optional JSON body, asserting a 2xx: the one
    /// shape of every admin-API write (an invocation operation, a service's
    /// visibility).
    async fn patch(&self, path: &str, body: Option<&Value>, what: &str) {
        let mut request = self.http.patch(format!("{}{path}", self.base));
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request
            .send()
            .await
            .unwrap_or_else(|error| panic!("PATCH {path}: {error}"));
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        assert!(
            (200..300).contains(&status),
            "{what} failed ({status}): {body}"
        );
    }

    /// `PATCH /invocations/{id}/{action}`, asserting success.
    async fn patch_invocation(&self, invocation_id: &str, action: &str) {
        self.patch(
            &format!("/invocations/{invocation_id}/{action}"),
            None,
            &format!("{action} of {invocation_id}"),
        )
        .await;
    }

    /// `PATCH /services/{service}` with `public`, asserting success.
    pub async fn set_public(&self, service: &str, public: bool) {
        self.patch(
            &format!("/services/{service}"),
            Some(&json!({ "public": public })),
            &format!("PATCH /services/{service} public={public}"),
        )
        .await;
    }

    /// Registers the endpoint at `uri` (`POST /deployments` with `force:
    /// true`), retried until the admin API accepts it: a new URI is a new
    /// revision of the services it binds, and new invocations route to it.
    pub async fn register(&self, uri: &str) {
        let registration = json!({ "uri": uri, "force": true });
        poll_until(
            REGISTER_DEADLINE,
            Duration::from_millis(500),
            || async {
                let response = self
                    .http
                    .post(format!("{}/deployments", self.base))
                    .json(&registration)
                    .send()
                    .await;
                match response {
                    Ok(response) if response.status().is_success() => Ok(()),
                    Ok(response) => Err(format!(
                        "deployment registration of {uri} failed: {}",
                        response.text().await.unwrap_or_default()
                    )),
                    Err(error) => Err(format!(
                        "admin API unreachable while registering {uri}: {error}"
                    )),
                }
            },
            String::clone,
        )
        .await;
    }

    /// Waits until `sys_invocation` holds no invocation that is not completed.
    pub async fn drain(&self) {
        poll_until(
            DRAIN_DEADLINE,
            Duration::from_millis(200),
            || async {
                let in_flight = self
                    .sql_or_panic(
                        "SELECT id, status FROM sys_invocation WHERE status <> 'completed'",
                    )
                    .await;
                if in_flight.is_empty() {
                    Ok(())
                } else {
                    Err(in_flight)
                }
            },
            |in_flight| format!("invocations still in flight: {in_flight:?}"),
        )
        .await;
    }

    /// Kills an invocation (`PATCH /invocations/{id}/kill`): what an operator
    /// does to one that will not finish, and what `on_max_attempts = kill`
    /// does after the handler's attempts are spent.
    pub async fn kill(&self, invocation_id: &str) {
        self.patch_invocation(invocation_id, "kill").await;
    }

    /// Cancels an invocation (`PATCH /invocations/{id}/cancel`): the
    /// cooperative stop. The server signals the running handler, whose next
    /// awaited step ends with the SDK's 409; the handler answers as it sees
    /// fit and the invocation completes with that answer. A kill ends it
    /// without one.
    pub async fn cancel(&self, invocation_id: &str) {
        self.patch_invocation(invocation_id, "cancel").await;
    }

    /// Pauses an invocation, preserving its journal for a later replay.
    pub async fn pause(&self, invocation_id: &str) {
        self.patch_invocation(invocation_id, "pause").await;
        self.await_status(invocation_id, &["paused"]).await;
    }

    /// Resumes an invocation on its pinned deployment.
    pub async fn resume(&self, invocation_id: &str) {
        self.patch_invocation(invocation_id, "resume").await;
    }

    /// Purges a completed invocation (`PATCH /invocations/{id}/purge`) and
    /// waits for its `sys_invocation` row to go (the purge is asynchronous),
    /// so a later call runs against a key Restate has no memory of.
    pub async fn purge(&self, invocation_id: &str) {
        self.patch_invocation(invocation_id, "purge").await;
        poll_until(
            POLL_DEADLINE,
            Duration::from_millis(200),
            || async {
                let rows = self
                    .sql_or_panic(&format!(
                        "SELECT id FROM sys_invocation WHERE id = {}",
                        sql_literal(invocation_id)
                    ))
                    .await;
                if rows.is_empty() { Ok(()) } else { Err(()) }
            },
            |()| format!("invocation {invocation_id} still present after purge"),
        )
        .await;
    }

    /// Waits until `sys_invocation` reports the invocation in one of
    /// `statuses`; the status it reached.
    pub async fn await_status(&self, invocation_id: &str, statuses: &[&str]) -> String {
        poll_until(
            POLL_DEADLINE,
            Duration::from_millis(100),
            || async {
                let rows = self
                    .sql_or_panic(&format!(
                        "SELECT status FROM sys_invocation WHERE id = {}",
                        sql_literal(invocation_id)
                    ))
                    .await;
                let status = rows
                    .first()
                    .and_then(|row| row["status"].as_str())
                    .unwrap_or_default()
                    .to_owned();
                if statuses.contains(&status.as_str()) {
                    Ok(status)
                } else {
                    Err(status)
                }
            },
            |status| format!("invocation {invocation_id} is {status:?}, not one of {statuses:?}"),
        )
        .await
    }

    /// The ids of the invocations matching `target` the server holds and has
    /// not completed, in id order. All-scope targets include the same
    /// service/key's invocations in every scope, including unscoped.
    pub async fn in_flight_ids_on(&self, target: &Target<'_>) -> Vec<String> {
        self.sql_or_panic(&format!(
            "SELECT id FROM sys_invocation WHERE {} AND status <> 'completed' ORDER BY id",
            target.predicate()
        ))
        .await
        .iter()
        .map(|row| row["id"].as_str().expect("id").to_owned())
        .collect()
    }

    /// The one invocation in flight matching `target`: its id, from
    /// `sys_invocation`; panics on none or more than one. How a scenario
    /// names an invocation the ingress has not answered yet (a call returns
    /// its id only with its answer): to cancel it, or to check that a retry
    /// attached to it.
    pub async fn in_flight_on(&self, target: &Target<'_>) -> String {
        let in_flight = self.in_flight_ids_on(target).await;
        assert_eq!(
            in_flight.len(),
            1,
            "one invocation in flight on {target:?}: {in_flight:?}"
        );
        in_flight[0].clone()
    }

    /// Waits until `sys_invocation` holds at least `count` invocations in flight
    /// matching `target` (accepted by the server, not completed): the
    /// server-side moment a call made while the key is held is queued behind
    /// it, which the ingress reports only with the call's answer. The ids.
    pub async fn await_in_flight_on(&self, target: &Target<'_>, count: usize) -> Vec<String> {
        poll_until(
            POLL_DEADLINE,
            Duration::from_millis(25),
            || async {
                let in_flight = self.in_flight_ids_on(target).await;
                if in_flight.len() >= count {
                    Ok(in_flight)
                } else {
                    Err(in_flight)
                }
            },
            |in_flight| {
                format!("{target:?} never had {count} invocation(s) in flight: {in_flight:?}")
            },
        )
        .await
    }

    /// The journal of an invocation, in index order.
    pub async fn journal(&self, invocation_id: &str) -> Vec<JournalEntry> {
        let rows = self
            .sql_or_panic(&format!(
                "SELECT index, version, entry_type, name, entry_json, raw FROM sys_journal WHERE id = {} ORDER BY index",
                sql_literal(invocation_id)
            ))
            .await;
        rows.iter().map(JournalEntry::from_row).collect()
    }

    /// The names of the `ctx.run` commands of an invocation, in journal
    /// order: which durable steps ran.
    pub async fn runs(&self, invocation_id: &str) -> Vec<String> {
        self.journal(invocation_id)
            .await
            .into_iter()
            .filter(JournalEntry::is_run)
            .filter_map(|entry| entry.name)
            .collect()
    }

    /// The `sys_invocation` row of an invocation; panics when there is none.
    pub async fn invocation(&self, invocation_id: &str) -> Invocation {
        let rows = self
            .sql_or_panic(&format!(
                "SELECT {} FROM sys_invocation WHERE id = {}",
                Invocation::COLUMNS,
                sql_literal(invocation_id)
            ))
            .await;
        let row = rows
            .first()
            .unwrap_or_else(|| panic!("no sys_invocation row for {invocation_id}"));
        Invocation::from_row(row)
    }

    /// Every journal entry of every invocation the server still holds, with
    /// `raw` hex-decoded, keyed by invocation id.
    pub async fn all_journals(&self) -> BTreeMap<String, Vec<JournalEntry>> {
        let rows = self
            .sql_or_panic(
                "SELECT id, index, version, entry_type, name, entry_json, raw FROM sys_journal ORDER BY id, index",
            )
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

    /// Every `sys_invocation` row the server still holds, with its id, in id
    /// order.
    pub async fn all_invocations(&self) -> Vec<(String, Invocation)> {
        let rows = self
            .sql_or_panic(&format!(
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

    /// Every handler of every registered service (`GET /services`, the latest
    /// revision of each): what the deployments offer, whether or not the run
    /// invoked it. What a [`Table`](crate::run_names::Table) is checked
    /// against, so a handler with neither a row nor an invocation is not
    /// invisible.
    pub async fn handlers(&self) -> Vec<Handler> {
        let response = self
            .http
            .get(format!("{}/services", self.base))
            .header("accept", "application/json")
            .send()
            .await
            .expect("GET /services");
        let status = response.status().as_u16();
        let body: Value = response.json().await.expect("the /services body is JSON");
        assert_eq!(status, 200, "GET /services failed ({status}): {body}");
        Handler::from_services(&body)
    }
}

// ----- the admin API, without a server -------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A Virtual Object key is the caller's data: a quote in it is doubled,
    /// so the predicate stays the predicate.
    #[test]
    fn a_sql_literal_doubles_its_quotes() {
        assert_eq!(sql_literal("ORD-1"), "'ORD-1'");
        assert_eq!(sql_literal("O'Brien"), "'O''Brien'");
        assert_eq!(sql_literal("x' OR '1'='1"), "'x'' OR ''1''=''1'");
        assert_eq!(sql_literal(""), "''");
        let target = Target::object("Svc", "O'Brien");
        assert_eq!(
            target.predicate(),
            "target_service_name = 'Svc' AND target_service_key = 'O''Brien' AND scope IS NULL"
        );
        assert!(
            target
                .scoped("acme")
                .predicate()
                .ends_with(" AND scope = 'acme'"),
            "a scoped target narrows to its scope"
        );
    }

    /// A probe that keeps failing ends with what it saw last, never with the
    /// in-flight message: the retry sleep that would cross the deadline is
    /// not taken.
    #[tokio::test(start_paused = true)]
    async fn poll_until_reports_the_last_failure_not_a_probe_started_at_the_deadline() {
        let outcome = tokio::spawn(poll_until::<(), u32, _>(
            Duration::from_secs(1),
            Duration::from_millis(300),
            {
                let mut attempt = 0;
                move || {
                    attempt += 1;
                    std::future::ready(Err(attempt))
                }
            },
            |attempt| format!("registration refused, attempt {attempt}"),
        ))
        .await;
        let message = outcome
            .expect_err("the deadline panics")
            .into_panic()
            .downcast::<String>()
            .expect("a message");
        assert!(
            message.starts_with("registration refused, attempt "),
            "{message}"
        );
    }

    /// A wait is bounded as a whole: a probe that never answers is cut at
    /// the deadline, not awaited past it. Under the paused clock the timeout
    /// fires at once.
    #[tokio::test(start_paused = true)]
    async fn poll_until_cuts_a_probe_still_in_flight_at_the_deadline() {
        let outcome = tokio::spawn(poll_until::<(), (), _>(
            Duration::from_secs(1),
            Duration::from_millis(10),
            std::future::pending,
            |()| String::new(),
        ))
        .await;
        let panic = outcome.expect_err("the deadline panics");
        let message = panic.into_panic().downcast::<String>().expect("a message");
        assert!(
            message.contains("deadline passed while a probe was in flight"),
            "{message}"
        );
    }
}
