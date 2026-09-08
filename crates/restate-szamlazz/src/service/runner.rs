//! The seam between the handler bodies and Restate's `ctx.run`: an
//! object-safe [`Runner`] the three SDK context types implement, so that
//! every context-touching helper (`durable`) and every handler body
//! (`entry`, `create`, `storno`, `agent`) is written once over
//! `&dyn Runner`, and a fake (`FakeRunner`, tests only) drives the same
//! bodies without a server.
//!
//! Why a trait object and not a generic: the SDK's context traits are sealed,
//! and a generic `async fn` over them trips rust-lang/rust#100013 (`Send` is
//! not general enough) inside the macro-generated dispatcher, which is why
//! the helpers were stamped out per context type by a macro (#68 J-05-05).
//! A trait object with boxed futures has no higher-ranked lifetime for the
//! compiler to lose: each `impl Runner for <context>` is a concrete type, so
//! the auto-trait leakage that proves the SDK's run future `Send` works as it
//! does in a hand-written handler (#133).
//!
//! What crosses the seam is bytes. The SDK journals a run's completion as
//! exactly the bytes `Serialize::serialize` produces, and `Json<T>` produces
//! `serde_json::to_vec(&t)`; a runner journaling a `Vec<u8>` (whose SDK
//! serialisation is the identity) that holds `serde_json::to_vec(&t)` writes
//! the **byte-identical** entry, so the seam changes no journal (ADR 0005;
//! the e2e asserts it on the raw run-result bytes). The typed helpers over
//! the seam (`durable::run_once` and its siblings) encode and decode; a
//! [`Journaled`](super::support::Journaled) type is what they carry.

use std::future::Future;
use std::pin::Pin;

use restate_sdk::context::{ContextSideEffects, RunFuture as _};
use restate_sdk::errors::{HandlerError, TerminalError};
use restate_sdk::prelude::{Context, ObjectContext, SharedObjectContext};

use crate::config::StepPolicy;

#[cfg(test)]
mod fake;
#[cfg(test)]
pub(crate) use fake::{FakeRunner, Recorded, RunRecord};

/// A boxed `Send` future: what an object-safe async method returns.
pub(crate) type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A boxed error: a step's retryable failure, as the SDK's `HandlerError`
/// carries one.
pub(crate) type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// One durable step's closure: produces the bytes to journal, or fails
/// retryably (the step's own "not settled" error, which the run retry
/// policy re-executes). `'static`, as the SDK requires of a run closure: a
/// step owns what it needs (an `Arc<Gateway>`, the request it sends), so
/// that a re-executed handler rebuilds it from its journal.
pub(crate) type Step = Box<dyn FnOnce() -> BoxFuture<'static, Result<Vec<u8>, BoxError>> + Send>;

/// What a handler execution runs its durable steps on: Restate's context, or
/// the fake.
///
/// One method per fact a handler reads off its context (the invocation id,
/// the scope, the Virtual Object key) and one for `ctx.run`: journal the
/// bytes `step` produces under `name`, re-executing it under `policy` while
/// it fails retryably, and replay the journaled bytes on a re-execution
/// without calling `step`. The three SDK contexts implement it below; the
/// handler bodies never see which.
pub(crate) trait Runner: Send + Sync {
    /// The invocation id, what the ingress returns as `x-restate-id`.
    fn invocation_id(&self) -> &str;

    /// The scope the request arrived under, as the SDK saw it: `None` when
    /// the request carried none, or when the server did not forward it.
    fn scope(&self) -> Option<&str>;

    /// The Virtual Object key: `Some` on the object contexts, `None` on the
    /// stateless service.
    fn key(&self) -> Option<&str>;

    /// `ctx.run(step).name(name).retry_policy(policy)`: the journaled bytes
    /// of the step, or the `TerminalError` the run ended with, exhaustion of
    /// the policy (500, the last failure's message) or cancellation (409).
    ///
    /// A retryable failure with executions left ends the whole handler
    /// execution: the future never resolves, the SDK reports the failure and
    /// the delay to the server, and the handler is re-executed from its
    /// first line after the delay, replaying every journaled entry.
    fn run(
        &self,
        name: String,
        policy: StepPolicy,
        step: Step,
    ) -> BoxFuture<'_, Result<Vec<u8>, TerminalError>>;
}

/// Implements [`Runner`] for one SDK context type: the three facts read off
/// the context's own methods, `run` as `ctx.run` with the closure's failure
/// as the SDK's retryable `HandlerError`.
macro_rules! sdk_runner {
    ($ctx:ident, |$this:ident| $key:expr) => {
        impl Runner for $ctx<'_> {
            fn invocation_id(&self) -> &str {
                $ctx::invocation_id(self)
            }

            fn scope(&self) -> Option<&str> {
                $ctx::scope(self)
            }

            fn key(&self) -> Option<&str> {
                let $this = self;
                $key
            }

            fn run(
                &self,
                name: String,
                policy: StepPolicy,
                step: Step,
            ) -> BoxFuture<'_, Result<Vec<u8>, TerminalError>> {
                Box::pin(async move {
                    ContextSideEffects::run(self, move || async move {
                        step().await.map_err(HandlerError::from)
                    })
                    .name(name)
                    .retry_policy(policy.into_sdk())
                    .await
                })
            }
        }
    };
}

sdk_runner!(ObjectContext, |ctx| Some(ctx.key()));
sdk_runner!(SharedObjectContext, |ctx| Some(ctx.key()));
sdk_runner!(Context, |_ctx| None);
