//! Internal XML plumbing: an order-preserving document writer, lenient
//! deserialization helpers, the verdict envelope every response opens with,
//! and the response blocks more than one operation parses.
//!
//! Request writers are hand-written on purpose: element order in the Számla
//! Agent XML is fixed, so the writer code *is* the wire specification.

use jiff::civil::Date;
use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use rust_decimal::Decimal;

use crate::credentials::Credentials;
use crate::error::{ApiError, ErrorCode, ParseError, ResponseError, body_excerpt};
use crate::wire::RawResponse;

const WRITE_EXPECT: &str = "writing XML to an in-memory buffer cannot fail";

/// Builds a complete UTF-8 XML document with the given root element and
/// default namespace.
pub(crate) fn document(
    root: &str,
    namespace: &str,
    build: impl FnOnce(&mut Element<'_>),
) -> Vec<u8> {
    let mut writer = Writer::new(Vec::new());
    writer
        .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
        .expect(WRITE_EXPECT);
    let mut start = BytesStart::new(root);
    start.push_attribute(("xmlns", namespace));
    writer.write_event(Event::Start(start)).expect(WRITE_EXPECT);
    build(&mut Element {
        writer: &mut writer,
    });
    writer
        .write_event(Event::End(BytesEnd::new(root)))
        .expect(WRITE_EXPECT);

    writer.into_inner()
}

/// Validates a structured Agent response envelope and returns its UTF-8 text.
///
/// A body that is not the expected envelope is reported with a [bounded
/// excerpt](body_excerpt) of itself, never whole.
pub(crate) fn response_text<'a>(
    body: &'a [u8],
    expected_root: &str,
    expected_namespace: &str,
) -> Result<&'a str, ParseError> {
    response_root(body, &[(expected_root, expected_namespace)]).map(|(_, text)| text)
}

/// Validates a response body against the envelopes an operation can answer
/// with, `(root, namespace)` pairs, and returns the index of the one that
/// matched together with the body's UTF-8 text.
///
/// One root under two namespaces (the NAV taxpayer reply under OSA 2.0 and
/// 3.0) and two roots (the XML query's `szamla` or the `xmlszamlavalasz`
/// error envelope) are both one call. A body that matches none is reported
/// with a [bounded excerpt](body_excerpt) of itself, never whole.
pub(crate) fn response_root<'a>(
    body: &'a [u8],
    expected: &[(&str, &str)],
) -> Result<(usize, &'a str), ParseError> {
    use quick_xml::name::ResolveResult;

    let text = std::str::from_utf8(body).map_err(|error| ParseError::Invalid {
        field: "response body",
        message: error.to_string(),
    })?;
    let mut reader = quick_xml::reader::NsReader::from_str(text);
    reader.config_mut().check_comments = true;
    let mut depth = 0usize;
    let mut matched = None;
    let mut first = true;
    let invalid = || ParseError::UnexpectedBody(body_excerpt(body));

    loop {
        let event = reader.read_event().map_err(quick_xml::DeError::from)?;
        let namespace = match &event {
            Event::Start(start) | Event::Empty(start) => {
                reader.resolver().resolve_element(start.name()).0
            }
            Event::End(end) => reader.resolver().resolve_element(end.name()).0,
            _ => ResolveResult::Unbound,
        };

        if matches!(namespace, ResolveResult::Unknown(_)) {
            return Err(ParseError::UnexpectedBody(
                "undeclared XML element prefix".into(),
            ));
        }
        let namespace_uri = namespace_uri(&namespace)?;
        if let Event::Start(start) | Event::Empty(start) = &event {
            for attribute in start.attributes() {
                let attribute = attribute
                    .map_err(quick_xml::Error::from)
                    .map_err(quick_xml::DeError::from)?;
                if matches!(
                    reader.resolver().resolve_attribute(attribute.key).0,
                    ResolveResult::Unknown(_)
                ) {
                    return Err(ParseError::UnexpectedBody(
                        "undeclared XML attribute prefix".into(),
                    ));
                }
            }
        }

        let empty = matches!(event, Event::Empty(_));
        match event {
            Event::Start(start) | Event::Empty(start) if depth == 0 => {
                if matched.is_some() {
                    return Err(invalid());
                }
                let local_name = start.local_name();
                let local = local_name.as_ref();
                matched = expected.iter().position(|(root, expected_namespace)| {
                    local == *root && namespace_uri.as_deref() == Some(*expected_namespace)
                });

                if matched.is_some() {
                    depth = usize::from(!empty);
                } else {
                    let wanted = expected
                        .iter()
                        .map(|(root, namespace)| format!("{root} in namespace {namespace}"))
                        .collect::<Vec<_>>()
                        .join(" or ");
                    return Err(ParseError::UnexpectedBody(format!(
                        "expected {wanted}, got {local}: {}",
                        body_excerpt(body)
                    )));
                }
            }
            Event::Start(_) => depth += 1,
            Event::End(_) => {
                depth = depth.checked_sub(1).ok_or_else(invalid)?;
            }
            Event::Decl(decl) => {
                if !first {
                    return Err(invalid());
                }
                validate_declaration(&decl)?;
            }
            Event::Text(value) if depth == 0 => {
                if !value.as_ref().chars().all(is_xml_space) {
                    return Err(invalid());
                }
            }
            Event::CData(_) | Event::GeneralRef(_) if depth == 0 => return Err(invalid()),
            Event::DocType(_) => return Err(invalid()),
            Event::PI(pi) if !valid_pi_target(pi.target()) => return Err(invalid()),
            Event::Eof => {
                return if depth == 0 {
                    matched.map(|index| (index, text)).ok_or_else(invalid)
                } else {
                    Err(invalid())
                };
            }
            _ => {}
        }
        first = false;
    }
}

