//! Ready-made async client built on [`reqwest`] (feature `client-reqwest`).
//!
//! A thin shell around the I/O-free core: every operation goes through
//! [`Client::send`], which works with any [`AgentRequest`] type. The client
//! owns what the core leaves to the transport: the endpoint URL, the
//! `JSESSIONID` session (through reqwest's cookie store), timeouts, TLS, and
//! the redirect policy.
//!
//! See [`Client`] for native session ownership and refresh, and the
//! [platform boundary](crate#features) for browser Fetch limitations.
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
    /// A non-2xx status reached before body interpretation, after checking
    /// nonblank `szlahu_down` and error-code headers. Success-number or other
    /// headers do not bypass it; the status does not identify who answered
    /// ([`ResponseError::HttpStatus`]).
    #[error("HTTP {status} before body interpretation: {body}")]
    HttpStatus {
        /// The HTTP status.
        status: u16,
        /// A bounded excerpt of the body.
        body: String,
    },
    /// The HTTP request itself failed.
    ///
    /// Retry with care: invoice creation has no idempotency key, so a timeout
    /// after the server already issued the document means a retry can issue a
    /// duplicate. Receipt creation call ids prevent duplicate issuance by
    /// returning error 338 on reuse, but do not replay the original success.
    /// Receipt storno of an already reversed receipt is documented as a refusal;
    /// a storno-specific call-ID/338 guarantee is not established.
    /// [`ClientError::outcome_class`] says which failures leave the outcome
    /// open; use the [operation recovery table](crate::error#recovery).
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),
    /// Status and headers arrived, but the response body did not complete.
    /// The retained evidence is not a completed response or a success verdict.
    #[error(transparent)]
    IncompleteResponse(Box<IncompleteResponse>),
}

/// Received HTTP evidence alongside a failed body transfer.
///
/// Even a number or refusal header does not settle an incomplete exchange:
/// unread body content may contradict it. Inspect these fields for diagnostics
/// and reconciliation, never pass an invented empty body to a response parser.
/// No partial body is retained. `Debug` lists header names only; the explicitly
/// accessed headers may contain session cookies and sensitive document data.
#[derive(thiserror::Error)]
#[error("incomplete HTTP {status} response: {source}")]
#[non_exhaustive]
pub struct IncompleteResponse {
    /// The received HTTP status.
    pub status: u16,
    /// Received headers, preserving repeated values and their original bytes.
    /// Vendor textual headers remain URL-encoded as received.
    pub headers: reqwest::header::HeaderMap,
    /// The body-transfer failure, also available through the error source chain.
    #[source]
    pub source: reqwest::Error,
}

impl std::fmt::Debug for IncompleteResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IncompleteResponse")
            .field("status", &self.status)
            .field("header_names", &self.headers.keys().collect::<Vec<_>>())
            .field("source", &self.source)
            .finish()
    }
}

impl ClientError {
    /// What this failure says about the document the request asked for: may
    /// one exist despite the error? See [`OutcomeClass`].
    ///
    /// A request refused before it was sent ([`ClientError::Request`]) is
    /// [`OutcomeClass::Rejected`]: nothing reached szamlazz.hu. An API error's
    /// class is its [`ErrorCode::outcome_class`](crate::ErrorCode::outcome_class).
    /// A transport failure, unavailability (`szlahu_down`), a non-2xx status
    /// reached before body interpretation and an unparseable response are
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
            | Self::IncompleteResponse(_)
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
#[derive(Default)]
pub struct ClientBuilder {
    credentials: Option<Credentials>,
    endpoint: Option<String>,
    http: Option<reqwest::Client>,
}

impl std::fmt::Debug for ClientBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientBuilder")
            .field("credentials", &self.credentials)
            .field(
                "endpoint",
                &self.endpoint.as_deref().map(endpoint_diagnostic),
            )
            .field("http", &self.http.as_ref().map(|_| "reqwest::Client"))
            .finish()
    }
}

