//! Object-wide sampling ([`Watch`]) of the retry state `sys_invocation`
//! exposes while selected invocations are in flight, recorded as [`Retries`].
//!
//! # Observation limits
//!
//! **A missing observed retry is not evidence that no retry occurred.** These
//! observations are qualified to **Restate server 1.7.8**, with **vqueues,
//! protocol v7 and scoped Virtual Objects enabled**, using **Rust SDK 0.12.0**.
//! Recheck them when changing the server version, features or configuration.
//!
//! - `retry_count` is the **invoker's count of starts, including the first
//!   execution**, not the total executions of a durable step (`ctx.run`) and
//!   not the number of external sends. A start may replay completed steps;
//!   different steps may fail during one invocation.
//! - With vqueues, a retry delay at or above `invocation_yield_threshold`
//!   (**2 seconds** in the recorded configuration) yields to the scheduler.
//!   The invoker drops its status row: `sys_invocation` shows `ready` without
//!   `retry_count`, `last_failure` or `last_failure_related_command_name`, and
//!   `retry_count` starts again at 1 on the next execution. The watch does not
//!   query the vqueue tables or reconstruct these resets.
//! - Completion also clears these invoker columns. An invocation, or an
//!   entire retry window, can pass between samples unseen. Starting a watch
//!   before the call does not ensure its task samples before the call runs.
//! - Sampling is **key-wide aggregation**: every invocation matching the
//!   [`Target`]'s service, key and scope selection contributes. The maximum
//!   count and distinct failures can come from different invocations; they
//!   are not one invocation's history. An all-scope target combines scopes
//!   too. Queued or concurrent matching invocations keep the watch sampling.
//!   [`Retries::observed_idle`] means the selection fell idle after
//!   being seen in flight; use [`Admin::await_status`] for one invocation.
//!
//! # Configuring a retry-observation test
//!
//! Start [`Watch`] before the call and [`finish`](Watch::finish) it after the
//! call answers, so a call missed entirely does not leave a sampler running.
//! Use an isolated key and an explicit scope selection when asserting on one
//! scenario. Pair observations with the scenario's independent evidence, such
//! as its mock's external-call count, when asserting that work re-executed.
//!
//! A **one-second test retry delay** kept the invoker state visible below the
//! observed two-second yield threshold on the configuration above. This is a
//! version-qualified test choice, not a production retry recommendation or a
//! visibility guarantee. In particular, increasing it to two seconds does not
//! widen the observable window: it takes the scheduler-yield path instead.
//! Check the delays a test actually reaches, including any policy multiplier
//! and cap, rather than just its initial delay.
//!
//! The poll interval is nominally **100 ms**. Slow admin queries and task
//! scheduling can leave larger gaps; no one- or two-second delay guarantees
//! ten samples. [`Retries::samples`] counts successfully decoded queries (even
//! empty results), while [`Retries::query_errors`] and [`Retries::last_query_error`]
//! expose failed queries and malformed rows. A malformed row rejects the whole
//! sample without changing retry observations or establishing completion. The
//! error names its zero-based row index and column. Status must be a string;
//! nullable retry columns accept omission or null, but reject other wrong types.
//! These help diagnose sparse or failing observation,
//! but neither many successful samples nor zero query errors proves complete
//! coverage. A query cancelled by `finish` contributes to neither count.
//!
//! # Evidence
//!
//! The [recorded 1.7.8 experiment, §7](https://github.com/sagikazarmark/szamlazz-rs/blob/dad170068c7826e815d5af2629888c8b24797193/docs/research/2026-09-06-pretix-invoice-sync/raw/02-restate.md#7-observability-for-a-ui--reconciler)
//! observed a two-second run delay as `running` → `ready` (no invoker fields)
//! → `running` with the count reset to 1; one-second delays showed
//! `backing-off` with those fields. Its pinned server source reading covers
//! [`handle_task_error`](https://github.com/restatedev/restate/blob/v1.7.8/crates/invoker-impl/src/invocation_state_machine.rs),
//! the [`RetryViaScheduler` arm](https://github.com/restatedev/restate/blob/v1.7.8/crates/invoker-impl/src/lib.rs)
//! (`status_store.on_end`), and the
//! [yield configuration](https://github.com/restatedev/restate/blob/v1.7.8/crates/types/src/config/invocation.rs).
//!
//! The sampling decision (the sampler behind [`Watch`]) is a pure function
//! of the rows, tested here without a server.

use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::admin::{Admin, Target};

/// Nominal interval between sample starts; it guarantees no sample count or
/// retry visibility (see the module's observation limits). A sample that
/// takes longer is followed by the next one at once, without another sleep.
const POLL: Duration = Duration::from_millis(100);