/// Present only the protocol namespace to serde, which matches local names.
/// Entire foreign subtrees are ignored, including descendants that re-enter
/// the protocol namespace. Serde then owns parent-path and field recognition.
/// Slice the original text rather than reserializing it: business whitespace,
/// entity references, CDATA and namespace aliases retain their exact spelling.
pub(crate) fn protocol_text<'a>(
    text: &'a str,
    namespace: &str,
) -> Result<std::borrow::Cow<'a, str>, ParseError> {
    let mut reader = quick_xml::reader::NsReader::from_str(text);
    let mut skipped_depth = 0usize;
    let mut kept_until = 0usize;
    let mut output = None::<String>;
    loop {
        let before = usize::try_from(reader.buffer_position()).expect("position within input str");
        let (resolved, event) = reader
            .read_resolved_event()
            .map_err(quick_xml::DeError::from)?;
        let foreign = namespace_uri(&resolved)?.as_deref() != Some(namespace);
        let empty = matches!(event, Event::Empty(_));
        match event {
            Event::Start(_) | Event::Empty(_) => {
                if skipped_depth == 0 && foreign {
                    output
                        .get_or_insert_with(String::new)
                        .push_str(&text[kept_until..before]);
                    // Keep an unknown child in the projected shape: deleting
                    // it entirely could turn `tr<foreign/>ue` into `true`.
                    output
                        .as_mut()
                        .expect("initialized")
                        .push_str("<__szamlazz_foreign/>");
                    if empty {
                        kept_until = usize::try_from(reader.buffer_position())
                            .expect("position within input str");
                    } else {
                        skipped_depth = 1;
                    }
                } else if skipped_depth > 0 && !empty {
                    skipped_depth += 1;
                }
            }
            Event::End(_) if skipped_depth > 0 => {
                skipped_depth -= 1;
                if skipped_depth == 0 {
                    kept_until = usize::try_from(reader.buffer_position())
                        .expect("position within input str");
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(match output {
        None => std::borrow::Cow::Borrowed(text),
        Some(mut output) => {
            output.push_str(&text[kept_until..]);
            std::borrow::Cow::Owned(output)
        }
    })
}

/// Namespace declaration values are XML attributes: character references
/// participate in URI identity just as they do in ordinary attribute values.
pub(crate) fn namespace_uri<'a>(
    resolved: &quick_xml::name::ResolveResult<'a>,
) -> Result<Option<std::borrow::Cow<'a, str>>, ParseError> {
    match resolved {
        quick_xml::name::ResolveResult::Bound(namespace) => {
            quick_xml::escape::unescape(namespace.0)
                .map(Some)
                .map_err(quick_xml::DeError::from)
                .map_err(ParseError::from)
        }
        quick_xml::name::ResolveResult::Unbound => Ok(None),
        quick_xml::name::ResolveResult::Unknown(_) => {
            Err(ParseError::UnexpectedBody("undeclared XML prefix".into()))
        }
    }
}

/// Check declaration separators as well as pseudo-attribute order and values:
/// quick-xml's attribute iterator permits adjacent quoted attributes.
fn validate_declaration(decl: &BytesDecl<'_>) -> Result<(), ParseError> {
    let invalid = || ParseError::UnexpectedBody("invalid XML declaration".into());
    if decl.version().map_err(quick_xml::DeError::from)?.as_ref() != "1.0" {
        return Err(invalid());
    }
    let start = BytesStart::from_content(decl.as_ref(), 3);
    let mut quote = None;
    let mut needs_space = false;
    for ch in start.attributes_raw().chars() {
        if let Some(open) = quote {
            if ch == open {
                quote = None;
                needs_space = true;
            }
        } else {
            if needs_space && !is_xml_space(ch) {
                return Err(invalid());
            }
            needs_space = false;
            if matches!(ch, '\'' | '"') {
                quote = Some(ch);
            }
        }
    }
    let mut last = 0;
    for attribute in start.attributes() {
        let attribute = attribute
            .map_err(quick_xml::Error::from)
            .map_err(quick_xml::DeError::from)?;
        let rank = match attribute.key.as_ref() {
            "version" if attribute.value.as_ref() == "1.0" => 1,
            "encoding" if valid_encoding_name(attribute.value.as_ref()) => 2,
            "standalone" if matches!(attribute.value.as_ref(), "yes" | "no") => 3,
            _ => return Err(invalid()),
        };
        if rank <= last {
            return Err(invalid());
        }
        last = rank;
    }
    Ok(())
}

/// XML 1.0 Name, with the reserved PI target excluded. Prefixes have no
/// namespace meaning on a PI, and non-ASCII Name characters are legal.
fn valid_pi_target(value: &str) -> bool {
    let mut chars = value.chars();
    !value.eq_ignore_ascii_case("xml")
        && chars.next().is_some_and(xml_name_start)
        && chars.all(|ch| xml_name_start(ch) || matches!(ch, '-' | '.' | '0'..='9' | '\u{b7}' | '\u{300}'..='\u{36f}' | '\u{203f}'..='\u{2040}'))
}

fn xml_name_start(ch: char) -> bool {
    matches!(ch, ':' | '_' | 'A'..='Z' | 'a'..='z' | '\u{c0}'..='\u{d6}' | '\u{d8}'..='\u{f6}' | '\u{f8}'..='\u{2ff}' | '\u{370}'..='\u{37d}' | '\u{37f}'..='\u{1fff}' | '\u{200c}'..='\u{200d}' | '\u{2070}'..='\u{218f}' | '\u{2c00}'..='\u{2fef}' | '\u{3001}'..='\u{d7ff}' | '\u{f900}'..='\u{fdcf}' | '\u{fdf0}'..='\u{fffd}' | '\u{10000}'..='\u{effff}')
}

fn valid_encoding_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// The verdict every Számla Agent response envelope opens with: `sikeres`,
/// and on failure `hibakod` / `hibauzenet`. One type for the
/// `xmlszamlavalasz`, `xmlszamladbkdelvalasz`, `xmlnyugtavalasz` and
/// `xmlnyugtasendvalasz` envelopes; the payload that follows it is each
/// operation's own and is read from the same text.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub(crate) struct Verdict {
    #[serde(deserialize_with = "de::flexible_bool")]
    pub(crate) sikeres: bool,
    #[serde(default, deserialize_with = "de::empty_as_none")]
    pub(crate) hibakod: Option<String>,
    #[serde(default)]
    pub(crate) hibauzenet: Option<String>,
}

