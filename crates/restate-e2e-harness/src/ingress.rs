//! The ingress: a call to a handler ([`Call`], the URL grammar written once),
//! its reply ([`Reply`]) and the structured fault inside Restate's error
//! envelope ([`Reply::fault`], the caller's type decoded a second time out of
//! the envelope's `message`).

use serde::de::DeserializeOwned;
use serde_json::Value;

/// Whether the ingress waits for the handler's answer or only accepts the
/// invocation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mode {
    /// `/restate/call/…`: the reply is the handler's answer (or fault).
    #[default]
    Call,
    /// `/restate/send/…`: a 202 with the accepted invocation's id; the
    /// handler runs on.
    Send,
}

/// A call to a handler through the ingress, as Restate's URL grammar has it:
/// `/restate/call/{service}/{handler}` for a service,
/// `/restate/call/{service}/{key}/{handler}` for a Virtual Object,
/// `/restate/scope/{scope}/…` under a scope, `send` in place of `call` to
/// submit without waiting. Every segment is inserted as given: a key with a
/// character a URL path cannot carry is the caller's to encode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Call<'a> {
    /// The service.
    pub service: &'a str,
    /// The handler.
    pub handler: &'a str,
    /// The Virtual Object key; `None` for a service handler.
    pub key: Option<&'a str>,
    /// The scope (`/restate/scope/{scope}/…`); `None` for an unscoped call.
    pub scope: Option<&'a str>,
    /// Call, or send.
    pub mode: Mode,
}

impl<'a> Call<'a> {
    /// `handler` of the service `service`, unscoped, waited for.
    #[must_use]
    pub const fn service(service: &'a str, handler: &'a str) -> Self {
        Self {
            service,
            handler,
            key: None,
            scope: None,
            mode: Mode::Call,
        }
    }

    /// `handler` of the Virtual Object `key` of `service`, unscoped, waited
    /// for.
    #[must_use]
    pub const fn object(service: &'a str, key: &'a str, handler: &'a str) -> Self {
        Self {
            service,
            handler,
            key: Some(key),
            scope: None,
            mode: Mode::Call,
        }
    }

    /// The same call under `scope`.
    #[must_use]
    pub const fn scoped(self, scope: &'a str) -> Self {
        Self {
            scope: Some(scope),
            ..self
        }
    }

    /// The same call submitted without waiting (`/restate/send/…`).
    #[must_use]
    pub const fn send(self) -> Self {
        Self {
            mode: Mode::Send,
            ..self
        }
    }

    /// The ingress path.
    #[must_use]
    pub fn path(&self) -> String {
        let mut path = String::from("/restate");
        if let Some(scope) = self.scope {
            path.push_str("/scope/");
            path.push_str(scope);
        }
        path.push_str(match self.mode {
            Mode::Call => "/call/",
            Mode::Send => "/send/",
        });
        path.push_str(self.service);
        if let Some(key) = self.key {
            path.push('/');
            path.push_str(key);
        }
        path.push('/');
        path.push_str(self.handler);
        path
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    /// The URL grammar, once: a service or an object, unscoped or under a
    /// scope, called or sent.
    #[test]
    fn a_call_renders_restates_ingress_path() {
        assert_eq!(
            Call::service("Inv.Api", "probe").path(),
            "/restate/call/Inv.Api/probe"
        );
        assert_eq!(
            Call::object("Inv.Stock", "SKU-1", "reserve").path(),
            "/restate/call/Inv.Stock/SKU-1/reserve"
        );
        assert_eq!(
            Call::object("Inv.Stock", "SKU-1", "reserve")
                .scoped("acme")
                .path(),
            "/restate/scope/acme/call/Inv.Stock/SKU-1/reserve"
        );
        assert_eq!(
            Call::service("Inv.Api", "probe").scoped("acme").path(),
            "/restate/scope/acme/call/Inv.Api/probe"
        );
        assert_eq!(
            Call::object("Inv.Stock", "SKU-1", "reserve").send().path(),
            "/restate/send/Inv.Stock/SKU-1/reserve"
        );
        assert_eq!(
            Call::object("Inv.Stock", "SKU-1", "reserve")
                .scoped("acme")
                .send()
                .path(),
            "/restate/scope/acme/send/Inv.Stock/SKU-1/reserve"
        );
        assert_eq!(Mode::default(), Mode::Call);
    }
}