/// The query [`Watch`] runs on `target`: every invocation on the Virtual
/// Objects selected by service, key and scope, their status and in-flight columns.
fn retries_query(target: &Target<'_>) -> String {
    format!(
        "SELECT status, retry_count, last_failure, last_failure_related_command_name \
         FROM sys_invocation WHERE {}",
        target.predicate()
    )
}

/// What a [`Watch`] saw of all selected invocations' run retries.
///
/// Partial, key-wide observations; absence is not evidence that no retry
/// occurred. See the [observation limits](self#observation-limits) for invoker
/// resets, scheduler yield, completion clearing and sampling gaps.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Retries {
    /// The highest `retry_count` seen: the invoker's count of starts, the
    /// first execution included. A maximum across all matching rows, not a
    /// sum or a durable-step execution count; scheduler yield can reset it.
    pub max_retry_count: u64,
    /// Every distinct `last_failure` seen, in order of first sight.
    pub failures: Vec<String>,
    /// Every distinct `last_failure_related_command_name` seen, in order of
    /// first sight.
    pub failing_commands: Vec<String>,
    /// Successfully decoded queries, including empty results, not rows or retries.
    /// Helps diagnose sampling density, but does not prove complete coverage.
    pub samples: u64,
    /// Whether the watch observed no selected invocation in flight after
    /// previously seeing one, rather than ending by [`Watch::finish`]. This
    /// describes the selection falling idle, not a particular invocation's
    /// completion; an invocation can also finish between samples unseen.
    pub observed_idle: bool,
    /// How many queries failed or contained malformed rows (retried, never
    /// fatal). A malformed sample contributes no observations. Zero errors
    /// does not rule out gaps between successful samples.
    pub query_errors: u64,
    /// The last such failure.
    pub last_query_error: Option<String>,
}

/// Whether the sampler goes on.
#[derive(Debug, PartialEq, Eq)]
enum Progress {
    /// Keep sampling: nothing in the selection was seen in flight yet, or
    /// something still is.
    Sampling,
    /// The selection was seen in flight and nothing in it is any more.
    Done,
}

/// One decoded observation. Nullable columns may be omitted by Restate's
/// JSON writer; other type mismatches are observation errors, not absence.
struct SampleRow<'a> {
    status: &'a str,
    retry_count: Option<u64>,
    last_failure: Option<&'a str>,
    failing_command: Option<&'a str>,
}

impl<'a> SampleRow<'a> {
    fn from_row(row: &'a Value, index: usize) -> Result<Self, String> {
        fn column<'a, T: Deserialize<'a>>(
            row: &'a Value,
            index: usize,
            name: &str,
        ) -> Result<T, String> {
            T::deserialize(&row[name])
                .map_err(|error| format!("retry sample row {index}, column {name}: {error}"))
        }
        Ok(Self {
            status: column(row, index, "status")?,
            retry_count: column(row, index, "retry_count")?,
            last_failure: column(row, index, "last_failure")?,
            failing_command: column(row, index, "last_failure_related_command_name")?,
        })
    }
}

/// The sampling decision over `sys_invocation` rows, pure: records the
/// in-flight columns of every row and decides when the selection falls idle.
/// An object may carry older, completed invocations from earlier
/// scenarios; they carry none of the columns and do not count as in flight.
#[derive(Debug, Default)]
struct Sampler {
    retries: Retries,
    seen_in_flight: bool,
}

impl Sampler {
    /// Records one sample's rows.
    fn observe(&mut self, rows: &[Value]) -> Progress {
        // Decode the entire sample before changing observations: a malformed
        // later row must not leave partial counts or establish completion.
        let rows: Result<Vec<_>, _> = rows
            .iter()
            .enumerate()
            .map(|(index, row)| SampleRow::from_row(row, index))
            .collect();
        let rows = match rows {
            Ok(rows) => rows,
            Err(error) => {
                self.query_failed(error);
                return Progress::Sampling;
            }
        };
        self.retries.samples += 1;
        let mut in_flight = false;
        for row in rows {
            if row.status != "completed" {
                in_flight = true;
            }
            if let Some(count) = row.retry_count {
                self.retries.max_retry_count = self.retries.max_retry_count.max(count);
            }
            if let Some(failure) = row.last_failure
                && !self.retries.failures.iter().any(|seen| seen == failure)
            {
                self.retries.failures.push(failure.to_owned());
            }
            if let Some(command) = row.failing_command
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
            self.retries.observed_idle = true;
            Progress::Done
        } else {
            Progress::Sampling
        }
    }

    /// Records a sample the admin API did not answer or whose rows cannot be decoded.
    fn query_failed(&mut self, error: String) {
        self.retries.query_errors += 1;
        self.retries.last_query_error = Some(error);
    }

    fn into_retries(self) -> Retries {
        self.retries
    }
}