impl Verdict {
    /// The error a `sikeres=false` verdict reports; `None` on success.
    ///
    /// A failure without a `hibakod` (or with an empty one) is
    /// [`ErrorCode::Absent`]: szamlazz.hu sent no code, and none is invented.
    pub(crate) fn api_error(&self) -> Option<ApiError> {
        if self.sikeres {
            return None;
        }
        Some(ApiError {
            code: self
                .hibakod
                .as_deref()
                .map_or(ErrorCode::Absent, ErrorCode::from),
            message: self.hibauzenet.clone().unwrap_or_default(),
        })
    }

    /// `Ok` on success, the reported [`ApiError`] otherwise.
    pub(crate) fn check(&self) -> Result<(), ApiError> {
        self.api_error().map_or(Ok(()), Err)
    }
}

/// Reads the verdict of the envelope `root` in `namespace` and fails on a
/// header error, on unavailability, on a body that is not that envelope, and
/// on a `sikeres=false` verdict; hands back the body text for the payload.
fn verdict_text<'a>(
    response: &'a RawResponse,
    root: &str,
    namespace: &str,
) -> Result<std::borrow::Cow<'a, str>, ResponseError> {
    response.check()?;
    let text = response_text(response.body(), root, namespace)?;
    let text = protocol_text(text, namespace)?;
    let verdict: Verdict = quick_xml::de::from_str(&text).map_err(ParseError::from)?;
    verdict.check()?;

    Ok(text)
}

