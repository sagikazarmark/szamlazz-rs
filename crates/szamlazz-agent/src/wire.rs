//! The wire layer: fully built HTTP requests and raw-response ingestion,
//! with no HTTP client attached.

use crate::credentials::Credentials;
use crate::error::{ApiError, ErrorCode, ParseError, RequestError, ResponseError, body_excerpt};

/// The single Számla Agent endpoint. Every operation POSTs here; the
/// multipart form field name selects the operation.
///
/// A [`WireRequest`] does not carry the URL: the endpoint is a property of the
/// transport, not of the operation, so the client owns it: the bundled
/// reqwest client through its builder, an integration with its own HTTP
/// client by sending a `POST` to this constant (or to a mock server in tests).
pub const ENDPOINT: &str = "https://www.szamlazz.hu/szamla/";

/// Fixed multipart boundary.
///
/// Deterministic on purpose: request serialization is pure, so golden-file
/// tests can assert entire bodies byte-for-byte. The marker cannot occur in
/// generated XML unless a caller embeds it in their own field values.
const BASE_BOUNDARY: &str = "----szamlazz-agent-4f7d1a2b9c3e";

/// A fully built HTTP request body, ready for any client to POST to
/// [`ENDPOINT`].
///
/// Exactly what an HTTP client needs and nothing about the transport: send
/// `body` with a `Content-Type` of `content_type`. Everything transport-side,
/// the URL, timeouts, TLS, and the `JSESSIONID` session cookie a response
/// sets (see [`RawResponse::session_cookie`]), is the client's to manage.
///
/// Read, never constructed, outside this crate; fields may be added.
#[derive(Clone)]
#[non_exhaustive]
pub struct WireRequest {
    /// Value for the `Content-Type` header: `multipart/form-data` with the
    /// boundary used in `body`.
    pub content_type: String,
    /// The complete request body: the operation's XML document in a file
    /// field named after the operation, plus any attachments.
    pub body: Vec<u8>,
}

impl std::fmt::Debug for WireRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WireRequest")
            .field("content_type", &self.content_type)
            .field("body_len", &self.body.len())
            .finish()
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

/// A raw HTTP response as received.
///
/// Build one from any HTTP client's response, then hand it to the request
/// type's `parse` function. szamlazz.hu signals errors in-band (HTTP 200
/// with `szlahu_*` headers and a `<hibakod>` body), so the parsers read the
/// headers and the body first; the HTTP status ([`RawResponse::with_status`])
/// only matters when neither carries a szamlazz.hu answer, where a non-2xx
/// says the endpoint (a proxy, a CDN, a misconfigured URL) answered instead.
///
/// `Debug` names the response's headers but never a cookie's value: the
/// `Set-Cookie` header carries the `JSESSIONID`, which authenticates as the
/// account for 90 minutes. The body is printed as its length.
#[derive(Clone)]
pub struct RawResponse {
    status: Option<u16>,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl std::fmt::Debug for RawResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let headers: Vec<(&str, std::borrow::Cow<'_, str>)> = self
            .headers
            .iter()
            .map(|(name, value)| (name.as_str(), redact_header(name, value)))
            .collect();

        formatter
            .debug_struct("RawResponse")
            .field("status", &self.status)
            .field("headers", &headers)
            .field("body_len", &self.body.len())
            .finish()
    }
}

/// A header value as `Debug` shows it: a cookie is reduced to its name.
fn redact_header<'v>(name: &str, value: &'v str) -> std::borrow::Cow<'v, str> {
    if name == "set-cookie" {
        let cookie_name = value.split(['=', ';']).next().unwrap_or_default().trim();
        std::borrow::Cow::Owned(format!("{cookie_name}=…"))
    } else {
        std::borrow::Cow::Borrowed(value)
    }
}

impl RawResponse {
    /// Creates a raw response from header pairs and the body bytes. Header
    /// name lookup is case-insensitive. The HTTP status is unknown until
    /// [`with_status`](Self::with_status) supplies it.
    pub fn new<N, V>(headers: impl IntoIterator<Item = (N, V)>, body: Vec<u8>) -> Self
    where
        N: AsRef<str>,
        V: AsRef<str>,
    {
        Self {
            status: None,
            headers: headers
                .into_iter()
                .map(|(n, v)| (n.as_ref().to_ascii_lowercase(), v.as_ref().to_owned()))
                .collect(),
            body,
        }
    }

