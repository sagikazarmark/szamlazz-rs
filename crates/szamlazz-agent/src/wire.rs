//! The sans-IO wire layer: fully built HTTP requests and raw-response
//! ingestion, with no HTTP client attached.

use crate::credentials::Credentials;
use crate::error::{ApiError, ErrorCode, RequestError, ResponseError};

/// The single Számla Agent endpoint. Every operation POSTs here; the
/// multipart form field name selects the operation.
pub const ENDPOINT: &str = "https://www.szamlazz.hu/szamla/";

/// Fixed multipart boundary.
///
/// Deterministic on purpose: request serialization is pure, so golden-file
/// tests can assert entire bodies byte-for-byte. The marker cannot occur in
/// generated XML unless a caller embeds it in their own field values.
const BASE_BOUNDARY: &str = "----szamlazz-agent-4f7d1a2b9c3e";

/// A fully built HTTP request, ready for any client to send.
#[derive(Clone)]
pub struct WireRequest {
    /// Absolute URL to POST to.
    pub url: String,
    /// Value for the `Content-Type` header.
    pub content_type: String,
    /// The complete request body.
    pub body: Vec<u8>,
    /// Value for the `Cookie` header, if a session is being reused.
    ///
    /// Optional performance feature: replaying the `JSESSIONID` cookie from a
    /// previous [`RawResponse::session_cookie`] skips re-authentication.
    /// Sessions expire after 90 minutes of inactivity.
    ///
    /// Only meaningful for sans-IO integrations that transmit the
    /// [`WireRequest`] themselves. The bundled reqwest client does not read
    /// this field; it reuses the session through reqwest's cookie store.
    pub session_cookie: Option<String>,
}

impl std::fmt::Debug for WireRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WireRequest")
            .field("url", &self.url)
            .field("content_type", &self.content_type)
            .field("body_len", &self.body.len())
            .field("has_session_cookie", &self.session_cookie.is_some())
            .finish()
    }
}

impl WireRequest {
    /// Attaches a session cookie captured from an earlier response.
    #[must_use]
    pub fn with_session_cookie(mut self, cookie: impl Into<String>) -> Self {
        self.session_cookie = Some(cookie.into());
        self
    }
}

/// An additional file part contributed by a specific Agent operation.
#[derive(Debug, Clone)]
pub struct MultipartFile<'a> {
    /// Multipart form field name.
    pub name: String,
    /// Uploaded filename.
    pub filename: &'a str,
    /// MIME content type.
    pub content_type: &'a str,
    /// Raw file bytes.
    pub content: &'a [u8],
}

/// Builds the multipart body carrying `xml` in a file field named `action`.
fn multipart(action: &str, xml: &[u8], files: Vec<MultipartFile<'_>>) -> (String, Vec<u8>) {
    let boundary = multipart_boundary(xml, &files);
    let mut body = Vec::with_capacity(xml.len() + 256);
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"{action}\"; filename=\"{action}\"\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: text/xml\r\n\r\n");
    body.extend_from_slice(xml);

    for file in files {
        body.extend_from_slice(format!("\r\n--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                disposition_value(&file.name),
                disposition_value(file.filename)
            )
            .as_bytes(),
        );
        body.extend_from_slice(
            format!(
                "Content-Type: {}\r\n\r\n",
                file.content_type.replace(['\r', '\n'], "")
            )
            .as_bytes(),
        );
        body.extend_from_slice(file.content);
    }
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    (format!("multipart/form-data; boundary={boundary}"), body)
}

fn disposition_value(value: &str) -> String {
    value
        .replace(['\r', '\n'], "")
        .replace('"', "%22")
        .replace('\\', "%5C")
}