/// A running sampler of all invocations matching a [`Target`]: one unscoped
/// object, one named scope's object, or the same service/key across all scopes.
/// It ends on its own once the selection falls idle after being seen in
/// flight, on [`Self::finish`], or when dropped; there is no window to wait out.
///
/// This is object-wide aggregation, not sampling one invocation id. To observe
/// a particular invocation's completion, use [`Admin::await_status`].
/// See [observation limits](self#observation-limits) and
/// [test configuration guidance](self#configuring-a-retry-observation-test)
/// before interpreting a missing retry or choosing a test retry delay.
#[derive(Debug)]
pub struct Watch {
    stop: watch::Sender<bool>,
    task: JoinHandle<Retries>,
}

impl Watch {
    /// Samples `admin`'s `sys_invocation` for the invocations on `target`
    /// at a nominal 100 ms interval until one of the three ends above. Start
    /// it before the call, [`finish`](Self::finish) it after: the sampler ends
    /// as soon as it observes the selection fall idle, and `finish` ends one whose call
    /// was answered between two samples. Other matching invocations contribute
    /// to the same [`Retries`] and keep the selection in flight.
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

    /// Stops sampling and returns what was seen, even if matching invocations
    /// are still in flight. A sample in flight is cancelled, not awaited.
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

    use serde_json::json;

    use super::*;

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
                    Some("place-hold"),
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
                    Some("place-hold"),
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
                failing_commands: vec!["place-hold".to_owned()],
                samples: 6,
                observed_idle: true,
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

    #[test]
    fn malformed_samples_are_errors_and_change_no_observations() {
        let mut cases = vec![
            (Value::Null, "status"),
            (json!(42), "status"),
            (json!([]), "status"),
            (json!({}), "status"),
        ];
        for value in [Value::Null, json!(false), json!(1), json!([]), json!({})] {
            cases.push((json!({"status": value}), "status"));
        }
        for value in [
            json!("3"),
            json!(-1),
            json!(1.5),
            json!(false),
            json!([]),
            json!({}),
        ] {
            cases.push((
                json!({"status": "completed", "retry_count": value}),
                "retry_count",
            ));
        }
        for field in ["last_failure", "last_failure_related_command_name"] {
            for value in [json!(3), json!(false), json!([]), json!({})] {
                let mut malformed = json!({"status": "completed"});
                malformed[field] = value;
                cases.push((malformed, field));
            }
        }
        for (malformed, field) in cases {
            let mut sampler = Sampler::default();
            let before = row("running", Some(99), Some("discard"), Some("discard"));
            assert_eq!(
                sampler.observe(&[before, malformed.clone()]),
                Progress::Sampling
            );
            let retries = &sampler.retries;
            assert_eq!(retries.samples, 0, "{malformed}");
            assert_eq!(retries.query_errors, 1, "{malformed}");
            let error = retries.last_query_error.as_deref().expect("decoding error");
            assert!(error.contains("row 1"), "{error}");
            assert!(error.contains(field), "{error}");
            assert_eq!(retries.max_retry_count, 0);
            assert!(retries.failures.is_empty());
            assert!(retries.failing_commands.is_empty());
            assert!(!retries.observed_idle);
            assert_eq!(
                sampler.observe(&[]),
                Progress::Sampling,
                "a rejected sample cannot establish in-flight work"
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn malformed_samples_do_not_complete_the_watch_and_valid_samples_recover() {
        let mut script = vec![
            // Omitted nullable columns and unknown status strings are valid.
            Ok(vec![json!({"status": "new-server-status"})]),
            Ok(vec![json!({"status": "completed", "retry_count": "3"})]),
            Ok(vec![row(
                "backing-off",
                Some(3),
                Some("failure"),
                Some("step"),
            )]),
            // Explicit nulls are valid too.
            Ok(vec![row("completed", None, None, None)]),
        ]
        .into_iter();
        let mut watch = Watch::over(move || std::future::ready(script.next().expect("script")));
        let retries = (&mut watch.task).await.expect("watch recovers");
        assert_eq!(retries.samples, 3);
        assert_eq!(retries.query_errors, 1);
        assert!(
            retries
                .last_query_error
                .as_deref()
                .expect("decoding error")
                .contains("retry_count")
        );
        assert_eq!(retries.max_retry_count, 3);
        assert_eq!(retries.failures, ["failure"]);
        assert_eq!(retries.failing_commands, ["step"]);
        assert!(retries.observed_idle);
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
                Some("lookup-stock"),
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
        assert_eq!(retries.failing_commands, ["lookup-stock"]);
        assert!(retries.observed_idle);
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
        assert!(!retries.observed_idle, "{retries:?}");
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
        assert!(!retries.observed_idle);
    }
}
