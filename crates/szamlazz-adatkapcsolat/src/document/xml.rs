//! XML shape, independent of the lenient typed document. Like the Agent's XML
//! boundary, combine quick-xml's structural checks with xmlparser's document
//! token grammar: serde can stop at the first root and skip malformed extensions.
//! DTD declarations have their own grammar check before either reader: neither
//! backend validates their complete syntax, and declarations must not supply
//! defaults or expand entities into the business record.

use std::borrow::Cow;

use quick_xml::events::attributes::Attribute;
use quick_xml::events::{BytesDecl, BytesStart, Event};
use quick_xml::name::{Namespace, NamespaceResolver, PrefixDeclaration, QName, ResolveResult};

use crate::{ParseError, RootKind};

mod dtd;

fn invalid(message: impl Into<String>) -> ParseError {
    quick_xml::DeError::Custom(message.into()).into()
}

pub(super) fn validate(text: &str, kind: RootKind) -> Result<Cow<'_, str>, ParseError> {
    let without_dtd = dtd::without_declaration(text)?;
    let validated = validate_document(&without_dtd, kind)?;
    match without_dtd {
        Cow::Borrowed(_) => match validated {
            Cow::Borrowed(_) => Ok(Cow::Borrowed(text)),
            Cow::Owned(normalized) => Ok(Cow::Owned(normalized)),
        },
        Cow::Owned(_) => Ok(Cow::Owned(validated.into_owned())),
    }
}

fn validate_document(text: &str, kind: RootKind) -> Result<Cow<'_, str>, ParseError> {
    let mut reader = NamespaceReader::new(text);
    let mut depth = 0usize;
    let mut seen_root = false;
    let mut first = true;
    loop {
        let (namespace, event) = reader.read_resolved_event()?;
        if let Event::Start(start) | Event::Empty(start) = &event {
            check_namespace(namespace, start, kind)?;
        }
        let empty = matches!(event, Event::Empty(_));
        match event {
            Event::Start(_) | Event::Empty(_) if depth == 0 => {
                if seen_root {
                    return Err(invalid("multiple XML roots"));
                }
                seen_root = true;
                depth = usize::from(!empty);
            }
            Event::Start(_) => depth += 1,
            Event::End(_) => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| invalid("unexpected XML end tag"))?;
            }
            Event::Decl(decl) => {
                if !first {
                    return Err(invalid("misplaced XML declaration"));
                }
                validate_declaration(&decl)?;
            }
            Event::Text(value) if depth == 0 => {
                if !value.as_ref().chars().all(is_xml_space) {
                    return Err(invalid("text outside XML root"));
                }
            }
            Event::CData(_) | Event::GeneralRef(_) if depth == 0 => {
                return Err(invalid("content outside XML root"));
            }
            Event::DocType(_) => {
                return Err(invalid("misplaced XML document type declaration"));
            }
            Event::PI(pi) if !valid_pi_target(pi.target()) => {
                return Err(invalid("invalid XML processing instruction target"));
            }
            Event::Eof => {
                if depth != 0 || !seen_root {
                    return Err(invalid("incomplete XML document"));
                }
                return validate_lexical(text);
            }
            _ => {}
        }
        first = false;
    }
}

fn check_namespace(
    namespace: ResolveResult<'_>,
    start: &BytesStart<'_>,
    kind: RootKind,
) -> Result<(), ParseError> {
    if namespace != ResolveResult::Bound(Namespace(kind.namespace())) {
        let actual = match namespace {
            ResolveResult::Bound(namespace) => namespace.as_ref().to_owned(),
            ResolveResult::Unbound => String::new(),
            ResolveResult::Unknown(prefix) => format!("unbound prefix {prefix}"),
        };
        return Err(ParseError::WrongNamespace {
            root: start.local_name().as_ref().to_owned(),
            expected: kind.namespace(),
            actual,
        });
    }
    Ok(())
}