fn multipart_boundary(xml: &[u8], files: &[MultipartFile<'_>]) -> String {
    let mut boundary = BASE_BOUNDARY.to_owned();

    while contains_bytes(xml, boundary.as_bytes())
        || files
            .iter()
            .any(|file| contains_bytes(file.content, boundary.as_bytes()))
    {
        boundary.push('x');
    }

    boundary
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// A raw HTTP response as received: status-independent, since szamlazz.hu
/// signals errors in-band.
///
/// Build one from any HTTP client's response, then hand it to the request
/// type's `parse` function.
#[derive(Debug, Clone)]
pub struct RawResponse {
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl RawResponse {
    /// Creates a raw response from header pairs and the body bytes. Header
    /// name lookup is case-insensitive.
    pub fn new<N, V>(headers: impl IntoIterator<Item = (N, V)>, body: Vec<u8>) -> Self
    where
        N: AsRef<str>,
        V: AsRef<str>,
    {
        Self {
            headers: headers
                .into_iter()
                .map(|(n, v)| (n.as_ref().to_ascii_lowercase(), v.as_ref().to_owned()))
                .collect(),
            body,
        }
    }

    /// The response body.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// The first header with the given name (case-insensitive).
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();

        self.headers
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
    }

    /// A `szlahu_*` header value, percent-decoded (szamlazz.hu URL-encodes
    /// them).
    pub fn szlahu(&self, name: &str) -> Option<String> {
        self.header(name).map(percent_decode)
    }

    /// The error szamlazz.hu reported via `szlahu_error_code` /
    /// `szlahu_error` headers, if any.
    #[must_use]
    pub fn header_error(&self) -> Option<ApiError> {
        let code = self.header("szlahu_error_code")?;
        let code = ErrorCode::from(code);
        let message = self.szlahu("szlahu_error").unwrap_or_default();

        Some(ApiError { code, message })
    }

    /// Fails on a header-signaled error, otherwise hands back the response.
    pub(crate) fn check(&self) -> Result<&Self, ResponseError> {
        self.check_available()?;

        match self.header_error() {
            Some(error) => Err(error.into()),
            None => Ok(self),
        }
    }

    pub(crate) fn check_available(&self) -> Result<(), ResponseError> {
        if let Some(message) = self
            .szlahu("szlahu_down")
            .filter(|message| !message.trim().is_empty())
        {
            return Err(ResponseError::ServiceUnavailable(message));
        }

        Ok(())
    }

    /// The `JSESSIONID` session cookie set by this response, as a `Cookie`
    /// header value for [`WireRequest::with_session_cookie`].
    pub fn session_cookie(&self) -> Option<String> {
        let name = "set-cookie";

        self.headers
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
            .find(|v| v.starts_with("JSESSIONID"))
            .and_then(|v| v.split(';').next())
            .map(str::to_owned)
    }
}

/// Percent-decodes a header value; `+` is treated as a space.
fn percent_decode(value: &str) -> String {
    let plus_decoded = value.replace('+', " ");

    percent_encoding::percent_decode_str(&plus_decoded)
        .decode_utf8()
        .map(std::borrow::Cow::into_owned)
        .unwrap_or(plus_decoded)
}

/// A Számla Agent operation: serializes itself to the wire and interprets the
/// raw response.
///
/// Implemented by every request type in this crate; the shared plumbing (URL,
/// multipart envelope, credential injection) lives in the provided
/// [`AgentRequest::to_wire`].
pub trait AgentRequest {
    /// The multipart form field name that selects this operation, e.g.
    /// `action-xmlagentxmlfile`.
    const ACTION: &'static str;

    /// The parsed success payload.
    type Response;

    /// Serializes the request document, injecting `credentials` into the
    /// settings block.
    fn write_xml(&self, credentials: &Credentials) -> Vec<u8>;

    /// Checks cross-field requirements that the XML schema cannot express.
    ///
    /// # Errors
    ///
    /// Implementations return an error when the request's fields do not form
    /// a valid Számla Agent operation.
    fn validate(&self) -> Result<(), RequestError> {
        Ok(())
    }

    /// Interprets a raw response into the typed payload or an error.
    ///
    /// # Errors
    ///
    /// Returns an error when szamlazz.hu reports failure or unavailability, or
    /// when the response cannot be parsed as this operation's payload.
    fn parse(&self, response: &RawResponse) -> Result<Self::Response, ResponseError>;

    /// Additional multipart file parts required by this operation.
    fn multipart_files(&self) -> Vec<MultipartFile<'_>> {
        Vec::new()
    }

    /// Builds the complete HTTP request for this operation.
    ///
    /// # Errors
    ///
    /// Returns an error when validation fails or generated XML contains an
    /// encoding or character that XML 1.0 cannot represent.
    fn to_wire(&self, credentials: &Credentials) -> Result<WireRequest, RequestError> {
        self.validate()?;
        let xml = self.write_xml(credentials);
        validate_xml_10(&xml)?;
        let (content_type, body) = multipart(Self::ACTION, &xml, self.multipart_files());

        Ok(WireRequest {
            url: ENDPOINT.to_owned(),
            content_type,
            body,
            session_cookie: None,
        })
    }
}

fn validate_xml_10(xml: &[u8]) -> Result<(), RequestError> {
    let xml = std::str::from_utf8(xml).map_err(|_| RequestError::InvalidXmlEncoding)?;

    if let Some(character) = xml
        .chars()
        .find(|&character| !is_xml_10_character(character))
    {
        return Err(RequestError::InvalidXmlCharacter(character as u32));
    }

    Ok(())
}

fn is_xml_10_character(character: char) -> bool {
    matches!(
        character,
        '\u{9}' | '\u{A}' | '\u{D}' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_request_debug_redacts_sensitive_transport_data() {
        let request = WireRequest {
            url: ENDPOINT.to_owned(),
            content_type: "multipart/form-data".to_owned(),
            body: b"<szamlaagentkulcs>secret-agent-key</szamlaagentkulcs>".to_vec(),
            session_cookie: Some("JSESSIONID=secret-session".to_owned()),
        };

        let debug = format!("{request:?}");
        assert!(debug.contains("WireRequest"));
        assert!(!debug.contains("secret-agent-key"));
        assert!(!debug.contains("secret-session"));
    }

    #[test]
    fn multipart_envelope_shape() {
        let (content_type, body) = multipart("action-xmlagentxmlfile", b"<xml/>", Vec::new());
        assert_eq!(
            content_type,
            format!("multipart/form-data; boundary={BASE_BOUNDARY}")
        );
        let body = String::from_utf8(body).expect("utf-8");
        assert!(body.starts_with(&format!("--{BASE_BOUNDARY}\r\n")));
        assert!(body.contains("name=\"action-xmlagentxmlfile\""));
        assert!(body.contains("\r\n\r\n<xml/>\r\n"));
        assert!(body.ends_with(&format!("--{BASE_BOUNDARY}--\r\n")));
    }

    #[test]
    fn multipart_boundary_never_occurs_in_file_content() {
        let content = format!("before\r\n--{BASE_BOUNDARY}\r\nafter");
        let file = MultipartFile {
            name: "attachfile1".to_owned(),
            filename: "x.txt",
            content_type: "text/plain",
            content: content.as_bytes(),
        };
        let (content_type, body) = multipart("action", b"<xml/>", vec![file]);
        let boundary = content_type
            .strip_prefix("multipart/form-data; boundary=")
            .expect("boundary");
        assert_ne!(boundary, BASE_BOUNDARY);
        assert_eq!(
            body.windows(content.len())
                .filter(|w| *w == content.as_bytes())
                .count(),
            1
        );
    }

    #[test]
    fn multipart_filename_escapes_header_metacharacters() {
        assert_eq!(disposition_value("a\\\"b.txt"), "a%5C%22b.txt");
    }

    #[test]
    fn header_error_is_decoded() {
        let response = RawResponse::new(
            [
                ("szlahu_error_code", "3"),
                ("szlahu_error", "Sikertelen+bejelentkez%C3%A9s"),
            ],
            Vec::new(),
        );
        let error = response.header_error().expect("error");
        assert_eq!(error.code, ErrorCode::InvalidCredentials);
        assert_eq!(error.message, "Sikertelen bejelentkezés");
    }

    #[test]
    fn header_error_preserves_unknown_code() {
        let response = RawResponse::new(
            [
                ("szlahu_error_code", "FUTURE_CODE"),
                ("szlahu_error", "future"),
            ],
            Vec::new(),
        );
        let error = response.header_error().expect("error");
        assert_eq!(error.code, ErrorCode::Unknown("FUTURE_CODE".to_owned()));
    }

    #[test]
    fn system_down_header_is_service_unavailable() {
        let response = RawResponse::new([("szlahu_down", "maintenance+window")], Vec::new());
        assert!(matches!(
            response.check(),
            Err(ResponseError::ServiceUnavailable(message)) if message == "maintenance window"
        ));
    }

    #[test]
    fn session_cookie_extraction() {
        let response = RawResponse::new(
            [
                ("Set-Cookie", "JSESSIONID=ABC123; Path=/; HttpOnly"),
                ("set-cookie", "other=1"),
            ],
            Vec::new(),
        );
        assert_eq!(
            response.session_cookie().as_deref(),
            Some("JSESSIONID=ABC123")
        );
    }
}
