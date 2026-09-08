//! The admin API as the harness speaks to it ([`Admin`]): the SQL
//! introspection endpoint ([`Admin::sql`]) and what is read through it
//! (journals, runs, `sys_invocation` rows), the invocation operations (kill,
//! cancel, purge) and the sampler ([`Watch`]) that records what
//! `sys_invocation` reports of an invocation's run retries **while it is in
//! flight** (`retry_count`, `last_failure` and
//! `last_failure_related_command_name` are in-flight columns, cleared once
//! the invocation completes; verified against 1.7.8) and ends as soon as it
//! observes the invocation completed.
//!
//! The sampling decision (the sampler behind [`Watch`]) is a pure function of
//! the rows, tested here without a server.

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::introspection::{Invocation, JournalEntry};

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
/// probe that did not answer; a failed probe is not followed by another
/// after the deadline.
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
            Err(last) => assert!(Instant::now() < deadline, "{}", describe(&last)),
        }
        tokio::time::sleep(interval).await;
    }
}

/// One Virtual Object as `sys_invocation` identifies it: the service, the
/// key, and the scope under scoped Virtual Objects. A key alone is not an
/// identity: two services, or two scopes, may hold the same key, and a read
/// by key alone would merge their invocations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target<'a> {
    /// `target_service_name`.
    pub service: &'a str,
    /// `target_service_key`.
    pub key: &'a str,
    /// `scope`: `Some` selects the object under that scope alone, `None`
    /// does not filter on the scope (the unscoped object, or every scope's,
    /// for a suite that keys uniquely across them).
    pub scope: Option<&'a str>,
}

impl<'a> Target<'a> {
    /// The object `key` of `service`, in whatever scope.
    #[must_use]
    pub const fn object(service: &'a str, key: &'a str) -> Self {
        Self {
            service,
            key,
            scope: None,
        }
    }

    /// The same object under `scope` alone.
    #[must_use]
    pub const fn scoped(self, scope: &'a str) -> Self {
        Self {
            scope: Some(scope),
            ..self
        }
    }

    /// The SQL predicate selecting this object's invocations.
    fn predicate(&self) -> String {
        let object = format!(
            "target_service_name = {} AND target_service_key = {}",
            sql_literal(self.service),
            sql_literal(self.key)
        );
        match self.scope {
            Some(scope) => format!("{object} AND scope = {}", sql_literal(scope)),
            None => object,
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

    /// The ids of the invocations on Virtual Object `target` the server holds
    /// and has not completed, in id order.
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

    /// The one invocation in flight on Virtual Object `target`: its id, from
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

    /// Waits until `sys_invocation` holds `count` invocations in flight on
    /// Virtual Object `target` (accepted by the server, not completed): the
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
                "SELECT index, entry_type, name, raw FROM sys_journal WHERE id = {} ORDER BY index",
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
                "SELECT id, index, entry_type, name, raw FROM sys_journal ORDER BY id, index",
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
}

// ----- the sampler --------------------------------------------------------------

/// How often [`Watch`] samples `sys_invocation`: a run retry under a test
/// policy of one or two seconds of back-off is visible for ten samples or
/// more. A sample that takes longer than this is followed by the next one at
/// once, never by a full interval's sleep.
const POLL: Duration = Duration::from_millis(100);

/// The query [`Watch`] runs on `target`: every invocation on the Virtual
/// Object, its status and its in-flight columns.
fn retries_query(target: &Target<'_>) -> String {
    format!(
        "SELECT status, retry_count, last_failure, last_failure_related_command_name \
         FROM sys_invocation WHERE {}",
        target.predicate()
    )
}

/// What a [`Watch`] saw of an invocation's run retries while it ran.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Retries {
    /// The highest `retry_count` seen: the invoker's count of starts, the
    /// first execution included.
    pub max_retry_count: u64,
    /// Every distinct `last_failure` seen, in order of first sight.
    pub failures: Vec<String>,
    /// Every distinct `last_failure_related_command_name` seen, in order of
    /// first sight.
    pub failing_commands: Vec<String>,
    /// How many samples answered; the density behind the three above.
    pub samples: u64,
    /// Whether the watch ended by observing the invocation completed (the
    /// key idle after it was in flight), rather than by
    /// [`Watch::finish`]: a watch that ended the second way saw the whole
    /// run only if the call was answered between two samples.
    pub observed_completion: bool,
    /// How many samples the admin API failed to answer (retried, never
    /// fatal).
    pub query_errors: u64,
    /// The last such failure.
    pub last_query_error: Option<String>,
}

/// Whether the sampler goes on.
#[derive(Debug, PartialEq, Eq)]
enum Progress {
    /// Keep sampling: nothing on the key was seen in flight yet, or
    /// something still is.
    Sampling,
    /// The key was seen in flight and nothing on it is any more: the
    /// invocation completed, its in-flight columns are gone.
    Done,
}

/// The sampling decision over `sys_invocation` rows, pure: records the
/// in-flight columns of every row and decides when the watched invocation is
/// over. A key may carry older, completed invocations from earlier
/// scenarios; they carry none of the columns and do not count as in flight.
#[derive(Debug, Default)]
struct Sampler {
    retries: Retries,
    seen_in_flight: bool,
}

impl Sampler {
    /// Records one sample's rows.
    fn observe(&mut self, rows: &[Value]) -> Progress {
        self.retries.samples += 1;
        let mut in_flight = false;
        for row in rows {
            if row["status"].as_str() != Some("completed") {
                in_flight = true;
            }
            if let Some(count) = row["retry_count"].as_u64() {
                self.retries.max_retry_count = self.retries.max_retry_count.max(count);
            }
            if let Some(failure) = row["last_failure"].as_str()
                && !self.retries.failures.iter().any(|seen| seen == failure)
            {
                self.retries.failures.push(failure.to_owned());
            }
            if let Some(command) = row["last_failure_related_command_name"].as_str()
                && !self
                    .retries
                    .failing_commands
                    .iter()
                    .any(|seen| seen == command)
            {
                self.retries.failing_commands.push(command.to_owned());
            }
        }
        if in_flight {
            self.seen_in_flight = true;
            Progress::Sampling
        } else if self.seen_in_flight {
            self.retries.observed_completion = true;
            Progress::Done
        } else {
            Progress::Sampling
        }
    }