/// Parses an operation's success payload out of the verdict envelope `root`
/// in `namespace`: the headers, the envelope shape and the verdict are
/// checked first (see [`verdict`]), then the whole envelope text is read as
/// `T`, so `T` declares the payload elements and nothing of the verdict.
pub(crate) fn valasz<T: serde::de::DeserializeOwned>(
    response: &RawResponse,
    root: &str,
    namespace: &str,
) -> Result<T, ResponseError> {
    let text = verdict_text(response, root, namespace)?;

    Ok(quick_xml::de::from_str(&text).map_err(ParseError::from)?)
}

/// Checks a response whose success carries no payload: the headers
/// (`szlahu_down`, `szlahu_error_code`, the status), the envelope `root` in
/// `namespace`, and the verdict.
pub(crate) fn verdict(
    response: &RawResponse,
    root: &str,
    namespace: &str,
) -> Result<(), ResponseError> {
    verdict_text(response, root, namespace).map(drop)
}

/// Writer positioned inside an open element.
pub(crate) struct Element<'w> {
    writer: &'w mut Writer<Vec<u8>>,
}

impl Element<'_> {
    /// Writes a nested container element.
    pub(crate) fn node(&mut self, name: &str, build: impl FnOnce(&mut Element<'_>)) {
        self.writer
            .write_event(Event::Start(BytesStart::new(name)))
            .expect(WRITE_EXPECT);
        build(&mut Element {
            writer: self.writer,
        });
        self.writer
            .write_event(Event::End(BytesEnd::new(name)))
            .expect(WRITE_EXPECT);
    }

    /// Writes `<name>value</name>` with XML-escaped text.
    pub(crate) fn text(&mut self, name: &str, value: &str) {
        self.writer
            .write_event(Event::Start(BytesStart::new(name)))
            .expect(WRITE_EXPECT);
        self.writer
            .write_event(Event::Text(BytesText::new(value)))
            .expect(WRITE_EXPECT);
        self.writer
            .write_event(Event::End(BytesEnd::new(name)))
            .expect(WRITE_EXPECT);
    }

    /// Writes the element only when the value is present.
    pub(crate) fn text_opt(&mut self, name: &str, value: Option<&str>) {
        if let Some(value) = value {
            self.text(name, value);
        }
    }

    /// Writes `true`/`false`.
    pub(crate) fn bool(&mut self, name: &str, value: bool) {
        self.text(name, if value { "true" } else { "false" });
    }

    /// Writes a decimal in plain (non-scientific) notation.
    pub(crate) fn decimal(&mut self, name: &str, value: Decimal) {
        self.text(name, &value.to_string());
    }

    /// Writes an ISO `YYYY-MM-DD` date.
    pub(crate) fn date(&mut self, name: &str, value: Date) {
        self.text(name, &value.to_string());
    }

    /// Writes the element only when the value is present.
    pub(crate) fn date_opt(&mut self, name: &str, value: Option<Date>) {
        if let Some(value) = value {
            self.date(name, value);
        }
    }

    /// Writes the credential fields in wire order (`felhasznalo`, `jelszo`,
    /// `szamlaagentkulcs`).
    pub(crate) fn credentials(&mut self, credentials: &Credentials) {
        match credentials {
            Credentials::AgentKey(key) => self.text("szamlaagentkulcs", key.expose()),
            Credentials::UserPassword { username, password } => {
                self.text("felhasznalo", username);
                self.text("jelszo", password);
            }
        }
    }
}

