//! Internal XML plumbing: an order-preserving document writer and lenient
//! deserialization helpers.
//!
//! Request writers are hand-written on purpose: element order in the Számla
//! Agent XML is fixed, so the writer code *is* the wire specification. See
//! ADR 0001.

use jiff::civil::Date;
use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use rust_decimal::Decimal;

use crate::credentials::Credentials;
use crate::error::ParseError;

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
pub(crate) fn response_text<'a>(
    body: &'a [u8],
    expected_root: &str,
    expected_namespace: &str,
) -> Result<&'a str, ParseError> {
    use quick_xml::name::{Namespace, ResolveResult};

    let text = std::str::from_utf8(body).map_err(|error| ParseError::Invalid {
        field: "response body",
        message: error.to_string(),
    })?;
    let mut reader = quick_xml::reader::NsReader::from_str(text);

    loop {
        let (namespace, event) = reader
            .read_resolved_event()
            .map_err(quick_xml::DeError::from)?;

        match event {
            Event::Start(start) | Event::Empty(start) => {
                let local_name = start.local_name();
                let local = local_name.as_ref();

                if local != expected_root
                    || namespace != ResolveResult::Bound(Namespace(expected_namespace))
                {
                    return Err(ParseError::UnexpectedBody(format!(
                        "expected {expected_root} in namespace {expected_namespace}, got {local}: {text}"
                    )));
                }
                return Ok(text);
            }
            Event::Eof => {
                let body = text.trim();
                return Err(ParseError::UnexpectedBody(if body.is_empty() {
                    "empty response".to_owned()
                } else {
                    body.to_owned()
                }));
            }
            _ => {}
        }
    }
}

/// Writer positioned inside an open element.
pub(crate) struct Element<'w> {
    writer: &'w mut Writer<Vec<u8>>,
}

impl Element<'_> {
    /// Writes a nested container element.
    pub fn node(&mut self, name: &str, build: impl FnOnce(&mut Element<'_>)) {
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
    pub fn text(&mut self, name: &str, value: &str) {
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
    pub fn text_opt(&mut self, name: &str, value: Option<&str>) {
        if let Some(value) = value {
            self.text(name, value);
        }
    }

    /// Writes `true`/`false`.
    pub fn bool(&mut self, name: &str, value: bool) {
        self.text(name, if value { "true" } else { "false" });
    }

    /// Writes a decimal in plain (non-scientific) notation.
    pub fn decimal(&mut self, name: &str, value: Decimal) {
        self.text(name, &value.to_string());
    }

    /// Writes an ISO `YYYY-MM-DD` date.
    pub fn date(&mut self, name: &str, value: Date) {
        self.text(name, &value.to_string());
    }

    /// Writes the element only when the value is present.
    pub fn date_opt(&mut self, name: &str, value: Option<Date>) {
        if let Some(value) = value {
            self.date(name, value);
        }
    }

    /// Writes the credential fields in wire order (`felhasznalo`, `jelszo`,
    /// `szamlaagentkulcs`).
    pub fn credentials(&mut self, credentials: &Credentials) {
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
    use serde::{Deserialize, Deserializer};

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
}
