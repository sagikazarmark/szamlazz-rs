//! Ready-made async client built on [`reqwest`] (feature `client-reqwest`).
//!
//! A thin shell around the I/O-free core: every operation goes through
//! [`Client::send`], which works with any [`AgentRequest`] type. The client
//! owns what the core leaves to the transport: the endpoint URL, the
//! `JSESSIONID` session (through reqwest's cookie store), timeouts, TLS, and
//! the redirect policy.
//!
//! ```no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use szamlazz_agent::client::Client;
//! use szamlazz_agent::{Credentials, ops::taxpayer::QueryTaxpayer};
//!
//! let client = Client::new(Credentials::agent_key("your-agent-key"))?;
//! let taxpayer = client.send(&QueryTaxpayer::new("12345678")?).await?;
//! # Ok(())
//! # }
//! ```

use crate::credentials::Credentials;
use crate::error::{ApiError, OutcomeClass, ParseError, RequestError, ResponseError};
use crate::wire::{AgentRequest, ENDPOINT, RawResponse};

/// Failure of a Számla Agent call made through [`Client`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ClientError {
    /// The request violates the Számla Agent wire contract.
    #[error(transparent)]
    Request(#[from] RequestError),
    /// szamlazz.hu reported an error.
    #[error(transparent)]
    Api(#[from] ApiError),
    /// The response could not be parsed.
    #[error(transparent)]
    Parse(#[from] ParseError),
    /// Számla Agent reported temporary system unavailability.
    #[error("szamlazz.hu is temporarily unavailable: {0}")]
    ServiceUnavailable(String),
    /// The endpoint answered with a non-2xx status and no `szlahu_*` header:
    /// a proxy, a CDN or a misconfigured URL spoke, not szamlazz.hu
    /// ([`ResponseError::HttpStatus`]).
    #[error("HTTP {status} from the endpoint with no szamlazz.hu answer: {body}")]
    HttpStatus {
        /// The HTTP status.
        status: u16,
        /// A bounded excerpt of the body.
        body: String,
    },
    /// The HTTP request itself failed.
    ///
    /// Retry with care: invoice creation has no idempotency key, so a timeout
    /// after the server already issued the document means a retry issues a
    /// duplicate. Receipt creation call ids prevent duplicate issuance by
    /// returning error 338 on reuse, but do not replay the original success.
    /// Receipt storno repeat semantics are not established.
    /// [`ClientError::outcome_class`] says which failures leave the outcome
    /// open; use the [operation recovery table](crate::error#recovery).
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),
}

impl ClientError {
    /// What this failure says about the document the request asked for: may
    /// one exist despite the error? See [`OutcomeClass`].
    ///
    /// A request refused before it was sent ([`ClientError::Request`]) is
    /// [`OutcomeClass::Rejected`]: nothing reached szamlazz.hu. An API error's
    /// class is its [`ErrorCode::outcome_class`](crate::ErrorCode::outcome_class).
    /// A transport failure, unavailability (`szlahu_down`), an answer from
    /// the endpoint rather than szamlazz.hu and an unparseable response are
    /// [`OutcomeClass::Unknown`]: the request may have been acted on, and the
    /// caller follows the [operation recovery table](crate::error#recovery)
    /// before sending again. An empty immediate query cannot rule out an
    /// earlier send still in flight.
    #[must_use]
    pub fn outcome_class(&self) -> OutcomeClass {
        match self {
            Self::Request(_) => OutcomeClass::Rejected,
            Self::Api(api) => api.code.outcome_class(),
            Self::Parse(_)
            | Self::ServiceUnavailable(_)
            | Self::HttpStatus { .. }
            | Self::Transport(_) => OutcomeClass::Unknown,
        }
    }
}

impl From<ResponseError> for ClientError {
    fn from(error: ResponseError) -> Self {
        match error {
            ResponseError::Api(api) => Self::Api(api),
            ResponseError::Parse(parse) => Self::Parse(parse),
            ResponseError::ServiceUnavailable(message) => Self::ServiceUnavailable(message),
            ResponseError::HttpStatus { status, body } => Self::HttpStatus { status, body },
        }
    }
}

/// Configures a [`Client`].
#[derive(Debug, Default)]
pub struct ClientBuilder {
    credentials: Option<Credentials>,
    endpoint: Option<String>,
    http: Option<reqwest::Client>,
}

impl ClientBuilder {
    /// Sets the credentials injected into every request. Required.
    #[must_use]
    pub fn credentials(mut self, credentials: Credentials) -> Self {
        self.credentials = Some(credentials);
        self
    }

    /// Overrides the endpoint URL, for pointing tests at a mock server.
    /// szamlazz.hu has no separate sandbox host; test mode is an account
    /// setting. Checked at [`build`](Self::build): a string that is not an
    /// `http` or `https` URL is [`BuildError::InvalidEndpoint`] there, not a
    /// transport error on every send.
    #[must_use]
    pub fn endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    /// Supplies a pre-configured [`reqwest::Client`] (proxies, timeouts, …).
    ///
    /// Enable `.cookie_store(true)` (as the default client does) so the
    /// `JSESSIONID` session cookie is reused and consecutive requests skip
    /// re-authentication; without it every request logs in again.
    #[must_use]
    pub fn http_client(mut self, http: reqwest::Client) -> Self {
        self.http = Some(http);
        self
    }

    /// Builds the client.
    ///
    /// # Errors
    ///
    /// Fails when no credentials were supplied, the endpoint is not an
    /// `http` or `https` URL, or the underlying HTTP client cannot be
    /// constructed.
    pub fn build(self) -> Result<Client, BuildError> {
        let credentials = self.credentials.ok_or(BuildError::MissingCredentials)?;
        let endpoint = parse_endpoint(self.endpoint.as_deref().unwrap_or(ENDPOINT))?;
        let http = match self.http {
            Some(http) => http,
            None => default_http_client()?,
        };

        Ok(Client {
            http,
            credentials,
            endpoint,
        })
    }
}

/// The endpoint as a URL, or why the string is not one.
fn parse_endpoint(endpoint: &str) -> Result<reqwest::Url, BuildError> {
    let invalid = |message: String| BuildError::InvalidEndpoint {
        endpoint: endpoint.to_owned(),
        message,
    };
    let url = reqwest::Url::parse(endpoint).map_err(|error| invalid(error.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(invalid(format!(
            "scheme {} is not http or https",
            url.scheme()
        )));
    }
    if url.host_str().is_none() {
        return Err(invalid("no host".to_owned()));
    }

    Ok(url)
}

/// [`ClientBuilder::build`] failure.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BuildError {
    /// No credentials were supplied.
    #[error("credentials are required")]
    MissingCredentials,
    /// The endpoint is not an `http` or `https` URL with a host.
    #[error("invalid endpoint {endpoint:?}: {message}")]
    InvalidEndpoint {
        /// The endpoint as given.
        endpoint: String,
        /// What is wrong with it.
        message: String,
    },
    /// The underlying HTTP client could not be constructed.
    #[error("failed to build HTTP client: {0}")]
    Http(#[from] reqwest::Error),
}

/// How long the default HTTP client waits for one request before giving up
/// (native targets; on wasm the runtime owns timeouts).
///
/// The detailed account record reports a ≥57-second stalled create with no
/// issuance found; the broader delayed-issuance assertion has unresolved
/// provenance (see [recovery evidence](crate::error#retry-limit-and-evidence)).
/// A timeout does not cancel server work. A caller must allow in-flight work
/// plus a margin before considering another send, and reconcile identity;
/// an immediate empty query alone is insufficient. Exported so a delay floor
/// can be derived rather than copied. A client built with
/// [`ClientBuilder::http_client`] carries its own timeout instead.
pub const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_mins(1);

/// On native targets the client keeps the `JSESSIONID` session cookie via
/// reqwest's cookie store, skipping re-authentication (sessions live 90
/// minutes), bounds each request to [`REQUEST_TIMEOUT`] so a stalled server
/// cannot hang the call forever, and does not follow redirects: the endpoint
/// never redirects, and following one would silently convert the multipart
/// POST into a body-less GET. On wasm the browser/runtime owns cookies and
/// redirect handling.
fn default_http_client() -> Result<reqwest::Client, reqwest::Error> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        reqwest::Client::builder()
            .cookie_store(true)
            .timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
    }
    #[cfg(target_arch = "wasm32")]
    {
        Ok(reqwest::Client::new())
    }
}

