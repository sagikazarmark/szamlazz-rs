//! XML 1.0 doctypedecl/internal-subset grammar, without entity expansion or
//! validity checks. xmlparser deliberately skips ELEMENT/ATTLIST/NOTATION;
//! checking their delimiters alone neither validates them nor handles quotes.
//! Productions follow XML 1.0 §2.8–4.2 with Namespaces in XML §6's QName/NCName
//! constraints. DTD prefixes are lexical names, not namespace lookups.

use std::borrow::Cow;

use super::{invalid, is_xml_character, is_xml_space, valid_pi_target};
use crate::ParseError;

/// Validate a prolog DTD, then hide it from the document tokenizer and serde.
/// It supplies no defaults, namespace bindings, or entities to the document.
pub(super) fn without_declaration(text: &str) -> Result<Cow<'_, str>, ParseError> {
    for token in xmlparser::Tokenizer::from(text) {
        let token = token.map_err(|e| invalid(format!("invalid XML syntax at {}", e.pos())))?;
        match token {
            xmlparser::Token::DtdStart { span, .. } | xmlparser::Token::EmptyDtd { span, .. } => {
                let start = span.start();
                let mut parser = Dtd {
                    rest: &text[start..],
                };
                parser.declaration()?;
                let end = text.len() - parser.rest.len();
                let mut output = String::with_capacity(text.len());
                output.push_str(&text[..start]);
                // Keep byte offsets and line numbers, including for UTF-8.
                output.extend(
                    text[start..end]
                        .bytes()
                        .map(|b| if b == b'\n' { '\n' } else { ' ' }),
                );
                output.push_str(&text[end..]);
                return Ok(Cow::Owned(output));
            }
            xmlparser::Token::ElementStart { .. } => break,
            _ => {}
        }
    }
    Ok(Cow::Borrowed(text))
}

struct Dtd<'a> {
    rest: &'a str,
}

impl<'a> Dtd<'a> {
    fn take(&mut self, token: &str) -> bool {
        if let Some(rest) = self.rest.strip_prefix(token) {
            self.rest = rest;
            true
        } else {
            false
        }
    }

    fn need(&mut self, token: &str) -> Result<(), ParseError> {
        if self.take(token) {
            Ok(())
        } else {
            Err(invalid("invalid DTD declaration syntax"))
        }
    }

    fn space(&mut self) -> bool {
        let trimmed = self.rest.trim_start_matches(is_xml_space);
        let found = trimmed.len() != self.rest.len();
        self.rest = trimmed;
        found
    }

    fn separator(&mut self) -> Result<(), ParseError> {
        if self.space() {
            Ok(())
        } else {
            Err(invalid("missing DTD whitespace"))
        }
    }