    /// Records a sample the admin API did not answer.
    fn query_failed(&mut self, error: String) {
        self.retries.query_errors += 1;
        self.retries.last_query_error = Some(error);
    }

    fn into_retries(self) -> Retries {
        self.retries
    }
}

/// A running sampler of one Virtual Object key: started before the call,
/// finished after it ([`Watch::finish`]). It ends on its own once it observes
/// the invocation completed, on `finish`, or when dropped; there is no
/// window to wait out.
#[derive(Debug)]
pub struct Watch {
    stop: watch::Sender<bool>,
    task: JoinHandle<Retries>,
}

impl Watch {
    /// Samples `admin`'s `sys_invocation` for the invocations on `target`
    /// every 100 ms until one of the three ends above. Start it before the
    /// call, [`finish`](Self::finish) it after: the sampler ends as soon as
    /// it observes the invocation completed, and `finish` ends one whose call
    /// was answered between two samples.
    #[must_use]
    pub fn start(admin: Admin, target: &Target<'_>) -> Self {
        let query = retries_query(target);
        Self::over(move || {
            let admin = admin.clone();
            let query = query.clone();
            async move { admin.sql(&query).await }
        })
    }

    /// The sampler over `sample`, one call per sample: the seam the tests
    /// drive with scripted rows. The stop is selected against the sample as
    /// well as against the interval, so a stop cancels a query in flight (an
    /// admin API that stalls would otherwise hold `finish` for the client's
    /// timeout) instead of waiting for its answer, which a stopped watch has
    /// no use for.
    fn over<S, F>(mut sample: S) -> Self
    where
        S: FnMut() -> F + Send + 'static,
        F: Future<Output = Result<Vec<Value>, String>> + Send,
    {
        let (stop, mut stopped) = watch::channel(false);
        let task = tokio::spawn(async move {
            let mut sampler = Sampler::default();
            loop {
                let sampled_at = Instant::now();
                let answer = tokio::select! {
                    biased;
                    _ = stopped.changed() => break,
                    answer = sample() => answer,
                };
                match answer {
                    Ok(rows) => {
                        if sampler.observe(&rows) == Progress::Done {
                            break;
                        }
                    }
                    Err(error) => sampler.query_failed(error),
                }
                tokio::select! {
                    biased;
                    _ = stopped.changed() => break,
                    () = tokio::time::sleep_until(sampled_at + POLL) => {}
                }
            }
            sampler.into_retries()
        });
        Self { stop, task }
    }