/// Serde helpers for szamlazz.hu's lenient response XML, where absent values
/// arrive as empty elements and booleans may be `0`/`1`.
pub(crate) mod de {
    use jiff::civil::Date;
    use serde::{Deserialize, Deserializer};

    /// Required finite numeric text, without implicit precision loss.
    pub fn decimal<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<rust_decimal::Decimal, D::Error> {
        crate::number::parse(String::deserialize(deserializer)?.trim())
            .map_err(serde::de::Error::custom)
    }

    pub fn optional_decimal<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<rust_decimal::Decimal>, D::Error> {
        let value = Option::<String>::deserialize(deserializer)?;
        match value.as_deref().map(str::trim) {
            None | Some("") => Ok(None),
            Some(value) => crate::number::parse(value)
                .map(Some)
                .map_err(serde::de::Error::custom),
        }
    }

    /// Read a printed civil date, discarding only a complete XSD timezone.
    /// The checked fallback retains the legacy reader's finite date domain.
    fn civil_date(value: &str) -> Result<Date, String> {
        let value = value.trim_matches(super::is_xml_space);
        let legacy = value.parse::<Date>();
        if let Ok(date) = legacy {
            return Ok(date);
        }
        let bare = if let Some(bare) = value.strip_suffix('Z') {
            bare
        } else if let Some((bare, offset)) = value
            .len()
            .checked_sub(6)
            .and_then(|n| value.split_at_checked(n))
        {
            let bytes = offset.as_bytes();
            if !matches!(bytes[0], b'+' | b'-')
                || bytes[3] != b':'
                || ![bytes[1], bytes[2], bytes[4], bytes[5]]
                    .iter()
                    .all(u8::is_ascii_digit)
            {
                return Err(format!("invalid civil date: {value}"));
            }
            let hours = (bytes[1] - b'0') * 10 + bytes[2] - b'0';
            let minutes = (bytes[4] - b'0') * 10 + bytes[5] - b'0';
            if hours > 14 || minutes > 59 || (hours == 14 && minutes != 0) {
                return Err(format!("invalid date offset: {value}"));
            }
            bare
        } else {
            return Err(format!("invalid civil date: {value}"));
        };
        // Only a whole hyphenated date may precede the new suffix forms.
        let parts: Vec<_> = bare.trim_start_matches(['+', '-']).split('-').collect();
        if parts.len() != 3
            || parts[0].len() < 4
            || parts[1].len() != 2
            || parts[2].len() != 2
            || !parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err(format!("invalid civil date: {value}"));
        }
        bare.parse::<Date>().map_err(|error| error.to_string())
    }

    pub fn date<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Date, D::Error> {
        civil_date(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }

    pub fn optional_date<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Date>, D::Error> {
        let value = Option::<String>::deserialize(deserializer)?;
        match value.as_deref().map(str::trim) {
            None | Some("") => Ok(None),
            Some(value) => civil_date(value)
                .map(Some)
                .map_err(serde::de::Error::custom),
        }
    }

    /// Deserializes an optional value from an element that may be absent or
    /// empty; non-empty content is parsed with `FromStr`.
    pub fn empty_as_none<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
    where
        D: Deserializer<'de>,
        T: std::str::FromStr,
        T::Err: std::fmt::Display,
    {
        let value = Option::<String>::deserialize(deserializer)?;

        match value.as_deref().map(str::trim) {
            None | Some("") => Ok(None),
            Some(text) => text.parse().map(Some).map_err(serde::de::Error::custom),
        }
    }

    /// Optional decoded business text: XML whitespace alone is absent;
    /// otherwise every decoded character (including NBSP) is retained.
    pub fn business_text<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
    where
        D: Deserializer<'de>,
        T: std::str::FromStr,
        T::Err: std::fmt::Display,
    {
        Option::<String>::deserialize(deserializer)?
            .filter(|value| !value.chars().all(super::is_xml_space))
            .map(|value| value.parse().map_err(serde::de::Error::custom))
            .transpose()
    }

    /// Deserializes a bool that may be spelled `true`/`false` or `0`/`1`.
    pub fn flexible_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;

        match value.trim() {
            "true" | "1" => Ok(true),
            "false" | "0" | "" => Ok(false),
            other => Err(serde::de::Error::custom(format!("invalid bool: {other}"))),
        }
    }

    /// Deserializes an optional bool with XML Schema's boolean lexical forms;
    /// an absent or empty element becomes `None`.
    pub fn optional_flexible_bool<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<String>::deserialize(deserializer)?;

        match value.as_deref().map(str::trim) {
            None | Some("") => Ok(None),
            Some("true" | "1") => Ok(Some(true)),
            Some("false" | "0") => Ok(Some(false)),
            Some(other) => Err(serde::de::Error::custom(format!("invalid bool: {other}"))),
        }
    }

    /// Deserializes a required value from element text via `FromStr`.
    ///
    /// quick-xml hands leaf elements to `deserialize_any` as maps, which types
    /// like [`rust_decimal::Decimal`] reject; required scalars must be parsed
    /// from the element text explicitly (optional ones go through
    /// [`empty_as_none`]).
    pub fn from_text<'de, D, T>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        T: std::str::FromStr,
        T::Err: std::fmt::Display,
    {
        let value = String::deserialize(deserializer)?;
        value.trim().parse().map_err(serde::de::Error::custom)
    }
}

