//! An ingress reply ([`Reply`]) and the structured fault inside Restate's
//! error envelope ([`Reply::fault`], the caller's type decoded a second time
//! out of the envelope's `message`).

use serde::de::DeserializeOwned;
use serde_json::Value;

/// An ingress reply: the status, the parsed body, the invocation id the
/// ingress reports in `x-restate-id` and the `x-restate-error-source` header
/// of an error reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    /// The HTTP status.
    pub status: u16,
    /// The body, parsed as JSON when it is; a string otherwise.
    pub body: Value,
    /// `x-restate-id`: the invocation the ingress ran or attached to.
    pub invocation_id: Option<String>,
    /// `x-restate-error-source` of an error reply: `invocation` for a
    /// handler's terminal error, `ingress` for the ingress's own.
    pub error_source: Option<String>,
}

impl Reply {
    /// The invocation id; panics when the reply carries none.
    #[must_use]
    pub fn invocation_id(&self) -> &str {
        self.invocation_id
            .as_deref()
            .unwrap_or_else(|| panic!("no x-restate-id on the reply: {}", self.body))
    }

    /// The structured fault inside the ingress error envelope, asserting the
    /// envelope Restate's ingress wraps a handler's `TerminalError` in: the
    /// body is `{"code": <HTTP status>, "message": "<string>", "source":
    /// "invocation"}`, `x-restate-error-source` is `invocation`, and the
    /// handler's fault is the JSON **string** in `message` (the
    /// `TerminalError`'s message), parsed a second time into `F`.
    #[must_use]
    pub fn fault<F: DeserializeOwned>(&self) -> F {
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
            "x-restate-error-source marks the fault as the handler's: {}",
            self.body
        );
        let message = self.body["message"]
            .as_str()
            .unwrap_or_else(|| panic!("an error envelope with a message: {}", self.body));
        serde_json::from_str(message)
            .unwrap_or_else(|error| panic!("a structured fault ({error}): {message}"))
    }
}