    /// Records the HTTP status the response arrived with.
    ///
    /// Optional: the parsers read szamlazz.hu's in-band answer first. With
    /// the status known, a non-2xx response that carries no `szlahu_*` header
    /// is refused as [`ParseError::HttpStatus`] (the endpoint answered, not
    /// szamlazz.hu) instead of being parsed as an unexpected body. The
    /// bundled reqwest client always sets it.
    #[must_use]
    pub fn with_status(mut self, status: u16) -> Self {
        self.status = Some(status);
        self
    }

    /// The HTTP status the response arrived with, when the client that built
    /// this response supplied it.
    #[must_use]
    pub fn status(&self) -> Option<u16> {
        self.status
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
    ///
    /// Document-issuing operations report the issued number as
    /// `szlahu_szamlaszam`, the totals as `szlahu_nettovegosszeg` /
    /// `szlahu_bruttovegosszeg` / `szlahu_kintlevoseg`, and szamlazz.hu's
    /// internal *document* identifier as `szlahu_id`, the same value the XML
    /// query returns as `alap/id` (a storno or corrective invoice carries its
    /// original's identifier as `gazdEsemAzon`). `szlahu_id` is not an
    /// account or supplier identifier; that is `szallito/id` in query bodies.
    pub fn szlahu(&self, name: &str) -> Option<String> {
        self.header(name).map(percent_decode)
    }

    /// The error szamlazz.hu reported via `szlahu_error_code` /
    /// `szlahu_error` headers, if any.
    ///
    /// Not every operation sets these headers. Invoice creation (e.g. 152,
    /// 73), storno (14, 221, 352), and proforma deletion (335) report errors
    /// in the headers *and* the body; the XML query (7) and credit-entry
    /// registration (463) report in the body only. `None` here therefore does
    /// not mean success: every parser in this crate also reads the body's
    /// `<hibakod>` / `<hibauzenet>`. An empty header is no error either: it
    /// is read as absent, like an empty `<hibakod>` element.
    #[must_use]
    pub fn header_error(&self) -> Option<ApiError> {
        let code = self
            .header("szlahu_error_code")
            .map(str::trim)
            .filter(|code| !code.is_empty())?;
        let code = ErrorCode::from(code);
        let message = self.szlahu("szlahu_error").unwrap_or_default();

        Some(ApiError { code, message })
    }

    /// Fails on a header-signaled error, otherwise hands back the response.
    ///
    /// In order: `szlahu_down`, `szlahu_error_code`, then (only when neither
    /// carried a szamlazz.hu answer) a known non-2xx status
    /// ([`ParseError::HttpStatus`]).
    pub(crate) fn check(&self) -> Result<&Self, ResponseError> {
        match self.header_verdict()? {
            Some(error) => Err(error.into()),
            None => Ok(self),
        }
    }

    /// What the headers and the status say before the body is read, in the
    /// one order every parser applies: `szlahu_down` is
    /// [`ResponseError::ServiceUnavailable`]; else the `szlahu_error_code`
    /// error, handed back as data for the parser to judge (invoice creation
    /// tolerates 56); else (only when neither carried a szamlazz.hu answer)
    /// a known non-2xx status is [`ParseError::HttpStatus`], the endpoint's
    /// answer, not szamlazz.hu's. `Ok(None)` says the body decides.
    pub(crate) fn header_verdict(&self) -> Result<Option<ApiError>, ResponseError> {
        if let Some(message) = self
            .szlahu("szlahu_down")
            .filter(|message| !message.trim().is_empty())
        {
            return Err(ResponseError::ServiceUnavailable(message));
        }
        if let Some(error) = self.header_error() {
            return Ok(Some(error));
        }
        if let Some(status) = self.status
            && !(200..300).contains(&status)
        {
            return Err(ParseError::HttpStatus {
                status,
                body: body_excerpt(&self.body),
            }
            .into());
        }

        Ok(None)
    }

    /// The `JSESSIONID` session cookie set by this response, as a `Cookie`
    /// header value for the next request.
    ///
    /// Optional performance feature for integrations that transmit the
    /// [`WireRequest`] themselves: replaying the cookie skips
    /// re-authentication. Sessions expire after 90 minutes of inactivity. The
    /// bundled reqwest client reuses the session through reqwest's cookie
    /// store instead and never calls this.
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
/// Implemented by every request type in this crate; the shared plumbing
/// (multipart envelope, credential injection) lives in the provided
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

    /// Builds the complete HTTP request body for this operation, to be sent
    /// as a `POST` to [`ENDPOINT`].
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

        Ok(WireRequest { content_type, body })
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
    fn wire_request_debug_redacts_the_body() {
        let request = WireRequest {
            content_type: "multipart/form-data".to_owned(),
            body: b"<szamlaagentkulcs>secret-agent-key</szamlaagentkulcs>".to_vec(),
        };

        let debug = format!("{request:?}");
        assert!(debug.contains("WireRequest"));
        assert!(debug.contains("multipart/form-data"));
        assert!(!debug.contains("secret-agent-key"));
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

    /// A present-but-empty `szlahu_error_code` is no error, like an empty
    /// `<hibakod>` element: the body decides.
    #[test]
    fn empty_error_code_header_is_absent() {
        for empty in ["", "  "] {
            let response = RawResponse::new(
                [("szlahu_error_code", empty), ("szlahu_error", "")],
                Vec::new(),
            );
            assert_eq!(response.header_error(), None, "{empty:?}");
            assert!(response.check().is_ok(), "{empty:?}");
        }
    }

    #[test]
    fn system_down_header_is_service_unavailable() {
        let response = RawResponse::new([("szlahu_down", "maintenance+window")], Vec::new());
        assert!(matches!(
            response.check(),
            Err(ResponseError::ServiceUnavailable(message)) if message == "maintenance window"
        ));
    }

    /// A 502 HTML page from a proxy carries no szamlazz.hu answer: refused by
    /// its status, with a bounded excerpt of the body (never the whole page),
    /// and the length noted.
    #[test]
    fn non_2xx_without_a_szamlazz_header_is_refused_by_status() {
        let page = format!("<html><body>Bad Gateway {}</body></html>", "x".repeat(2000));
        let total = page.len();
        let response =
            RawResponse::new([("content-type", "text/html")], page.into_bytes()).with_status(502);

        match response.check() {
            Err(ResponseError::Parse(ParseError::HttpStatus { status, body })) => {
                assert_eq!(status, 502);
                assert!(body.starts_with("<html><body>Bad Gateway"), "{body}");
                assert!(body.len() < 600, "bounded: {} bytes", body.len());
                assert!(
                    body.contains(&format!("{total} bytes")),
                    "notes the length: {body}"
                );
            }
            other => panic!("expected HttpStatus, got {other:?}"),
        }

        let text = response.check().expect_err("refused").to_string();
        assert!(text.starts_with("HTTP 502"), "{text}");
    }

    /// The status is a tie-breaker, not the verdict: szamlazz.hu's in-band
    /// headers are read first whatever the status, and an unknown status
    /// (a caller with its own HTTP client that did not supply one) changes
    /// nothing.
    #[test]
    fn in_band_headers_take_precedence_over_the_status() {
        let error = RawResponse::new(
            [("szlahu_error_code", "3"), ("szlahu_error", "login")],
            Vec::new(),
        )
        .with_status(500);
        assert!(
            matches!(error.check(), Err(ResponseError::Api(api)) if api.code == ErrorCode::InvalidCredentials)
        );

        let down = RawResponse::new([("szlahu_down", "maintenance")], Vec::new()).with_status(503);
        assert!(matches!(
            down.check(),
            Err(ResponseError::ServiceUnavailable(_))
        ));

        let ok = RawResponse::new::<&str, &str>([], b"<szamla/>".to_vec()).with_status(200);
        assert!(ok.check().is_ok());
        let unknown = RawResponse::new::<&str, &str>([], b"<html/>".to_vec());
        assert!(unknown.check().is_ok(), "no status, no verdict");
    }

    /// A `RawResponse` is the natural thing to log on a parse failure; its
    /// `Set-Cookie` header carries the `JSESSIONID`, which authenticates as
    /// the account for 90 minutes. `Debug` names the cookie, never its value,
    /// and prints the body's length rather than the body.
    #[test]
    fn raw_response_debug_redacts_cookies_and_the_body() {
        let response = RawResponse::new(
            [
                ("Set-Cookie", "JSESSIONID=SECRET-SESSION; Path=/; HttpOnly"),
                ("szlahu_szamlaszam", "E-TST-2026-1"),
            ],
            b"<szamla>body</szamla>".to_vec(),
        )
        .with_status(200);

        let debug = format!("{response:?}");
        assert!(debug.contains("RawResponse"), "{debug}");
        assert!(debug.contains("200"), "{debug}");
        assert!(debug.contains("szlahu_szamlaszam"), "{debug}");
        assert!(debug.contains("E-TST-2026-1"), "{debug}");
        assert!(debug.contains("set-cookie"), "names the header: {debug}");
        assert!(debug.contains("JSESSIONID"), "names the cookie: {debug}");
        assert!(!debug.contains("SECRET-SESSION"), "{debug}");
        assert!(!debug.contains("<szamla>"), "{debug}");
        assert!(debug.contains("body_len"), "{debug}");
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