pub(crate) fn is_xml_space(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\r' | '\n')
}

/// The `osszegek` totals block, byte-identical on a queried invoice
/// (`szamla`) and on a receipt (`nyugta`), and its one projection onto the
/// public [`Totals`](crate::types::Totals) tree.
pub(crate) mod totals {
    use rust_decimal::Decimal;

    use super::de;
    use crate::types::{GrandTotal, Totals, VatTotal};

    /// The `osszegek` element: per-VAT-rate subtotals and the grand total.
    #[derive(Debug, serde::Deserialize)]
    pub struct OsszegekXml {
        /// Per-VAT-rate subtotals (`afakulcsossz`); may be absent, parsed as
        /// no subtotals.
        #[serde(default)]
        pub afakulcsossz: Vec<AfakulcsosszXml>,
        /// The grand total (`totalossz`).
        pub totalossz: TotalosszXml,
    }

    /// One `afakulcsossz` element: the subtotal of a single VAT rate.
    #[derive(Debug, serde::Deserialize)]
    pub struct AfakulcsosszXml {
        /// The special VAT code (`afatipus`); an empty element is none.
        #[serde(default, deserialize_with = "de::business_text")]
        pub afatipus: Option<String>,
        /// The numeric VAT rate token (`afakulcs`).
        pub afakulcs: String,
        /// Net subtotal (`netto`).
        #[serde(deserialize_with = "de::decimal")]
        pub netto: Decimal,
        /// VAT subtotal (`afa`).
        #[serde(deserialize_with = "de::decimal")]
        pub afa: Decimal,
        /// Gross subtotal (`brutto`).
        #[serde(deserialize_with = "de::decimal")]
        pub brutto: Decimal,
    }

    /// The `totalossz` element: the document's grand total.
    #[derive(Debug, serde::Deserialize)]
    pub struct TotalosszXml {
        /// Net total (`netto`).
        #[serde(deserialize_with = "de::decimal")]
        pub netto: Decimal,
        /// VAT total (`afa`).
        #[serde(deserialize_with = "de::decimal")]
        pub afa: Decimal,
        /// Gross total (`brutto`).
        #[serde(deserialize_with = "de::decimal")]
        pub brutto: Decimal,
    }

    impl From<OsszegekXml> for Totals {
        fn from(osszegek: OsszegekXml) -> Self {
            Self {
                by_vat_rate: osszegek.afakulcsossz.into_iter().map(Into::into).collect(),
                total: osszegek.totalossz.into(),
            }
        }
    }