    fn name(&mut self) -> Result<&'a str, ParseError> {
        let mut stream = xmlparser::Stream::from(self.rest);
        let name = stream
            .consume_name()
            .map_err(|_| invalid("invalid DTD name"))?;
        self.rest = &self.rest[stream.pos()..];
        Ok(name.as_str())
    }

    fn qname(&mut self) -> Result<&'a str, ParseError> {
        let name = self.name()?;
        let valid = name.split_once(':').map_or_else(
            || is_ncname(name),
            |(prefix, local)| is_ncname(prefix) && is_ncname(local),
        );
        if !valid {
            return Err(invalid("invalid DTD qualified name"));
        }
        Ok(name)
    }

    fn ncname(&mut self) -> Result<&'a str, ParseError> {
        let name = self.name()?;
        if !is_ncname(name) {
            return Err(invalid("invalid DTD unqualified name"));
        }
        Ok(name)
    }

    fn literal(&mut self) -> Result<&'a str, ParseError> {
        let quote = if self.take("'") {
            '\''
        } else {
            self.need("\"")?;
            '"'
        };
        let end = self
            .rest
            .find(quote)
            .ok_or_else(|| invalid("unclosed DTD literal"))?;
        let value = &self.rest[..end];
        self.rest = &self.rest[end + 1..];
        if !value.chars().all(is_xml_character) {
            return Err(invalid("forbidden XML character in DTD"));
        }
        Ok(value)
    }

    fn declaration(&mut self) -> Result<(), ParseError> {
        self.need("<!DOCTYPE")?;
        self.separator()?;
        self.qname()?;
        if self.space() && !self.rest.starts_with(['[', '>']) {
            self.external_id(false)?;
            self.space();
        }
        if self.take("[") {
            loop {
                self.space();
                if self.take("]") {
                    break;
                }
                if self.take("<!ELEMENT") {
                    self.element()?;
                } else if self.take("<!ATTLIST") {
                    self.attlist()?;
                } else if self.take("<!ENTITY") {
                    self.entity()?;
                } else if self.take("<!NOTATION") {
                    self.separator()?;
                    self.ncname()?;
                    self.separator()?;
                    self.external_id(true)?;
                    self.end()?;
                } else if self.take("<!--") {
                    let end = self
                        .rest
                        .find("--")
                        .ok_or_else(|| invalid("unclosed DTD comment"))?;
                    if !self.rest[..end].chars().all(is_xml_character) {
                        return Err(invalid("forbidden DTD comment character"));
                    }
                    self.rest = &self.rest[end..];
                    self.need("-->")?;
                } else if self.take("<?") {
                    let target = self.ncname()?;
                    if !valid_pi_target(target) {
                        return Err(invalid("invalid DTD processing instruction target"));
                    }
                    if !self.take("?>") {
                        self.separator()?;
                        let end = self
                            .rest
                            .find("?>")
                            .ok_or_else(|| invalid("unclosed DTD processing instruction"))?;
                        if !self.rest[..end].chars().all(is_xml_character) {
                            return Err(invalid("forbidden DTD instruction character"));
                        }
                        self.rest = &self.rest[end + 2..];
                    }
                } else if self.take("%") {
                    // DeclSep permits PEReference. Validate its spelling but
                    // never interpret replacement text or load external data.
                    self.ncname()?;
                    self.need(";")?;
                } else {
                    return Err(invalid("invalid DTD internal subset"));
                }
            }
            self.space();
        }
        self.need(">")
    }

    fn end(&mut self) -> Result<(), ParseError> {
        self.space();
        self.need(">")
    }

    fn external_id(&mut self, notation: bool) -> Result<(), ParseError> {
        if self.take("SYSTEM") {
            self.separator()?;
            self.literal()?;
        } else {
            self.need("PUBLIC")?;
            self.separator()?;
            let public = self.literal()?;
            if !public
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || " \r\n-'()+,./:=?;!*#@$_%".contains(c))
            {
                return Err(invalid("invalid DTD public identifier"));
            }
            let space = self.space();
            if !notation || self.rest.starts_with(['\'', '"']) {
                if !space {
                    return Err(invalid("missing DTD external identifier whitespace"));
                }
                self.literal()?;
            }
        }
        Ok(())
    }

    fn entity(&mut self) -> Result<(), ParseError> {
        self.separator()?;
        let parameter = self.take("%");
        if parameter {
            self.separator()?;
        }
        self.ncname()?;
        self.separator()?;
        if self.rest.starts_with(['\'', '"']) {
            references(self.literal()?, false)?;
        } else {
            self.external_id(false)?;
            if self.space() && self.take("NDATA") {
                if parameter {
                    return Err(invalid("NDATA on parameter entity"));
                }
                self.separator()?;
                self.ncname()?;
            }
        }
        self.end()
    }

    fn attlist(&mut self) -> Result<(), ParseError> {
        self.separator()?;
        self.qname()?;
        loop {
            let space = self.space();
            if self.take(">") {
                return Ok(());
            }
            if !space {
                return Err(invalid("missing DTD attribute separator"));
            }
            self.qname()?;
            self.separator()?;
            if self.take("(") {
                self.enumeration(false)?;
            } else {
                match self.name()? {
                    "NOTATION" => {
                        self.separator()?;
                        self.need("(")?;
                        self.enumeration(true)?;
                    }
                    "CDATA" | "ID" | "IDREF" | "IDREFS" | "ENTITY" | "ENTITIES" | "NMTOKEN"
                    | "NMTOKENS" => {}
                    _ => return Err(invalid("invalid DTD attribute type")),
                }
            }
            self.separator()?;
            if !(self.take("#REQUIRED") || self.take("#IMPLIED")) {
                if self.take("#FIXED") {
                    self.separator()?;
                }
                references(self.literal()?, true)?;
            }
        }
    }

    fn enumeration(&mut self, names: bool) -> Result<(), ParseError> {
        loop {
            self.space();
            if names {
                self.ncname()?;
            } else {
                // Nmtoken uses NameChar but permits a non-NameStartChar first.
                let end = self
                    .rest
                    .find(|c: char| !name_char(c))
                    .unwrap_or(self.rest.len());
                if end == 0 {
                    return Err(invalid("empty DTD enumeration token"));
                }
                self.rest = &self.rest[end..];
            }
            self.space();
            if self.take(")") {
                return Ok(());
            }
            self.need("|")?;
        }
    }

    fn element(&mut self) -> Result<(), ParseError> {
        self.separator()?;
        self.qname()?;
        self.separator()?;
        if !(self.take("EMPTY") || self.take("ANY")) {
            self.need("(")?;
            self.space();
            if self.take("#PCDATA") {
                self.space();
                let mut names = false;
                while self.take("|") {
                    names = true;
                    self.space();
                    self.qname()?;
                    self.space();
                }
                self.need(")")?;
                if names {
                    self.need("*")?;
                } else {
                    self.take("*");
                }
            } else {
                // Explicit stack: arbitrarily nested content models must not
                // overflow the receiver's call stack. Each group uses either
                // choice or sequence separators, never both.
                let mut groups = vec![None];
                loop {
                    self.space();
                    if self.take("(") {
                        groups.push(None);
                        continue;
                    }
                    self.qname()?;
                    loop {
                        let _ = self.take("?") || self.take("*") || self.take("+");
                        self.space();
                        if self.take(")") {
                            groups.pop();
                            if groups.is_empty() {
                                break;
                            }
                        } else {
                            let separator = if self.take("|") {
                                '|'
                            } else {
                                self.need(",")?;
                                ','
                            };
                            let group = groups.last_mut().expect("open DTD content group");
                            if group.is_some_and(|previous| previous != separator) {
                                return Err(invalid("mixed DTD content separators"));
                            }
                            *group = Some(separator);
                            break;
                        }
                    }
                    if groups.is_empty() {
                        break;
                    }
                }
                let _ = self.take("?") || self.take("*") || self.take("+");
            }
        }
        self.end()
    }
}