// Use the transport's URL parser, rather than guessing where an authority
// ends. If parsing or userinfo removal fails, do not echo the original text.
fn endpoint_diagnostic(endpoint: &str) -> String {
    let Ok(mut url) = reqwest::Url::parse(endpoint) else {
        return "[invalid URL]".to_owned();
    };
    if url.set_password(None).is_err() || url.set_username("").is_err() {
        return "[invalid URL]".to_owned();
    }
    url.to_string()
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
    /// On native targets, enable `.cookie_store(true)` (as the default client does)
    /// or supply `.cookie_provider(Arc<Jar>)` so the
    /// `JSESSIONID` session cookie is reused and consecutive requests skip
    /// re-authentication; without it every request logs in again.
    /// The supplied client owns all transport settings. Its retry policy
    /// also remains active: one [`Client::send`]
    /// may submit several POSTs without application-level reconciliation.
    /// Configure retries, deadlines and redirects for the operations you send.
    /// Reuse the jar within
    /// one account, never across independently authenticated accounts. Cloning
    /// a reqwest client shares its jar; a new client over the same provider
    /// also shares it. Refresh with a fresh client **and fresh provider** after
    /// credential/account changes or relevant account edits (see [`Client`]).
    ///
    /// Browser wasm uses Fetch and browser-managed cookies, not this native
    /// jar. Injecting a client does not enable cross-origin credentials:
    /// reqwest sets that on individual requests, and [`Client::send`] leaves
    /// Fetch's same-origin credentials default in place. See [Features](crate#features).
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
        endpoint: endpoint_diagnostic(endpoint),
        message,
    };
    let url = reqwest::Url::parse(endpoint).map_err(|error| invalid(error.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(invalid("scheme is not http or https".to_owned()));
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
        /// The endpoint with URL userinfo removed, or `[invalid URL]` when
        /// parsing or userinfo removal failed.
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
/// reqwest's cookie store, skipping re-authentication (sessions expire after
/// 90 minutes of inactivity), bounds each request to [`REQUEST_TIMEOUT`] so a stalled server
/// cannot hang the call forever, and does not follow redirects: some redirect
/// statuses convert POST to GET, while others can forward the credential-bearing
/// body to a different target. On wasm the browser/runtime owns cookies and
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
///
/// On native targets, reuse a client within one account to retain its session.
/// Independently authenticated accounts need distinct cookie jars. [`Clone`]
/// shares the underlying reqwest client and jar; it does **not** refresh the
/// session. Each new default client starts with a fresh in-memory jar.
///
/// [Vendor guidance](https://docs.szamlazz.hu/agent/basics/session-cookie)
/// advises a fresh session after company-data or email edits: build a new
/// default client, or inject a fresh client with a fresh cookie provider.
/// Use a fresh jar on credential/account changes too. This is caller ownership
/// policy, not evidence of vendor invalidation or key-versus-cookie precedence.
/// Sessions expire after 90 minutes of inactivity; without persistence requests
/// reauthenticate. Disk persistence and automatic refresh are not required.
///
/// These native jar guarantees do not describe browser wasm; see
/// [Features](crate#features) for the Fetch/CORS boundary.
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    credentials: Credentials,
    endpoint: reqwest::Url,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("http", &"reqwest::Client")
            .field("credentials", &self.credentials)
            .field("endpoint", &endpoint_diagnostic(self.endpoint.as_str()))
            .finish()
    }
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
    /// cannot be parsed. Response interpretation follows [`RawResponse`]:
    /// nonblank down header, error-code header (operation-specific numbered
    /// 56), non-2xx status, then body.
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
        let headers = response.headers().clone();
        let body = response.bytes().await.map_err(|source| {
            ClientError::IncompleteResponse(Box::new(IncompleteResponse {
                status,
                headers: headers.clone(),
                source,
            }))
        })?;
        let headers: Vec<(String, String)> = headers
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_owned(),
                    String::from_utf8_lossy(value.as_bytes()).into_owned(),
                )
            })
            .collect();
        let raw = RawResponse::new(headers, body.to_vec()).with_status(status);

        Ok(request.parse(&raw)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_agent_key_is_redacted_in_client_diagnostics() {
        let key = "legacy-agent-key-secret";
        let builder = Client::builder().credentials(Credentials::user_password(key, key));
        assert!(!format!("{builder:?}").contains(key));
        let client = builder.build().expect("client without sending");
        assert!(!format!("{client:?}").contains(key));
    }

    /// A malformed endpoint is refused when the client is built, not on
    /// every send as a transport error.
    #[test]
    fn endpoint_diagnostics_redact_url_credentials() {
        for endpoint in [
            "https://alice:URL_PASSWORD@example.test/szamla/",
            "ftp://alice:URL_PASSWORD@example.test/szamla/",
            "https://alice:URL_PASSWORD@[bad/",
            "alice:URL_PASSWORD@example.test/szamla/",
        ] {
            let builder = Client::builder()
                .credentials(Credentials::agent_key("XML_SECRET"))
                .endpoint(endpoint);
            let mut diagnostics = vec![format!("{builder:?}")];
            match builder.build() {
                Ok(client) => diagnostics.push(format!("{client:?}")),
                Err(error) => {
                    diagnostics.push(format!("{error:?}"));
                    diagnostics.push(error.to_string());
                }
            }
            for diagnostic in diagnostics {
                assert!(!diagnostic.contains("URL_PASSWORD"), "{diagnostic}");
                assert!(!diagnostic.contains("alice"), "{diagnostic}");
                assert!(!diagnostic.contains("XML_SECRET"), "{diagnostic}");
            }
        }
    }

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
                } => assert_eq!(given, endpoint_diagnostic(endpoint)),
                other => panic!("{endpoint}: expected InvalidEndpoint, got {other:?}"),
            }
        }
        assert!(matches!(
            Client::builder().endpoint("http://127.0.0.1:1/").build(),
            Err(BuildError::MissingCredentials)
        ));
    }
}
