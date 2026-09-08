//! The admin API as the harness speaks to it: the SQL introspection endpoint
//! ([`Admin::sql`]) and the sampler over it ([`Watch`]) that records what
//! `sys_invocation` reports of an invocation's run retries **while it is in
//! flight** (`retry_count`, `last_failure` and
//! `last_failure_related_command_name` are in-flight columns, cleared once
//! the invocation completes; verified against 1.7.8) and ends as soon as it
//! observes the invocation completed.
//!
//! The sampling decision ([`Sampler`]) is a pure function of the rows, tested
//! here without a server.

use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// The admin API of one Restate server, owned (a detached task carries its
/// own copy).
#[derive(Clone)]
pub(crate) struct Admin {
    base: String,
    http: reqwest::Client,
}

impl Admin {
    pub(crate) fn new(base: String, http: reqwest::Client) -> Self {
        Self { base, http }
    }

    /// Runs `query` against the introspection API (`POST /query`): the rows,
    /// or why the exchange produced none (a transport failure, a non-200, a
    /// body that is not the rows). The caller decides whether that is fatal;
    /// a sampler retries, a scenario panics.
    pub(crate) async fn sql(&self, query: &str) -> Result<Vec<Value>, String> {
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
}

// ----- the sampler --------------------------------------------------------------

/// How often [`Watch`] samples `sys_invocation`: a run retry under the test
/// policies (one or two seconds of back-off) is visible for ten samples or
/// more. A sample that takes longer than this is followed by the next one at
/// once, never by a full interval's sleep.
const POLL: Duration = Duration::from_millis(100);

/// The query [`Watch`] runs on `key`: every invocation on the Virtual Object,
/// its status and its in-flight columns.
fn retries_query(key: &str) -> String {
    format!(
        "SELECT status, retry_count, last_failure, last_failure_related_command_name \
         FROM sys_invocation WHERE target_service_key = '{key}'"
    )
}

/// What a [`Watch`] saw of an invocation's run retries while it ran.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Retries {
    /// The highest `retry_count` seen: the invoker's count of starts, the
    /// first execution included.
    pub(crate) max_retry_count: u64,
    /// Every distinct `last_failure` seen, in order of first sight.
    pub(crate) failures: Vec<String>,
    /// Every distinct `last_failure_related_command_name` seen, in order of
    /// first sight.
    pub(crate) failing_commands: Vec<String>,
    /// How many samples answered; the density behind the three above.
    pub(crate) samples: u64,
    /// Whether the watch ended by observing the invocation completed (the
    /// key idle after it was in flight), rather than by
    /// [`Watch::finish`]: a watch that ended the second way saw the whole
    /// run only if the call was answered between two samples.
    pub(crate) observed_completion: bool,
    /// How many samples the admin API failed to answer (retried, never
    /// fatal), and the last such failure.
    pub(crate) query_errors: u64,
    pub(crate) last_query_error: Option<String>,
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
pub(crate) struct Watch {
    stop: watch::Sender<bool>,
    task: JoinHandle<Retries>,
}

impl Watch {
    /// Samples `admin`'s `sys_invocation` for the invocations on `key` every
    /// [`POLL`] until one of the three ends above.
    pub(crate) fn start(admin: Admin, key: &str) -> Self {
        let query = retries_query(key);
        Self::over(move || {
            let admin = admin.clone();
            let query = query.clone();
            async move { admin.sql(&query).await }
        })
    }

    /// The sampler over `sample`, one call per sample: the seam the tests
    /// drive with scripted rows.
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
                match sample().await {
                    Ok(rows) => {
                        if sampler.observe(&rows) == Progress::Done {
                            break;
                        }
                    }
                    Err(error) => sampler.query_failed(error),
                }
                if *stopped.borrow() {
                    break;
                }
                tokio::select! {
                    () = tokio::time::sleep_until((sampled_at + POLL).into()) => {}
                    _ = stopped.changed() => break,
                }
            }
            sampler.into_retries()
        });
        Self { stop, task }
    }

    /// Stops sampling (the call has returned, so the invocation is complete
    /// and its in-flight columns gone) and returns what was seen.
    pub(crate) async fn finish(mut self) -> Retries {
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

/// A `sys_invocation` row as the sampler reads it.
#[cfg(test)]
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

/// The sampler records the in-flight columns of every row (the highest count,
/// each distinct failure and failing command once, in order) and is done the
/// first time it sees nothing in flight on a key it saw in flight before;
/// rows of older, completed invocations on the key neither count as in
/// flight nor end it before the watched one started.
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

/// A sample the admin API did not answer is counted, kept as the last error
/// and changes nothing else: the next sample is taken.
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

/// The watch ends by itself when a sample shows the key idle after it was in
/// flight, before `finish` is called; a query failure in between is retried
/// at the next sample.
#[tokio::test]
async fn the_watch_ends_on_its_own_when_the_invocation_completes() {
    let script = std::sync::Arc::new(std::sync::Mutex::new(vec![
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
    let sampled = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (script_for_watch, sampled_for_watch) = (script.clone(), sampled.clone());
    let mut watch = Watch::over(move || {
        let script = script_for_watch.clone();
        let sampled = sampled_for_watch.clone();
        async move {
            sampled.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let mut script = script.lock().expect("script");
            assert!(!script.is_empty(), "sampled past the completed row");
            script.remove(0)
        }
    });
    // The task ends without `finish`: await it directly.
    let retries = (&mut watch.task).await.expect("the watch task");
    assert_eq!(sampled.load(std::sync::atomic::Ordering::SeqCst), 5);
    assert_eq!(retries.samples, 4);
    assert_eq!(retries.query_errors, 1);
    assert_eq!(retries.max_retry_count, 2);
    assert_eq!(retries.failing_commands, ["lookup-invoice"]);
    assert!(retries.observed_completion);
    assert!(script.lock().expect("script").is_empty());
}

/// `finish` stops a watch whose key it never saw in flight (a call answered
/// between two samples) at once, with what it saw.
#[tokio::test]
async fn finish_stops_a_watch_that_saw_nothing_in_flight() {
    let sampled = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let sampled_for_watch = sampled.clone();
    let watch = Watch::over(move || {
        let sampled = sampled_for_watch.clone();
        async move {
            sampled.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(vec![row("completed", None, None, None)])
        }
    });
    tokio::time::sleep(POLL * 3).await;
    let started = Instant::now();
    let retries = watch.finish().await;
    assert!(
        started.elapsed() < POLL,
        "finish does not wait out a poll interval: {:?}",
        started.elapsed()
    );
    assert!(retries.samples >= 2, "{retries:?}");
    assert!(!retries.observed_completion, "{retries:?}");
    assert_eq!(retries.max_retry_count, 0);
    assert!(retries.failing_commands.is_empty());
    assert_eq!(
        retries.samples,
        u64::try_from(sampled.load(std::sync::atomic::Ordering::SeqCst)).expect("count"),
        "every sample answered"
    );
}
