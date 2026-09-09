//! An ingress reply as the scenarios read it ([`Reply`]): the harness crate's
//! reply with the worker's structured fault ([`Fault`]) decoded out of the
//! ingress error envelope ([`Reply::fault`]).

use std::ops::Deref;

use restate_szamlazz::contract::Fault;

/// The harness crate's [`Reply`](restate_e2e_harness::Reply) (status, body,
/// invocation id, error source; dereferenced to) with the fault decoded into
/// the contract's [`Fault`].
#[derive(Debug)]
pub(crate) struct Reply(pub(crate) restate_e2e_harness::Reply);

impl Deref for Reply {
    type Target = restate_e2e_harness::Reply;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Reply {
    /// The structured fault inside the ingress error envelope, asserting the
    /// envelope the crate README documents (*Faults*): the body is
    /// Restate's `{"code": <HTTP status>, "message": "<string>", "source":
    /// "invocation"}`, `x-restate-error-source` is `invocation`, and the
    /// worker's fault is the JSON **string** in `message` (the handler's
    /// `TerminalError` message), parsed a second time.
    pub(crate) fn fault(&self) -> Fault {
        self.0.fault()
    }
}
