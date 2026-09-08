//! [`FakeRunner`]: a [`Runner`] with an in-memory journal, for driving a
//! handler without a server.
//!
//! It does what the SDK and the server do around a `ctx.run`, in miniature
//! and deterministically: it **journals** a step's bytes by position and
//! **replays** them on a re-execution without calling the step; a step that
//! fails retryably **ends the execution** (the run future never resolves;
//! [`drive`](FakeRunner::drive) drops the handler future, as the SDK's
//! abort does) and the handler is **re-executed from its first line** after
//! the policy's delay, or the run ends with the policy's terminal 500 when
//! `max_attempts` or `max_duration` is spent, exactly by the server's rule
//! (`restate-sdk-shared-core` 7.0.3, `retries.rs`: `initial_delay ×
//! factor^(n−1)` capped at `max_delay`; exhausted when `max_attempts ≤ n` or
//! `max_duration ≤` the time since the entry's first failure). Time is
//! **simulated**: the fake advances its own clock by each delay it would
//! have waited, never sleeping, so a suite over it is as fast as its wiremock
//! round trips and never trips a real timeout the way paused tokio time can
//! (paused time auto-advances to the next timer whenever the runtime idles on
//! a socket, which is exactly what an in-flight HTTP request does).
//!
//! What a test can do with it: **seed** a completion by name before the first
//! execution ([`seed`](FakeRunner::seed)), a replay of a previous
//! deployment's entry, the ADR 0005 property the fixture tests approximate
//! per type, driven here through a whole handler; **inject** a retryable
//! failure on a step's first `n` executions without running it
//! ([`fail_first`](FakeRunner::fail_first)), or on every one until the policy
//! is exhausted ([`exhaust`](FakeRunner::exhaust)); **cancel** the invocation
//! when a step is reached ([`cancel_at`](FakeRunner::cancel_at)), the SDK's
//! 409 before the closure runs; and read back every run in order
//! ([`record`](FakeRunner::record): name, policy, which handler execution and
//! which attempt of the step, and how it ended: journaled bytes, a replay, a
//! retried or exhausted failure, a cancellation)
//! and the names of the journal ([`journaled_names`](FakeRunner::journaled_names)),
//! which the offline run-name pin reads.

use std::collections::HashMap;
use std::future::{Future, poll_fn};
use std::pin::pin;
use std::sync::Mutex;
use std::task::Poll;
use std::time::Duration;

use restate_sdk::errors::{HandlerError, TerminalError};

use super::{BoxFuture, Runner, Step};
use crate::config::StepPolicy;

/// The SDK's status for a cancelled run (`endpoint/context.rs`).
const CANCELLED: u16 = 409;

/// How the fake answers a run that reached it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Recorded {
    /// The step ran and settled: these bytes are journaled.
    Journaled(Vec<u8>),
    /// A journaled completion was replayed without running the step: the
    /// bytes, or the terminal failure (its code and message) the entry
    /// holds.
    Replayed(Completion),
    /// The step failed retryably (its own failure, or an injected one) and
    /// the execution was ended; the policy re-executes it after `delay`.
    Retried { message: String, delay: Duration },
    /// The step failed retryably and the policy was exhausted: the run's
    /// terminal 500 carrying `message`, journaled.
    Exhausted { message: String },
    /// The invocation was cancelled at this step: the run's terminal 409
    /// before the step ran, journaled.
    Cancelled,
}

/// One `run` call, as the fake saw it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RunRecord {
    pub(crate) name: String,
    pub(crate) policy: StepPolicy,
    /// Which execution of the handler (1-based) the call was made in.
    pub(crate) execution: u32,
    /// Which attempt of the step this was (1-based; what the policy's
    /// `max_attempts` counts): `0` for a replay, which attempts nothing.
    pub(crate) attempt: u32,
    pub(crate) recorded: Recorded,
}

/// What a test scripts for a step, by name.
#[derive(Debug, Clone)]
enum Script {
    /// Fail retryably, without running, on the first `attempts` executions.
    FailFirst { attempts: u32, message: String },
    /// Cancel the invocation when the step is reached.
    Cancel,
}