/// `NsReader` installs raw declarations before they can be normalized, including
/// its reserved-binding checks. Use the resolver directly, as the Agent does,
/// so attribute normalization happens exactly once, before namespace identity.
struct NamespaceReader<'a> {
    reader: quick_xml::Reader<&'a [u8]>,
    resolver: NamespaceResolver,
    pending_pop: bool,
}

impl<'a> NamespaceReader<'a> {
    fn new(text: &'a str) -> Self {
        let mut reader = quick_xml::Reader::from_str(text);
        reader.config_mut().check_comments = true;
        Self {
            reader,
            resolver: NamespaceResolver::default(),
            pending_pop: false,
        }
    }

    fn read_resolved_event(&mut self) -> Result<(ResolveResult<'_>, Event<'a>), ParseError> {
        if self.pending_pop {
            self.resolver.pop();
            self.pending_pop = false;
        }
        let event = self.reader.read_event().map_err(quick_xml::DeError::from)?;
        match &event {
            Event::Start(start) | Event::Empty(start) => {
                self.push(start)?;
                self.pending_pop = matches!(event, Event::Empty(_));
            }
            Event::End(_) => self.pending_pop = true,
            _ => {}
        }
        let namespace = match &event {
            Event::Start(start) | Event::Empty(start) => {
                self.resolver.resolve_element(start.name()).0
            }
            Event::End(end) => self.resolver.resolve_element(end.name()).0,
            _ => ResolveResult::Unbound,
        };
        Ok((namespace, event))
    }

    fn push(&mut self, start: &BytesStart<'_>) -> Result<(), ParseError> {
        const XML: &str = "http://www.w3.org/XML/1998/namespace";
        const XMLNS: &str = "http://www.w3.org/2000/xmlns/";
        if start.name().as_ref().starts_with("xmlns:") {
            return Err(invalid("reserved xmlns prefix on XML element"));
        }
        // End/empty events retain this scope until the next event.
        self.resolver
            .push(&BytesStart::new("scope"))
            .map_err(quick_xml::Error::from)
            .map_err(quick_xml::DeError::from)?;
        for attribute in start.attributes() {
            let attribute = attribute.map_err(quick_xml::DeError::from)?;
            if let Some(prefix) = attribute.key.as_namespace_binding() {
                let value = attribute
                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                    .map_err(quick_xml::DeError::from)?;
                if matches!(prefix, PrefixDeclaration::Default)
                    && matches!(value.as_ref(), XML | XMLNS)
                    || matches!(prefix, PrefixDeclaration::Named(_)) && value.is_empty()
                {
                    return Err(invalid("invalid XML namespace binding"));
                }
                self.resolver
                    .add(prefix, Namespace(&value))
                    .map_err(quick_xml::Error::from)
                    .map_err(quick_xml::DeError::from)?;
            }
        }
        let mut attributes = std::collections::HashSet::new();
        for attribute in start.attributes() {
            let attribute = attribute.map_err(quick_xml::DeError::from)?;
            if attribute.key.as_namespace_binding().is_some() {
                continue;
            }
            let (namespace, local) = self.resolver.resolve_attribute(attribute.key);
            let namespace = match namespace {
                ResolveResult::Bound(namespace) => Some(namespace.0),
                ResolveResult::Unbound => None,
                ResolveResult::Unknown(_) => {
                    return Err(invalid("undeclared XML attribute prefix"));
                }
            };
            if !attributes.insert((namespace, local)) {
                return Err(invalid("duplicate expanded XML attribute name"));
            }
        }
        Ok(())
    }
}

