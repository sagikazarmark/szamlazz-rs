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
/// submit without waiting. Segments are logical values, never pre-encoded:
/// [`Self::path`] percent-encodes them, so the key is the same value that
/// [`Target`](crate::Target) selects.
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

    /// The ingress path, with each logical segment percent-encoded. A literal
    /// `%2F` in a key becomes `%252F`, distinct from a slash (`%2F`).
    ///
    /// Panics for a segment equal to `.` or `..`: HTTP URL parsing normalizes
    /// those even when percent-encoded, so they cannot be sent faithfully.
    #[must_use]
    pub fn path(&self) -> String {
        let mut path = String::from("/restate");
        if let Some(scope) = self.scope {
            path.push_str("/scope/");
            push_segment(&mut path, scope);
        }
        path.push_str(match self.mode {
            Mode::Call => "/call/",
            Mode::Send => "/send/",
        });
        push_segment(&mut path, self.service);
        if let Some(key) = self.key {
            path.push('/');
            push_segment(&mut path, key);
        }
        path.push('/');
        push_segment(&mut path, self.handler);
        path
    }
}

/// RFC 3986 unreserved bytes can appear literally; encode everything else,
/// including each byte of UTF-8, so a value stays exactly one path segment.
fn push_segment(path: &mut String, segment: &str) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    assert!(
        !matches!(segment, "." | ".."),
        "an ingress path segment cannot be {segment:?}: HTTP URL parsing normalizes dot segments"
    );
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            path.push(char::from(byte));
        } else {
            path.push('%');
            path.push(char::from(HEX[usize::from(byte >> 4)]));
            path.push(char::from(HEX[usize::from(byte & 15)]));
        }
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
    use serde::Deserialize;
    use serde_json::json;

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

    #[test]
    fn all_segments_are_logical_values_encoded_once() {
        assert_eq!(
            Call::object("Svc/name", "O'Brien/é ?#%2F", "read?all")
                .scoped("scope#one")
                .send()
                .path(),
            "/restate/scope/scope%23one/send/Svc%2Fname/O%27Brien%2F%C3%A9%20%3F%23%252F/read%3Fall"
        );
        for key in [".", ".."] {
            assert!(std::panic::catch_unwind(|| Call::object("Svc", key, "h").path()).is_err());
        }
    }

    /// A fault reply as the ingress wraps a handler's `TerminalError`.
    fn fault_reply() -> Reply {
        Reply {
            status: 422,
            body: json!({
                "code": 422,
                "message": "{\"code\":\"refused\",\"message\":\"no\"}",
                "source": "invocation",
            }),
            invocation_id: Some("inv_1".to_owned()),
            error_source: Some("invocation".to_owned()),
        }
    }

    /// The consumer-side shape of the fault inside.
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Fault {
        code: String,
        message: String,
    }

    /// The message `reply.fault::<Fault>()` refuses with.
    fn refused(reply: &Reply) -> String {
        *std::panic::catch_unwind(|| reply.fault::<Fault>())
            .expect_err("refused")
            .downcast::<String>()
            .expect("a message")
    }

    /// The envelope's three marks are asserted, then the string in `message`
    /// is decoded a second time into the caller's type.
    #[test]
    fn a_fault_is_decoded_out_of_the_envelope() {
        assert_eq!(
            fault_reply().fault::<Fault>(),
            Fault {
                code: "refused".to_owned(),
                message: "no".to_owned(),
            }
        );
        assert_eq!(fault_reply().invocation_id(), "inv_1");
    }

    /// Each mark missing is its own refusal, naming what was expected: a
    /// `code` that is not the status, a `source` that is not the invocation's,
    /// the header absent, a `message` that is not a string, one that is not
    /// the caller's type.
    #[test]
    fn an_envelope_off_in_any_mark_is_refused() {
        let mut off = fault_reply();
        off.body["code"] = json!(500);
        assert!(refused(&off).contains("the envelope's code is the HTTP status"));

        let mut off = fault_reply();
        off.body["source"] = json!("ingress");
        assert!(refused(&off).contains("invocation's terminal error"));

        let mut off = fault_reply();
        off.error_source = None;
        assert!(refused(&off).contains("x-restate-error-source"));

        let mut off = fault_reply();
        off.body["message"] = json!({ "code": "refused" });
        assert!(refused(&off).contains("an error envelope with a message"));

        let mut off = fault_reply();
        off.body["message"] = json!("not json");
        assert!(refused(&off).contains("a structured fault"));

        let no_id = Reply {
            invocation_id: None,
            ..fault_reply()
        };
        let message = *std::panic::catch_unwind(|| no_id.invocation_id().to_owned())
            .expect_err("refused")
            .downcast::<String>()
            .expect("a message");
        assert!(message.contains("no x-restate-id"), "{message}");
    }
}