/// How a journaled run completed: its bytes, or a terminal failure's code and
/// message (an exhausted policy's 500, a cancellation's 409).
type Completion = Result<Vec<u8>, (u16, String)>;

/// What `begin` decided for a run.
enum Begin {
    /// Replay this journaled completion.
    Replay(Completion),
    /// Run the step.
    Run,
    /// Fail without running, with this message.
    Fail(String),
    /// Cancel: the 409.
    Cancel,
}

/// What `settle` decided for a run that failed retryably.
enum Settled {
    /// End the execution; re-execute after the delay.
    Retry,
    /// The policy is spent: the terminal 500.
    Exhausted(TerminalError),
}

#[derive(Debug, Default)]
struct Inner {
    /// The journal: every run ever started, with how it completed.
    journal: Vec<(String, Completion)>,
    /// The replay cursor of the current execution.
    position: usize,
    /// The current execution, 1-based; 0 before the first.
    execution: u32,
    /// Executions of each step so far, by name (the entry's retry count).
    attempts: HashMap<String, u32>,
    /// The simulated clock at each step's first failure, by name.
    first_failure: HashMap<String, Duration>,
    /// The simulated clock: the sum of every delay waited so far.
    clock: Duration,
    scripts: HashMap<String, Script>,
    record: Vec<RunRecord>,
    /// Set by a run that ended its execution; read by `drive`.
    execution_ended: bool,
}

/// A [`Runner`] with an in-memory journal; see the module docs.
#[derive(Debug)]
pub(crate) struct FakeRunner {
    invocation_id: String,
    scope: Option<String>,
    key: Option<String>,
    inner: Mutex<Inner>,
}

impl FakeRunner {
    /// A runner as an object handler sees its context: keyed by `key`,
    /// unscoped.
    pub(crate) fn object(key: &str) -> Self {
        Self::new(Some(key))
    }

    /// A runner as a stateless service handler sees its context: no key,
    /// unscoped.
    pub(crate) fn service() -> Self {
        Self::new(None)
    }

    fn new(key: Option<&str>) -> Self {
        Self {
            invocation_id: "inv_fake".to_owned(),
            scope: None,
            key: key.map(str::to_owned),
            inner: Mutex::new(Inner::default()),
        }
    }

    /// The same runner under `scope`.
    pub(crate) fn scoped(mut self, scope: &str) -> Self {
        self.scope = Some(scope.to_owned());
        self
    }

    /// Journals a completed run named `name` holding `bytes` before the first
    /// execution, at the next position: the handler's run at that position
    /// replays it (the name must match, as the SDK's journal mismatch check
    /// would insist) instead of running its step. A previous deployment's
    /// entry, replayed through the current code.
    pub(crate) fn seed(self, name: &str, bytes: impl Into<Vec<u8>>) -> Self {
        self.lock()
            .journal
            .push((name.to_owned(), Ok(bytes.into())));
        self
    }

    /// The step named `name` fails retryably with `message`, without running,
    /// on its first `attempts` executions, and runs from the next one on.
    pub(crate) fn fail_first(self, name: &str, attempts: u32, message: &str) -> Self {
        self.lock().scripts.insert(
            name.to_owned(),
            Script::FailFirst {
                attempts,
                message: message.to_owned(),
            },
        );
        self
    }

    /// The step named `name` fails retryably with `message` on every
    /// execution, so its policy is exhausted.
    pub(crate) fn exhaust(self, name: &str, message: &str) -> Self {
        self.fail_first(name, u32::MAX, message)
    }

    /// The invocation is cancelled when the step named `name` is reached:
    /// the run ends with the SDK's 409 before the step runs.
    pub(crate) fn cancel_at(self, name: &str) -> Self {
        self.lock().scripts.insert(name.to_owned(), Script::Cancel);
        self
    }