fn name_char(c: char) -> bool {
    super::xml_ncname_start(c)
        || matches!(c, ':' | '-' | '.' | '0'..='9' | '\u{b7}' | '\u{300}'..='\u{36f}' | '\u{203f}'..='\u{2040}')
}

fn is_ncname(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(super::xml_ncname_start) && chars.all(|c| c != ':' && name_char(c))
}

/// `EntityValue` permits literal '<' and named references; `AttValue` forbids '<'.
/// A parameter reference inside a declaration is forbidden in an internal
/// subset (XML WFC: PEs in Internal Subset). Neither kind is expanded here.
fn references(mut text: &str, attribute: bool) -> Result<(), ParseError> {
    while !text.is_empty() {
        if text.starts_with('&') {
            let end = text
                .find(';')
                .ok_or_else(|| invalid("unclosed DTD reference"))?;
            let reference = &text[1..end];
            if let Some(number) = reference.strip_prefix('#') {
                let (digits, radix) = number.strip_prefix('x').map_or((number, 10), |s| (s, 16));
                if digits.is_empty()
                    || !digits.chars().all(|c| {
                        if radix == 16 {
                            c.is_ascii_hexdigit()
                        } else {
                            c.is_ascii_digit()
                        }
                    })
                {
                    return Err(invalid("invalid DTD character reference"));
                }
                let ch = u32::from_str_radix(digits, radix)
                    .ok()
                    .and_then(char::from_u32);
                if !ch.is_some_and(is_xml_character) {
                    return Err(invalid("forbidden DTD character reference"));
                }
            } else {
                let mut parser = Dtd { rest: reference };
                parser.ncname()?;
                if !parser.rest.is_empty() {
                    return Err(invalid("invalid DTD entity reference"));
                }
            }
            text = &text[end + 1..];
        } else {
            let ch = text.chars().next().expect("nonempty DTD value");
            if attribute && ch == '<' || !attribute && ch == '%' {
                return Err(invalid("forbidden character in DTD value"));
            }
            text = &text[ch.len_utf8()..];
        }
    }
    Ok(())
}
