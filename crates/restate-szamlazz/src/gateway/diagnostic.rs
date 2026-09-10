//! Allowlisted diagnostic projection. Never format an upstream error or source.
//! The strings returned here are safe to retain in run failures and outcomes.

use szamlazz_agent::{ClientError, ErrorCode, ParseError};

use super::Unanswered;

pub(super) fn credential_message(code: &str) -> Option<&'static str> {
    match ErrorCode::from(code) {
        ErrorCode::InvalidCredentials => Some("credentials rejected: invalid credentials"),
        ErrorCode::BrowserSessionActive => Some("credentials rejected: browser session active"),
        ErrorCode::LoginBlocked => Some("credentials rejected: login blocked"),
        ErrorCode::MultipleAccounts => Some("credentials rejected: multiple-account access"),
        _ => None,
    }
}

pub(super) fn exchange(operation: &str, error: &ClientError) -> Unanswered {
    let category = match error {
        ClientError::ServiceUnavailable(_) => {
            return Unanswered::Unavailable(format!("{operation}: szlahu_down"));
        }
        ClientError::HttpStatus { status, .. } => {
            return Unanswered::Transport(format!("{operation}: HTTP {status}"));
        }
        ClientError::Parse(parse) => match parse {
            ParseError::Xml(_) => "parse: invalid XML",
            ParseError::Missing(_) => "parse: missing response field",
            ParseError::Invalid { .. } => "parse: invalid response value",
            ParseError::Base64(_) => "parse: invalid base64",
            ParseError::UnexpectedBody(_) => "parse: unexpected response body",
            _ => "parse: unclassified failure",
        },
        ClientError::Transport(transport) => {
            let category = if transport.is_timeout() {
                "timeout"
            } else if transport.is_connect() {
                "connection"
            } else if transport.is_redirect() {
                "redirect"
            } else if transport.is_body() {
                "body transfer"
            } else if transport.is_decode() {
                "decoding"
            } else if transport.is_builder() {
                "request construction"
            } else if transport.is_request() {
                "request exchange"
            } else {
                "unclassified"
            };
            let status = transport
                .status()
                .map(|status| format!("; HTTP {}", status.as_u16()))
                .unwrap_or_default();
            return Unanswered::Transport(format!("{operation}: transport: {category}{status}"));
        }
        // Answers and request refusals are split off by callers. Future client
        // variants retain uncertainty without opting their Display into storage.
        _ => "unclassified exchange failure",
    };
    Unanswered::Transport(format!("{operation}: {category}"))
}
