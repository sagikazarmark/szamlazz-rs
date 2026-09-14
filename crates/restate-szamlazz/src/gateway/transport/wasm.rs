//! Single JS-thread exchange; never journal JS errors, cookies or bodies.

use szamlazz_agent::{
    Credentials,
    wire::{AgentRequest, RawResponse},
};
use wasm_bindgen::{JsCast, prelude::*};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = fetch)]
    fn fetch(request: &web_sys::Request) -> js_sys::Promise;
}

#[derive(Debug)]
pub(crate) enum ClientError {
    Request(szamlazz_agent::RequestError),
    Api(szamlazz_agent::ApiError),
    Parse(szamlazz_agent::ParseError),
    ServiceUnavailable(()),
    HttpStatus { status: u16 },
    Transport(szamlazz_agent::reqwest::Error),
    Exchange(&'static str),
}

impl From<szamlazz_agent::ResponseError> for ClientError {
    fn from(error: szamlazz_agent::ResponseError) -> Self {
        match error {
            szamlazz_agent::ResponseError::Api(e) => Self::Api(e),
            szamlazz_agent::ResponseError::Parse(e) => Self::Parse(e),
            szamlazz_agent::ResponseError::ServiceUnavailable(_) => Self::ServiceUnavailable(()),
            szamlazz_agent::ResponseError::HttpStatus { status, .. } => Self::HttpStatus { status },
            _ => Self::Exchange("unclassified response failure"),
        }
    }
}

impl From<szamlazz_agent::ClientError> for ClientError {
    fn from(error: szamlazz_agent::ClientError) -> Self {
        match error {
            szamlazz_agent::ClientError::Request(e) => Self::Request(e),
            szamlazz_agent::ClientError::Api(e) => Self::Api(e),
            szamlazz_agent::ClientError::Parse(e) => Self::Parse(e),
            szamlazz_agent::ClientError::ServiceUnavailable(_) => Self::ServiceUnavailable(()),
            szamlazz_agent::ClientError::HttpStatus { status, .. } => Self::HttpStatus { status },
            szamlazz_agent::ClientError::Transport(e) => Self::Transport(e),
            _ => Self::Exchange("incomplete exchange"),
        }
    }
}

pub(crate) enum Client {
    Fetch {
        endpoint: String,
        credentials: Credentials,
    },
    Supplied(szamlazz_agent::Client),
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ExecutionHttp")
    }
}

impl From<szamlazz_agent::Client> for Client {
    fn from(client: szamlazz_agent::Client) -> Self {
        Self::Supplied(client)
    }
}

impl Client {
    pub(crate) fn new(endpoint: String, credentials: Credentials) -> Self {
        Self::Fetch {
            endpoint,
            credentials,
        }
    }

    pub(crate) async fn send<R: AgentRequest>(
        &self,
        request: &R,
    ) -> Result<R::Response, ClientError> {
        let raw = match self {
            Self::Supplied(client) => {
                return send_wrapper::SendWrapper::new(client.send(request))
                    .await
                    .map_err(Into::into);
            }
            Self::Fetch {
                endpoint,
                credentials,
            } => {
                let wire = request.to_wire(credentials).map_err(ClientError::Request)?;
                // The enclosing Workers event polls and drops this future on
                // its JS thread. No JS values escape this checked wrapper.
                send_wrapper::SendWrapper::new(exchange(endpoint, wire)).await?
            }
        };
        request.parse(&raw).map_err(Into::into)
    }
}

struct Abort(web_sys::AbortController);
impl Drop for Abort {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn exchange(
    endpoint: &str,
    wire: szamlazz_agent::wire::WireRequest,
) -> Result<RawResponse, ClientError> {
    let abort = Abort(
        web_sys::AbortController::new()
            .map_err(|_| ClientError::Exchange("abort controller unavailable"))?,
    );
    let init = web_sys::RequestInit::new();
    init.set_method("POST");
    init.set_redirect(web_sys::RequestRedirect::Manual);
    init.set_signal(Some(&abort.0.signal()));
    init.set_body(&js_sys::Uint8Array::from(wire.body.as_slice()));
    let headers =
        web_sys::Headers::new().map_err(|_| ClientError::Exchange("headers unavailable"))?;
    headers
        .set("content-type", &wire.content_type)
        .map_err(|_| ClientError::Exchange("invalid content type"))?;
    init.set_headers(&headers);
    let request = web_sys::Request::new_with_str_and_init(endpoint, &init)
        .map_err(|_| ClientError::Exchange("request construction failed"))?;
    let transfer = async {
        let response: web_sys::Response = wasm_bindgen_futures::JsFuture::from(fetch(&request))
            .await
            .map_err(|_| ClientError::Exchange("fetch failed"))?
            .dyn_into()
            .map_err(|_| ClientError::Exchange("invalid response"))?;
        let status = response.status();
        let mut headers = Vec::new();
        for entry in response.headers().entries() {
            let entry =
                js_sys::Array::from(&entry.map_err(|_| ClientError::Exchange("invalid headers"))?);
            let name = entry
                .get(0)
                .as_string()
                .ok_or(ClientError::Exchange("invalid header name"))?;
            let value = entry
                .get(1)
                .as_string()
                .ok_or(ClientError::Exchange("invalid header value"))?;
            // Fetch erases repeated non-cookie header boundaries. Never turn
            // concatenated identities into a fabricated document number. This
            // deliberate experimental restriction also refuses literal commas.
            if name.starts_with("szlahu_") && value.contains(',') {
                return Err(ClientError::Exchange("ambiguous Fetch protocol header"));
            }
            headers.push((name, value));
        }
        // Explicit no-session persistence: every XML request carries its own
        // credentials. No response cookie is attached to a subsequent request.
        let body = wasm_bindgen_futures::JsFuture::from(
            response
                .array_buffer()
                .map_err(|_| ClientError::Exchange("body unavailable"))?,
        )
        .await
        .map_err(|_| ClientError::Exchange("incomplete response"))?;
        Ok(RawResponse::new(headers, js_sys::Uint8Array::new(&body).to_vec()).with_status(status))
    };
    use futures_util::future::{Either, select};
    let timer = gloo_timers::future::TimeoutFuture::new(
        u32::try_from(szamlazz_agent::client::REQUEST_TIMEOUT.as_millis()).unwrap_or(u32::MAX),
    );
    let result = match select(std::pin::pin!(transfer), std::pin::pin!(timer)).await {
        Either::Left((result, _)) => result,
        Either::Right(_) => Err(ClientError::Exchange("complete-response deadline exceeded")),
    };
    drop(abort);
    result
}