    impl From<AfakulcsosszXml> for VatTotal {
        fn from(ossz: AfakulcsosszXml) -> Self {
            Self {
                vat_type: ossz.afatipus,
                vat_rate_code: ossz.afakulcs,
                net: ossz.netto,
                vat: ossz.afa,
                gross: ossz.brutto,
            }
        }
    }

    impl From<TotalosszXml> for GrandTotal {
        fn from(total: TotalosszXml) -> Self {
            Self {
                net: total.netto,
                vat: total.afa,
                gross: total.brutto,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use rust_decimal::dec;

    use super::*;

    #[test]
    fn writes_escaped_ordered_document() {
        let xml = document("root", "http://example.com/ns", |root| {
            root.node("child", |child| {
                child.text("a", "x < y & z");
                child.bool("b", true);
                child.decimal("c", dec!(12700.50));
                child.date("d", date(2026, 7, 4));
            });
        });
        assert_eq!(
            String::from_utf8(xml).expect("utf-8"),
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <root xmlns=\"http://example.com/ns\">\
             <child><a>x &lt; y &amp; z</a><b>true</b><c>12700.50</c><d>2026-07-04</d></child>\
             </root>"
        );
    }

    #[test]
    fn validates_response_encoding_root_and_namespace() {
        let body = br#"<result xmlns="http://example.com/result"/>"#;
        assert_eq!(
            response_text(body, "result", "http://example.com/result").expect("response"),
            std::str::from_utf8(body).expect("UTF-8")
        );
        assert!(response_text(body, "other", "http://example.com/result").is_err());
        assert!(response_text(body, "result", "http://example.com/wrong").is_err());
        assert!(response_text(b"<result>\xff</result>", "result", "").is_err());
    }

    /// One call answers "which of these envelopes is it": the matched pair's
    /// index, whichever root or namespace it is; none of them is refused
    /// naming every accepted shape.
    #[test]
    fn response_root_names_the_matched_envelope() {
        let shapes = [
            ("reply", "http://example.com/v2"),
            ("reply", "http://example.com/v3"),
            ("document", "http://example.com/doc"),
        ];
        let v3 = br#"<reply xmlns="http://example.com/v3"/>"#;
        assert_eq!(response_root(v3, &shapes).expect("matched").0, 1);
        let document =
            br#"<?xml version="1.0"?><document xmlns="http://example.com/doc"><a/></document>"#;
        assert_eq!(response_root(document, &shapes).expect("matched").0, 2);

        let other = br#"<reply xmlns="http://example.com/v1"/>"#;
        match response_root(other, &shapes).expect_err("no match") {
            ParseError::UnexpectedBody(message) => {
                assert!(
                    message.contains("reply in namespace http://example.com/v2"),
                    "{message}"
                );
                assert!(
                    message.contains(" or document in namespace http://example.com/doc"),
                    "{message}"
                );
                assert!(message.contains("got reply"), "{message}");
            }
            other => panic!("expected an unexpected body, got {other:?}"),
        }
        assert!(matches!(
            response_root(b"   ", &shapes),
            Err(ParseError::UnexpectedBody(message)) if message == "empty response"
        ));
    }

    const ROOT: &str = "xmlvalasz";
    const NS: &str = "http://example.com/xmlvalasz";

    fn envelope(inner: &str) -> RawResponse {
        RawResponse::new::<&str, &str>(
            [],
            format!(r#"<{ROOT} xmlns="{NS}">{inner}</{ROOT}>"#).into_bytes(),
        )
    }

    /// The verdict table: every spelling of `sikeres`, the code and message
    /// with and without each other, and the honest absent code. One table
    /// for the four envelopes that share the verdict.
    #[test]
    fn verdict_table() {
        let cases: [(&str, Result<(), ApiError>); 9] = [
            ("<sikeres>true</sikeres>", Ok(())),
            ("<sikeres>1</sikeres>", Ok(())),
            ("<sikeres>true</sikeres><hibakod>7</hibakod>", Ok(())),
            (
                "<sikeres>false</sikeres><hibakod>7</hibakod><hibauzenet>Hiányzó adat</hibauzenet>",
                Err(ApiError {
                    code: ErrorCode::MissingData,
                    message: "Hiányzó adat".to_owned(),
                }),
            ),
            (
                "<sikeres>0</sikeres><hibakod> 463 </hibakod>",
                Err(ApiError {
                    code: ErrorCode::PaymentOnReversedInvoice,
                    message: String::new(),
                }),
            ),
            (
                "<sikeres>false</sikeres><hibakod>FUTURE</hibakod><hibauzenet>x</hibauzenet>",
                Err(ApiError {
                    code: ErrorCode::Unknown("FUTURE".to_owned()),
                    message: "x".to_owned(),
                }),
            ),
            (
                "<sikeres>false</sikeres><hibauzenet>no code</hibauzenet>",
                Err(ApiError {
                    code: ErrorCode::Absent,
                    message: "no code".to_owned(),
                }),
            ),
            (
                "<sikeres>false</sikeres><hibakod></hibakod><hibauzenet>empty code</hibauzenet>",
                Err(ApiError {
                    code: ErrorCode::Absent,
                    message: "empty code".to_owned(),
                }),
            ),
            (
                "<sikeres>false</sikeres>",
                Err(ApiError {
                    code: ErrorCode::Absent,
                    message: String::new(),
                }),
            ),
        ];
        for (inner, expected) in cases {
            let response = envelope(inner);
            let text = response_text(response.body(), ROOT, NS).expect("envelope");
            let parsed: Verdict = quick_xml::de::from_str(text).expect("verdict parses");
            assert_eq!(parsed.check(), expected, "{inner}");
            assert_eq!(parsed.api_error(), expected.clone().err(), "{inner}");
            match (verdict(&response, ROOT, NS), expected) {
                (Ok(()), Ok(())) => {}
                (Err(ResponseError::Api(api)), Err(expected)) => {
                    assert_eq!(api, expected, "{inner}");
                }
                (got, expected) => panic!("{inner}: expected {expected:?}, got {got:?}"),
            }
        }
    }

    /// `valasz` reads the payload the operation declares after the verdict
    /// passed, from the same text; the verdict is checked first, so a failed
    /// envelope whose payload would not parse still reports the code.
    #[test]
    fn valasz_reads_the_payload_after_the_verdict() {
        #[derive(serde::Deserialize)]
        struct Payload {
            #[serde(default, deserialize_with = "de::empty_as_none")]
            szamlaszam: Option<String>,
            #[serde(default, deserialize_with = "de::empty_as_none")]
            osszeg: Option<Decimal>,
        }

        let ok =
            envelope("<sikeres>true</sikeres><szamlaszam>E-1</szamlaszam><osszeg>12.5</osszeg>");
        let payload: Payload = valasz(&ok, ROOT, NS).expect("payload");
        assert_eq!(payload.szamlaszam.as_deref(), Some("E-1"));
        assert_eq!(payload.osszeg, Some(dec!(12.5)));

        let refused = envelope("<sikeres>false</sikeres><hibakod>3</hibakod><osszeg>junk</osszeg>");
        assert!(matches!(
            valasz::<Payload>(&refused, ROOT, NS),
            Err(ResponseError::Api(api)) if api.code == ErrorCode::InvalidCredentials
        ));

        let malformed = envelope("<sikeres>true</sikeres><osszeg>junk</osszeg>");
        assert!(matches!(
            valasz::<Payload>(&malformed, ROOT, NS),
            Err(ResponseError::Parse(ParseError::Xml(_)))
        ));

        // The headers are read before the body, in the wire's one order.
        let down = RawResponse::new([("szlahu_down", "maintenance")], Vec::new());
        assert!(matches!(
            verdict(&down, ROOT, NS),
            Err(ResponseError::ServiceUnavailable(_))
        ));
        let header_error = RawResponse::new(
            [("szlahu_error_code", "3"), ("szlahu_error", "login")],
            Vec::new(),
        );
        assert!(matches!(
            verdict(&header_error, ROOT, NS),
            Err(ResponseError::Api(api)) if api.code == ErrorCode::InvalidCredentials
        ));
        let wrong_root = RawResponse::new::<&str, &str>([], b"<other/>".to_vec());
        assert!(matches!(
            verdict(&wrong_root, ROOT, NS),
            Err(ResponseError::Parse(ParseError::UnexpectedBody(_)))
        ));
    }
}