    /// Runs `execution` as Restate would run a handler: once, and again from
    /// its first line, replaying every journaled completion, each time a
    /// step ends the execution with a retryable failure the policy re-executes
    /// (after the policy's delay, on the simulated clock), until the handler
    /// answers.
    pub(crate) async fn drive<T, F, Fut>(&self, mut execution: F) -> Result<T, HandlerError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, HandlerError>>,
    {
        loop {
            self.begin_execution();
            let mut handler = pin!(execution());
            let answer = poll_fn(|cx| match handler.as_mut().poll(cx) {
                Poll::Ready(answer) => Poll::Ready(Some(answer)),
                // A run that ended the execution returned `Pending` up the
                // chain having set the flag in the same poll.
                Poll::Pending if self.take_execution_ended() => Poll::Ready(None),
                Poll::Pending => Poll::Pending,
            })
            .await;
            if let Some(answer) = answer {
                return answer;
            }
            // Dropping `handler` is the abort; the loop re-executes.
        }
    }

    /// Every run call so far, in order, across executions.
    pub(crate) fn record(&self) -> Vec<RunRecord> {
        self.lock().record.clone()
    }

    /// The names of the journal's runs, in order: every step ever started,
    /// which is what `sys_journal`'s `Command: Run` rows show and what the
    /// run-name pin reads.
    pub(crate) fn journaled_names(&self) -> Vec<String> {
        self.lock()
            .journal
            .iter()
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// The journaled bytes of the run named `name`, when it completed with a
    /// value.
    pub(crate) fn journaled(&self, name: &str) -> Option<Vec<u8>> {
        self.lock()
            .journal
            .iter()
            .find(|(journaled, _)| journaled == name)
            .and_then(|(_, result)| result.clone().ok())
    }

    /// How many times the handler was executed.
    pub(crate) fn executions(&self) -> u32 {
        self.lock().execution
    }

    /// The simulated clock: the sum of every delay the policies made the
    /// handler wait.
    pub(crate) fn clock(&self) -> Duration {
        self.lock().clock
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("the fake runner's state")
    }

    fn begin_execution(&self) {
        let mut inner = self.lock();
        inner.position = 0;
        inner.execution += 1;
        inner.execution_ended = false;
    }

    fn take_execution_ended(&self) -> bool {
        std::mem::take(&mut self.lock().execution_ended)
    }

    /// What to do with a run that reached the fake: replay the journaled
    /// completion at this position, or start the step (its attempt counted),
    /// as the script says.
    fn begin(&self, name: &str, policy: StepPolicy) -> Begin {
        let mut inner = self.lock();
        let position = inner.position;
        inner.position += 1;
        let execution = inner.execution;
        if let Some((journaled, result)) = inner.journal.get(position) {
            assert_eq!(
                journaled, name,
                "journal mismatch at position {position}: the journal holds `{journaled}`, the handler ran `{name}` (a renamed, inserted or reordered step strands the invocation; ADR 0005)"
            );
            let result = result.clone();
            inner.record.push(RunRecord {
                name: name.to_owned(),
                policy,
                execution,
                attempt: 0,
                recorded: Recorded::Replayed(result.clone()),
            });
            return Begin::Replay(result);
        }
        let attempt = inner.attempts.entry(name.to_owned()).or_insert(0);
        *attempt += 1;
        assert!(
            *attempt < 10_000,
            "the step `{name}` was re-executed 10 000 times: its policy bounds nothing"
        );
        let attempt = *attempt;
        match inner.scripts.get(name) {
            Some(Script::Cancel) => {
                inner
                    .journal
                    .push((name.to_owned(), Err((CANCELLED, "cancelled".to_owned()))));
                inner.record.push(RunRecord {
                    name: name.to_owned(),
                    policy,
                    execution,
                    attempt,
                    recorded: Recorded::Cancelled,
                });
                Begin::Cancel
            }
            Some(Script::FailFirst { attempts, message }) if attempt <= *attempts => {
                Begin::Fail(message.clone())
            }
            _ => Begin::Run,
        }
    }

    /// Journals a step's value.
    fn settle_value(&self, name: &str, policy: StepPolicy, bytes: Vec<u8>) {
        let mut inner = self.lock();
        let execution = inner.execution;
        let attempt = inner.attempts[name];
        inner.journal.push((name.to_owned(), Ok(bytes.clone())));
        inner.record.push(RunRecord {
            name: name.to_owned(),
            policy,
            execution,
            attempt,
            recorded: Recorded::Journaled(bytes),
        });
    }

    /// Decides a step's retryable failure by the server's rule: re-execute
    /// after the delay, or exhausted.
    fn settle_failure(&self, name: &str, policy: StepPolicy, message: String) -> Settled {
        let mut inner = self.lock();
        let execution = inner.execution;
        let attempt = inner.attempts[name];
        let clock = inner.clock;
        let first_failure = *inner.first_failure.entry(name.to_owned()).or_insert(clock);
        let loop_duration = clock.saturating_sub(first_failure);
        let exhausted = policy
            .max_attempts
            .is_some_and(|max_attempts| max_attempts <= attempt)
            || policy
                .max_duration
                .is_some_and(|max_duration| max_duration <= loop_duration);
        if exhausted {
            inner
                .journal
                .push((name.to_owned(), Err((500, message.clone()))));
            inner.record.push(RunRecord {
                name: name.to_owned(),
                policy,
                execution,
                attempt,
                recorded: Recorded::Exhausted {
                    message: message.clone(),
                },
            });
            return Settled::Exhausted(TerminalError::new_with_code(500, message));
        }
        let delay = next_delay(policy, attempt);
        inner.clock += delay;
        inner.record.push(RunRecord {
            name: name.to_owned(),
            policy,
            execution,
            attempt,
            recorded: Recorded::Retried { message, delay },
        });
        inner.execution_ended = true;
        Settled::Retry
    }
}

/// The delay before the re-execution after the `attempt`th failure:
/// `initial_delay × factor^(attempt−1)`, capped at `max_delay` (the server's
/// `RetryPolicy::Exponential::next_retry`).
fn next_delay(policy: StepPolicy, attempt: u32) -> Duration {
    let exponent = i32::try_from(attempt.saturating_sub(1)).unwrap_or(i32::MAX);
    let delay = Duration::try_from_secs_f32(
        policy.initial_delay.as_secs_f32() * policy.factor.powi(exponent),
    )
    .unwrap_or(Duration::MAX);
    policy.max_delay.map_or(delay, |max| max.min(delay))
}

impl Runner for FakeRunner {
    fn invocation_id(&self) -> &str {
        &self.invocation_id
    }