fn validate_lexical(text: &str) -> Result<Cow<'_, str>, ParseError> {
    // Covers literal characters in *every* token, including comments, CDATA,
    // instructions and attributes that typed deserialization never visits.
    if !text.chars().all(is_xml_character) {
        return Err(invalid("forbidden XML character"));
    }
    let mut normalized = String::new();
    let mut copied_to = 0;
    for token in xmlparser::Tokenizer::from(text) {
        let token =
            token.map_err(|error| invalid(format!("invalid XML syntax at {}", error.pos())))?;
        if let xmlparser::Token::Attribute {
            prefix,
            local,
            value,
            ..
        } = token
            && (prefix.as_str() == "xmlns" || prefix.is_empty() && local.as_str() == "xmlns")
        {
            let attribute = Attribute {
                key: QName("xmlns"),
                value: value.as_str().into(),
            };
            let binding = attribute
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map_err(quick_xml::DeError::from)?;
            if binding != value.as_str() {
                // serde owns another NsReader; give it the same normalized
                // declarations so its reserved-binding checks do not compare
                // raw references. Copy all business content byte-for-byte.
                normalized.push_str(&text[copied_to..value.start()]);
                normalized.push_str(
                    &quick_xml::escape::escape(binding)
                        .replace('\t', "&#9;")
                        .replace('\n', "&#10;")
                        .replace('\r', "&#13;"),
                );
                copied_to = value.end();
            }
        }
        let value = match token {
            xmlparser::Token::Text { text } => text,
            xmlparser::Token::Attribute { value, .. } => value,
            _ => continue,
        };
        // Do not fetch external DTDs or expand declared entities. As in the
        // typed parse, only predefined and character references are read.
        let decoded =
            quick_xml::escape::unescape(value.as_str()).map_err(quick_xml::DeError::from)?;
        if !decoded.chars().all(is_xml_character) {
            return Err(invalid("forbidden XML character reference"));
        }
    }
    if copied_to == 0 {
        Ok(Cow::Borrowed(text))
    } else {
        normalized.push_str(&text[copied_to..]);
        Ok(Cow::Owned(normalized))
    }
}

fn is_xml_character(ch: char) -> bool {
    matches!(ch, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}

fn is_xml_space(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\r' | '\n')
}

// xmlparser treats declarations separated by tab/CR/LF as PIs. As at the
// Agent boundary, validate the XML 1.0 declaration grammar for those too.
fn validate_declaration(decl: &BytesDecl<'_>) -> Result<(), ParseError> {
    if decl.version().map_err(quick_xml::DeError::from)?.as_ref() != "1.0" {
        return Err(invalid("invalid XML declaration"));
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
                return Err(invalid("invalid XML declaration"));
            }
            needs_space = false;
            if matches!(ch, '\'' | '"') {
                quote = Some(ch);
            }
        }
    }
    let mut last = 0;
    for attribute in start.attributes() {
        let attribute = attribute.map_err(quick_xml::DeError::from)?;
        let rank = match attribute.key.as_ref() {
            "version" if attribute.value.as_ref() == "1.0" => 1,
            "encoding" if valid_encoding_name(attribute.value.as_ref()) => 2,
            "standalone" if matches!(attribute.value.as_ref(), "yes" | "no") => 3,
            _ => return Err(invalid("invalid XML declaration")),
        };
        if rank <= last {
            return Err(invalid("invalid XML declaration"));
        }
        last = rank;
    }
    Ok(())
}

fn valid_pi_target(value: &str) -> bool {
    let mut chars = value.chars();
    !value.eq_ignore_ascii_case("xml")
        && chars.next().is_some_and(xml_ncname_start)
        && chars.all(|ch| xml_ncname_start(ch) || matches!(ch, '-' | '.' | '0'..='9' | '\u{b7}' | '\u{300}'..='\u{36f}' | '\u{203f}'..='\u{2040}'))
}

fn xml_ncname_start(ch: char) -> bool {
    matches!(ch, '_' | 'A'..='Z' | 'a'..='z' | '\u{c0}'..='\u{d6}' | '\u{d8}'..='\u{f6}' | '\u{f8}'..='\u{2ff}' | '\u{370}'..='\u{37d}' | '\u{37f}'..='\u{1fff}' | '\u{200c}'..='\u{200d}' | '\u{2070}'..='\u{218f}' | '\u{2c00}'..='\u{2fef}' | '\u{3001}'..='\u{d7ff}' | '\u{f900}'..='\u{fdcf}' | '\u{fdf0}'..='\u{fffd}' | '\u{10000}'..='\u{effff}')
}

fn valid_encoding_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}