/// An async Számla Agent client.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    credentials: Credentials,
    endpoint: reqwest::Url,
}

impl Client {
    /// A client with default HTTP settings.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying HTTP client cannot be constructed.
    pub fn new(credentials: Credentials) -> Result<Self, BuildError> {
        Self::builder().credentials(credentials).build()
    }

    /// Starts configuring a client.
    #[must_use]
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    /// Sends any Számla Agent operation and parses its typed response.
    ///
    /// # Errors
    ///
    /// Returns an error if the request violates its wire contract, transport
    /// fails, szamlazz.hu reports an error or unavailability, or the response
    /// cannot be parsed.
    pub async fn send<R: AgentRequest>(&self, request: &R) -> Result<R::Response, ClientError> {
        let wire = request.to_wire(&self.credentials)?;

        let response = self
            .http
            .post(self.endpoint.clone())
            .header(reqwest::header::CONTENT_TYPE, wire.content_type)
            .body(wire.body)
            .send()
            .await?;

        let status = response.status().as_u16();
        let headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_owned(),
                    String::from_utf8_lossy(value.as_bytes()).into_owned(),
                )
            })
            .collect();
        let body = response.bytes().await?;

        let raw = RawResponse::new(headers, body.to_vec()).with_status(status);

        Ok(request.parse(&raw)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A malformed endpoint is refused when the client is built, naming the
    /// string, not on every send as a transport error.
    #[test]
    fn a_malformed_endpoint_fails_at_build() {
        assert!(
            Client::builder()
                .credentials(Credentials::agent_key("key"))
                .build()
                .is_ok(),
            "the default endpoint"
        );
        for endpoint in [
            "not a url",
            "ftp://example.test/",
            "http://",
            "mailto:x@example.test",
        ] {
            let error = Client::builder()
                .credentials(Credentials::agent_key("key"))
                .endpoint(endpoint)
                .build()
                .expect_err(endpoint);
            match error {
                BuildError::InvalidEndpoint {
                    endpoint: given, ..
                } => assert_eq!(given, endpoint),
                other => panic!("{endpoint}: expected InvalidEndpoint, got {other:?}"),
            }
        }
        assert!(matches!(
            Client::builder().endpoint("http://127.0.0.1:1/").build(),
            Err(BuildError::MissingCredentials)
        ));
    }
}