    fn scope(&self) -> Option<&str> {
        self.scope.as_deref()
    }

    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn run(
        &self,
        name: String,
        policy: StepPolicy,
        step: Step,
    ) -> BoxFuture<'_, Result<Vec<u8>, TerminalError>> {
        Box::pin(async move {
            let outcome = match self.begin(&name, policy) {
                Begin::Replay(result) => {
                    return result
                        .map_err(|(code, message)| TerminalError::new_with_code(code, message));
                }
                Begin::Cancel => return Err(TerminalError::new_with_code(CANCELLED, "cancelled")),
                Begin::Fail(message) => Err(message),
                Begin::Run => step().await.map_err(|error| error.to_string()),
            };
            match outcome {
                Ok(bytes) => {
                    self.settle_value(&name, policy, bytes.clone());
                    Ok(bytes)
                }
                Err(message) => match self.settle_failure(&name, policy, message) {
                    Settled::Exhausted(error) => Err(error),
                    // The execution ends here: `drive` sees the flag on this
                    // very poll and drops the handler future.
                    Settled::Retry => std::future::pending().await,
                },
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    /// A step producing `bytes`, counting how often it ran in `runs`.
    fn counting_step(bytes: &'static [u8], runs: &Arc<AtomicU32>) -> Step {
        let runs = Arc::clone(runs);
        Box::new(move || {
            Box::pin(async move {
                runs.fetch_add(1, Ordering::SeqCst);
                Ok(bytes.to_vec())
            })
        })
    }

    /// A step that fails retryably with `message` every time.
    fn failing_step(message: &'static str) -> Step {
        Box::new(move || Box::pin(async move { Err(message.into()) }))
    }

    fn read() -> StepPolicy {
        StepPolicy {
            initial_delay: Duration::from_secs(1),
            factor: 2.0,
            max_delay: Some(Duration::from_secs(4)),
            max_attempts: Some(3),
            max_duration: None,
        }
    }

    /// A step that settles is journaled and, on a re-execution, replayed
    /// without running again; the record shows both.
    #[tokio::test]
    async fn a_settled_step_is_journaled_and_replayed_on_the_next_execution() {
        let runner = FakeRunner::object("ORD-1");
        let first_runs = Arc::new(AtomicU32::new(0));
        let flaky_runs = Arc::new(AtomicU32::new(0));
        let flaky_fails = Arc::new(AtomicU32::new(0));
        let answer = runner
            .drive(|| {
                let runner = &runner;
                let first_runs = Arc::clone(&first_runs);
                let flaky_runs = Arc::clone(&flaky_runs);
                let flaky_fails = Arc::clone(&flaky_fails);
                async move {
                    let first = runner
                        .run(
                            "first".to_owned(),
                            StepPolicy::ONCE,
                            counting_step(b"1", &first_runs),
                        )
                        .await?;
                    // Fails once on its own, then settles.
                    let step: Step = Box::new(move || {
                        Box::pin(async move {
                            flaky_runs.fetch_add(1, Ordering::SeqCst);
                            if flaky_fails.fetch_add(1, Ordering::SeqCst) == 0 {
                                Err("not yet".into())
                            } else {
                                Ok(b"2".to_vec())
                            }
                        })
                    });
                    let second = runner.run("second".to_owned(), read(), step).await?;
                    Ok::<_, HandlerError>((first, second))
                }
            })
            .await
            .expect("answers");
        assert_eq!(answer, (b"1".to_vec(), b"2".to_vec()));
        assert_eq!(runner.executions(), 2);
        assert_eq!(first_runs.load(Ordering::SeqCst), 1, "replayed, not re-run");
        assert_eq!(flaky_runs.load(Ordering::SeqCst), 2);
        assert_eq!(runner.journaled_names(), ["first", "second"]);
        assert_eq!(runner.clock(), Duration::from_secs(1));
        let record = runner.record();
        assert_eq!(record.len(), 4, "{record:#?}");
        let seen: Vec<(u32, u32, &Recorded)> = record
            .iter()
            .map(|run| (run.execution, run.attempt, &run.recorded))
            .collect();
        assert_eq!(
            seen,
            [
                (1, 1, &Recorded::Journaled(b"1".to_vec())),
                (
                    1,
                    1,
                    &Recorded::Retried {
                        message: "not yet".to_owned(),
                        delay: Duration::from_secs(1),
                    }
                ),
                (2, 0, &Recorded::Replayed(Ok(b"1".to_vec()))),
                (2, 2, &Recorded::Journaled(b"2".to_vec())),
            ]
        );
        assert_eq!(record[1].policy, read());
    }

    /// An injected failure spends the policy by the server's rule: three
    /// attempts under `max_attempts = 3`, delays 1 s then 2 s (the third
    /// failure is the exhaustion, no delay), and the run's terminal 500
    /// carries the last message.
    #[tokio::test]
    async fn an_exhausted_policy_ends_the_run_with_the_terminal_500() {
        let runner = FakeRunner::service().exhaust("read", "szamlazz.hu did not answer");
        let error = runner
            .drive(|| async {
                runner
                    .run("read".to_owned(), read(), failing_step("never runs"))
                    .await
                    .map_err(HandlerError::from)
            })
            .await
            .expect_err("exhausted");
        let text = error.as_ref() as &dyn std::error::Error;
        assert_eq!(
            text.to_string(),
            "Terminal error [500]: szamlazz.hu did not answer"
        );
        assert_eq!(runner.executions(), 3);
        assert_eq!(runner.clock(), Duration::from_secs(3));
        let ends: Vec<_> = runner
            .record()
            .into_iter()
            .map(|record| record.recorded)
            .collect();
        assert_eq!(
            ends,
            [
                Recorded::Retried {
                    message: "szamlazz.hu did not answer".to_owned(),
                    delay: Duration::from_secs(1),
                },
                Recorded::Retried {
                    message: "szamlazz.hu did not answer".to_owned(),
                    delay: Duration::from_secs(2),
                },
                Recorded::Exhausted {
                    message: "szamlazz.hu did not answer".to_owned(),
                },
            ]
        );
    }

    /// A policy with no attempt cap is bounded by `max_duration` on the
    /// simulated clock: the delays 1, 2, 4, 8, 16, 32 s sum past a minute at
    /// the seventh failure (the resolve policy's shape).
    #[tokio::test]
    async fn max_duration_bounds_a_policy_without_an_attempt_cap() {
        let policy = StepPolicy {
            initial_delay: Duration::from_secs(1),
            factor: 2.0,
            max_delay: Some(Duration::from_secs(60)),
            max_attempts: None,
            max_duration: Some(Duration::from_secs(60)),
        };
        let runner = FakeRunner::service().exhaust("account", "resolver down");
        runner
            .drive(|| async {
                runner
                    .run("account".to_owned(), policy, failing_step("never runs"))
                    .await
                    .map_err(HandlerError::from)
            })
            .await
            .expect_err("exhausted");
        assert_eq!(runner.executions(), 7);
        assert_eq!(runner.clock(), Duration::from_secs(63));
    }

    /// `fail_first` fails without running for exactly that many executions
    /// and lets the step run afterwards; the step's own failure counts like
    /// an injected one.
    #[tokio::test]
    async fn fail_first_injects_then_lets_the_step_run() {
        let runner = FakeRunner::service().fail_first("read", 2, "injected");
        let runs = Arc::new(AtomicU32::new(0));
        let bytes = runner
            .drive(|| async {
                runner
                    .run("read".to_owned(), read(), counting_step(b"ok", &runs))
                    .await
                    .map_err(HandlerError::from)
            })
            .await
            .expect("settles on the third execution");
        assert_eq!(bytes, b"ok");
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        assert_eq!(runner.executions(), 3);
    }

    /// A cancellation is the run's 409 before the step runs; the run is in
    /// the journal (its command was written) and nothing is re-executed.
    #[tokio::test]
    async fn cancel_at_ends_the_run_with_409_before_the_step_runs() {
        let runner = FakeRunner::object("ORD-1").cancel_at("hint");
        let runs = Arc::new(AtomicU32::new(0));
        let error = runner
            .drive(|| async {
                runner
                    .run("hint".to_owned(), read(), counting_step(b"x", &runs))
                    .await
                    .map_err(HandlerError::from)
            })
            .await
            .expect_err("cancelled");
        let text = error.as_ref() as &dyn std::error::Error;
        assert_eq!(text.to_string(), "Terminal error [409]: cancelled");
        assert_eq!(runs.load(Ordering::SeqCst), 0);
        assert_eq!(runner.executions(), 1);
        assert_eq!(runner.journaled_names(), ["hint"]);
        assert_eq!(runner.record()[0].recorded, Recorded::Cancelled);
    }

    /// A seeded entry is replayed at its position without the step running,
    /// and a step whose name differs from the seeded one at that position is
    /// the journal mismatch ADR 0005 warns of.
    #[tokio::test]
    async fn a_seeded_entry_replays_and_a_renamed_step_mismatches() {
        let runner = FakeRunner::service().seed("namespace", br#""acct""#);
        let runs = Arc::new(AtomicU32::new(0));
        let bytes = runner
            .drive(|| async {
                runner
                    .run(
                        "namespace".to_owned(),
                        StepPolicy::ONCE,
                        counting_step(b"never", &runs),
                    )
                    .await
                    .map_err(HandlerError::from)
            })
            .await
            .expect("replayed");
        assert_eq!(bytes, br#""acct""#);
        assert_eq!(runs.load(Ordering::SeqCst), 0);
        assert_eq!(
            runner.record()[0].recorded,
            Recorded::Replayed(Ok(br#""acct""#.to_vec()))
        );

        let renamed = FakeRunner::service().seed("namespace", br#""acct""#);
        let mismatch = tokio::spawn(async move {
            renamed
                .drive(|| async {
                    renamed
                        .run(
                            "ns".to_owned(),
                            StepPolicy::ONCE,
                            counting_step(b"never", &Arc::new(AtomicU32::new(0))),
                        )
                        .await
                        .map_err(HandlerError::from)
                })
                .await
        })
        .await
        .expect_err("the mismatch panics");
        let payload = mismatch.into_panic();
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| {
                payload
                    .downcast_ref::<&str>()
                    .map(|text| (*text).to_owned())
            })
            .unwrap_or_default();
        assert!(
            message.contains("journal mismatch at position 0"),
            "{message}"
        );
    }
}