    /// Stops sampling (the call has returned, so the invocation is complete
    /// and its in-flight columns gone) and returns what was seen; a sample in
    /// flight is cancelled, not awaited.
    pub async fn finish(mut self) -> Retries {
        let _ = self.stop.send(true);
        (&mut self.task).await.expect("the watch task")
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

// ----- the sampler, without a server ----------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

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
        assert!(
            retries_query(&target)
                .ends_with("target_service_name = 'Svc' AND target_service_key = 'O''Brien'")
        );
        assert!(
            retries_query(&target.scoped("acme")).ends_with(" AND scope = 'acme'"),
            "a scoped target narrows to its scope"
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

    /// A `sys_invocation` row as the sampler reads it.
    fn row(
        status: &str,
        retry_count: Option<u64>,
        failure: Option<&str>,
        command: Option<&str>,
    ) -> Value {
        json!({
            "status": status,
            "retry_count": retry_count,
            "last_failure": failure,
            "last_failure_related_command_name": command,
        })
    }

    /// The sampler records the in-flight columns of every row (the highest
    /// count, each distinct failure and failing command once, in order) and
    /// is done the first time it sees nothing in flight on a key it saw in
    /// flight before; rows of older, completed invocations on the key neither
    /// count as in flight nor end it before the watched one started.
    #[test]
    fn the_sampler_records_the_in_flight_columns_and_ends_when_the_key_falls_idle() {
        let mut sampler = Sampler::default();
        let old = row("completed", None, None, None);
        assert_eq!(sampler.observe(&[]), Progress::Sampling, "nothing yet");
        assert_eq!(
            sampler.observe(std::slice::from_ref(&old)),
            Progress::Sampling,
            "an earlier scenario's invocation on the key is not the watched one"
        );
        assert_eq!(
            sampler.observe(&[old.clone(), row("running", Some(1), None, None)]),
            Progress::Sampling,
            "the first execution"
        );
        assert_eq!(
            sampler.observe(&[
                old.clone(),
                row(
                    "backing-off",
                    Some(2),
                    Some("transport failure: reply lost"),
                    Some("create-invoice"),
                ),
            ]),
            Progress::Sampling,
            "the back-off before the second execution"
        );
        assert_eq!(
            sampler.observe(&[
                old.clone(),
                row(
                    "running",
                    Some(2),
                    Some("transport failure: reply lost"),
                    Some("create-invoice"),
                ),
            ]),
            Progress::Sampling,
            "the same failure again: recorded once"
        );
        assert_eq!(
            sampler.observe(&[old.clone(), old]),
            Progress::Done,
            "nothing in flight any more: the invocation completed"
        );
        assert_eq!(
            sampler.into_retries(),
            Retries {
                max_retry_count: 2,
                failures: vec!["transport failure: reply lost".to_owned()],
                failing_commands: vec!["create-invoice".to_owned()],
                samples: 6,
                observed_completion: true,
                query_errors: 0,
                last_query_error: None,
            }
        );
    }

    /// A sample the admin API did not answer is counted, kept as the last
    /// error and changes nothing else: the next sample is taken.
    #[test]
    fn a_failed_query_is_counted_and_never_fatal() {
        let mut sampler = Sampler::default();
        assert_eq!(
            sampler.observe(&[row("running", Some(1), None, None)]),
            Progress::Sampling
        );
        sampler.query_failed("sql query: connection reset".to_owned());
        sampler.query_failed("sql failed (503): {}".to_owned());
        assert_eq!(
            sampler.observe(&[row("completed", None, None, None)]),
            Progress::Done
        );
        let retries = sampler.into_retries();
        assert_eq!(retries.samples, 2, "the failed queries are not samples");
        assert_eq!(retries.query_errors, 2);
        assert_eq!(
            retries.last_query_error.as_deref(),
            Some("sql failed (503): {}")
        );
        assert_eq!(retries.max_retry_count, 1);
    }

    /// The watch ends by itself when a sample shows the key idle after it was
    /// in flight, before `finish` is called; a query failure in between is
    /// retried at the next sample.
    #[tokio::test]
    async fn the_watch_ends_on_its_own_when_the_invocation_completes() {
        let script = Arc::new(Mutex::new(vec![
            Ok(vec![]),
            Ok(vec![row("running", Some(1), None, None)]),
            Err("sql query: connection reset".to_owned()),
            Ok(vec![row(
                "backing-off",
                Some(2),
                Some("transport failure"),
                Some("lookup-invoice"),
            )]),
            Ok(vec![row("completed", None, None, None)]),
        ]));
        let sampled = Arc::new(AtomicUsize::new(0));
        let (script_for_watch, sampled_for_watch) = (script.clone(), sampled.clone());
        let mut watch = Watch::over(move || {
            let script = script_for_watch.clone();
            let sampled = sampled_for_watch.clone();
            async move {
                sampled.fetch_add(1, Ordering::SeqCst);
                let mut script = script.lock().expect("script");
                assert!(!script.is_empty(), "sampled past the completed row");
                script.remove(0)
            }
        });
        // The task ends without `finish`: await it directly.
        let retries = (&mut watch.task).await.expect("the watch task");
        assert_eq!(sampled.load(Ordering::SeqCst), 5);
        assert_eq!(retries.samples, 4);
        assert_eq!(retries.query_errors, 1);
        assert_eq!(retries.max_retry_count, 2);
        assert_eq!(retries.failing_commands, ["lookup-invoice"]);
        assert!(retries.observed_completion);
        assert!(script.lock().expect("script").is_empty());
    }

    /// `finish` stops a watch whose key it never saw in flight (a call
    /// answered between two samples) at once, with what it saw: under tokio's
    /// paused clock the interval sleep is cancelled rather than waited out, so
    /// no time passes in `finish` at all (a waited-out interval would
    /// auto-advance the clock to the sleep's deadline). `finish` is called
    /// halfway through an interval, so that a sleep that is not cancelled has
    /// half of it to go.
    #[tokio::test(start_paused = true)]
    async fn finish_stops_a_watch_that_saw_nothing_in_flight() {
        let sampled = Arc::new(AtomicUsize::new(0));
        let sampled_for_watch = sampled.clone();
        let watch = Watch::over(move || {
            let sampled = sampled_for_watch.clone();
            async move {
                sampled.fetch_add(1, Ordering::SeqCst);
                Ok(vec![row("completed", None, None, None)])
            }
        });
        tokio::time::sleep(POLL * 2 + POLL / 2).await;
        let started = Instant::now();
        let retries = tokio::time::timeout(Duration::from_secs(1), watch.finish())
            .await
            .expect("finish returns");
        assert_eq!(
            started.elapsed(),
            Duration::ZERO,
            "finish does not wait out a poll interval"
        );
        assert!(retries.samples >= 2, "{retries:?}");
        assert!(!retries.observed_completion, "{retries:?}");
        assert_eq!(retries.max_retry_count, 0);
        assert!(retries.failing_commands.is_empty());
        assert_eq!(
            retries.samples,
            u64::try_from(sampled.load(Ordering::SeqCst)).expect("count"),
            "every sample answered"
        );
    }

    /// `finish` cancels a sample in flight instead of awaiting its answer: a
    /// sample that never answers (an admin API that stalls) does not hold
    /// `finish` for the client's timeout.
    #[tokio::test(start_paused = true)]
    async fn finish_cancels_a_sample_in_flight() {
        let watch = Watch::over(std::future::pending::<Result<Vec<Value>, String>>);
        tokio::time::sleep(POLL).await;
        let started = Instant::now();
        let retries = tokio::time::timeout(Duration::from_secs(1), watch.finish())
            .await
            .expect("finish returns without the sample's answer");
        assert_eq!(started.elapsed(), Duration::ZERO, "{retries:?}");
        assert_eq!(
            retries.samples, 0,
            "the one sample never answered: {retries:?}"
        );
        assert!(!retries.observed_completion);
    }
}
