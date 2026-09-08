//! An ingress reply ([`Reply`]) and the structured fault inside Restate's
//! error envelope (the contract's [`Fault`], decoded a second time out of
//! the envelope's `message`).

use restate_szamlazz::contract::Fault;
use serde_json::Value;

/// An ingress reply: the status, the parsed body, the invocation id the
/// ingress reports in `x-restate-id` and the `x-restate-error-source` header
/// of an error reply.
#[derive(Debug)]
pub(crate) struct Reply {
    pub(crate) status: u16,
    pub(crate) body: Value,
    pub(crate) invocation_id: Option<String>,
    pub(crate) error_source: Option<String>,
}

impl Reply {
    pub(crate) fn invocation_id(&self) -> &str {
        self.invocation_id
            .as_deref()
            .unwrap_or_else(|| panic!("no x-restate-id on the reply: {}", self.body))
    }

    /// The structured fault inside the ingress error envelope, asserting the
    /// envelope the endpoint README documents (*Faults*): the body is
    /// Restate's `{"code": <HTTP status>, "message": "<string>", "source":
    /// "invocation"}`, `x-restate-error-source` is `invocation`, and the
    /// worker's fault is the JSON **string** in `message` (the handler's
    /// `TerminalError` message), parsed a second time.
    pub(crate) fn fault(&self) -> Fault {
        assert_eq!(
            self.body["code"].as_u64(),
            Some(u64::from(self.status)),
            "the envelope's code is the HTTP status: {}",
            self.body
        );
        assert_eq!(
            self.body["source"], "invocation",
            "a fault is the invocation's terminal error: {}",
            self.body
        );
        assert_eq!(
            self.error_source.as_deref(),
            Some("invocation"),
            "x-restate-error-source marks the fault as the worker's: {}",
            self.body
        );
        let message = self.body["message"]
            .as_str()
            .unwrap_or_else(|| panic!("an error envelope with a message: {}", self.body));
        serde_json::from_str(message)
            .unwrap_or_else(|error| panic!("a structured fault ({error}): {message}"))
    }
}
