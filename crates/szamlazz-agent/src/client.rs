//! Ready-made async client built on [`reqwest`] (feature `client-reqwest`).
//!
//! A thin shell around the sans-IO core: every operation goes through
//! [`Client::send`], which works with any [`AgentRequest`] type.
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
use crate::error::{ApiError, ParseError, RequestError, ResponseError};
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
    /// The HTTP request itself failed.
    ///
    /// Retry with care: invoice creation has no idempotency key, so a timeout
    /// after the server already issued the document means a retry issues a
    /// duplicate. Receipt call ids prevent duplicate issuance by returning
    /// error 338 on reuse, but a retry does not replay the original success.
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),
}

impl From<ResponseError> for ClientError {
    fn from(error: ResponseError) -> Self {
        match error {
            ResponseError::Api(api) => Self::Api(api),
            ResponseError::Parse(parse) => Self::Parse(parse),
            ResponseError::ServiceUnavailable(message) => Self::ServiceUnavailable(message),
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

    /// Overrides the endpoint URL — for pointing tests at a mock server.
    /// szamlazz.hu has no separate sandbox host; test mode is an account
    /// setting.
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
    /// Fails when no credentials were supplied or the underlying HTTP client
    /// cannot be constructed.
    pub fn build(self) -> Result<Client, BuildError> {
        let credentials = self.credentials.ok_or(BuildError::MissingCredentials)?;
        let http = match self.http {
            Some(http) => http,
            None => default_http_client()?,
        };

        Ok(Client {
            http,
            credentials,
            endpoint: self.endpoint.unwrap_or_else(|| ENDPOINT.to_owned()),
        })
    }
}

/// [`ClientBuilder::build`] failure.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BuildError {
    /// No credentials were supplied.
    #[error("credentials are required")]
    MissingCredentials,
    /// The underlying HTTP client could not be constructed.
    #[error("failed to build HTTP client: {0}")]
    Http(#[from] reqwest::Error),
}

/// On native targets the client keeps the `JSESSIONID` session cookie via
/// reqwest's cookie store, skipping re-authentication (sessions live 90
/// minutes), bounds each request to 60 seconds so a stalled server cannot
/// hang the call forever, and does not follow redirects: the endpoint never
/// redirects, and following one would silently convert the multipart POST
/// into a body-less GET. On wasm the browser/runtime owns cookies and
/// redirect handling.
fn default_http_client() -> Result<reqwest::Client, reqwest::Error> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        reqwest::Client::builder()
            .cookie_store(true)
            .timeout(std::time::Duration::from_mins(1))
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
    endpoint: String,
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
        let mut wire = request.to_wire(&self.credentials)?;
        wire.url = self.endpoint.clone();

        let response = self
            .http
            .post(&wire.url)
            .header(reqwest::header::CONTENT_TYPE, wire.content_type)
            .body(wire.body)
            .send()
            .await?;

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

        let raw = RawResponse::new(headers, body.to_vec());

        Ok(request.parse(&raw)?)
    }
}
