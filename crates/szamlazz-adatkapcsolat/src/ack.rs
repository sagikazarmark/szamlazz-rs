//! Acks: the response XML a receiver returns for a pushed document.
//!
//! `KEY_ERR` / `KEY_DEL` are deliberate protocol speech (constructors on the
//! ack types), while transient failures are expressed by *not* acking
//! (non-200), which makes szamlazz.hu retry for up to 72 hours.

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};

use crate::AckError;
use crate::document::RootKind;

const WRITE_EXPECT: &str = "writing XML to an in-memory buffer cannot fail";

/// A control code sent instead of a normal acknowledgement: deliberate
/// protocol speech, not an error (an error is a non-200, which szamlazz.hu
/// retries).
///
/// Breaking change in 0.4: `KeyError` is [`KeyUnknown`](Self::KeyUnknown).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ControlCode {
    /// `KEY_ERR`: the presented key is unknown to this receiver; szamlazz.hu
    /// stops sending but keeps the connection. The document answered this way
    /// is **never resent** if it is a bank transaction or a receipt, and an
    /// invoice only when it next changes, so send it for a definite verdict,
    /// never for a check that could not complete (answer a non-200 instead).
    KeyUnknown,
    /// `KEY_DEL`: sever the connection; the account owner is notified by
    /// email. A few in-flight documents may still arrive afterwards.
    Disconnect,
}

impl ControlCode {
    /// The wire token: `KEY_ERR` or `KEY_DEL`.
    #[must_use]
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::KeyUnknown => "KEY_ERR",
            Self::Disconnect => "KEY_DEL",
        }
    }

    /// Renders this control code as the Ack of the pushed `kind`: the one
    /// Ack shape every kind shares, so a receiver that has identified a push
    /// ([`Document::identify`](crate::Document::identify)) can answer
    /// `KEY_ERR` or `KEY_DEL` without parsing (or authenticating) the body.
    /// Infallible: a control Ack carries no caller text.
    #[must_use]
    pub fn to_xml(self, kind: RootKind) -> Vec<u8> {
        render(kind, |writer| write_leaf(writer, "hibakod", self.as_wire()))
    }
}

/// Which invoice stream a pushed invoice arrived on: selects the Ack root an
/// [`InvoiceAck`] renders and the XSD
/// [`InvoiceDocument::validate`](crate::InvoiceDocument::validate) checks
/// against (`szamla.xsd` or `szamlabe.xsd`). The invoice half of
/// [`RootKind`] ([`RootKind::direction`]); exhaustive for the same reason.
/// Breaking change in 0.4: the enum was `#[non_exhaustive]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InvoiceDirection {
    /// Outgoing invoices (`<szamla>` → `<szamlavalasz>`).
    Outgoing,
    /// Incoming invoices (`<szamlabe>` → `<szamlabevalasz>`).
    Incoming,
}

/// Acknowledgement of a pushed invoice: echoes the document id, optionally
/// with the registration number your system assigned.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct InvoiceAck {
    id: Option<i32>,
    registration_number: Option<String>,
    control: Option<ControlCode>,
}

impl InvoiceAck {
    /// Accepts the document, echoing its `alap`/`id`.
    ///
    /// `id` matters when you render the Ack yourself: pass the pushed
    /// invoice's [`info.id`](crate::InvoiceInfo::id). Under the `axum` router
    /// it is overridden, because a successful Ack must echo the pushed id
    /// whatever a handler supplied ([`for_document`](Self::for_document)).
    pub fn accept(id: i32) -> Self {
        Self {
            id: Some(id),
            registration_number: None,
            control: None,
        }
    }

    /// Adds the registration number (`iktatoszam`) your system assigned; it
    /// is recorded on the szamlazz.hu side.
    #[doc(alias = "iktatószám")]
    pub fn with_registration_number(mut self, number: impl Into<String>) -> Self {
        self.registration_number = Some(number.into());
        self
    }

    /// Answers `KEY_ERR`: the key is unknown, stop sending until it changes.
    pub fn key_unknown() -> Self {
        Self {
            id: None,
            registration_number: None,
            control: Some(ControlCode::KeyUnknown),
        }
    }

    /// Answers `KEY_DEL`: sever the connection.
    pub fn disconnect() -> Self {
        Self {
            id: None,
            registration_number: None,
            control: Some(ControlCode::Disconnect),
        }
    }

    /// Decomposes the ack for merging (fan-out).
    pub(crate) fn parts(&self) -> (Option<i32>, Option<&str>, Option<ControlCode>) {
        (self.id, self.registration_number.as_deref(), self.control)
    }

    /// Binds the Ack to the document it answers: a successful Ack echoes the
    /// pushed `id` whatever [`accept`](Self::accept) was given, and a control
    /// Ack carries no invoice metadata. The `axum` router calls this on every
    /// handler-produced Ack; call it yourself when dispatching without it.
    /// Breaking change in 0.4: public, and no longer behind the `axum`
    /// feature.
    pub fn for_document(mut self, id: i32) -> Self {
        if self.control.is_some() {
            self.id = None;
            self.registration_number = None;
        } else {
            self.id = Some(id);
        }
        self
    }

    /// Renders the Ack for the given invoice stream.
    ///
    /// # Errors
    ///
    /// Returns an error when the registration number contains a character
    /// forbidden by XML 1.0.
    pub fn to_xml(&self, direction: InvoiceDirection) -> Result<Vec<u8>, AckError> {
        if let Some(number) = &self.registration_number {
            validate_xml_10(number)?;
        }

        Ok(render(direction.into(), |writer| {
            if self.id.is_some() || self.registration_number.is_some() {
                write_start(writer, "alap");
                if let Some(id) = self.id {
                    write_leaf(writer, "id", &id.to_string());
                }
                if let Some(number) = &self.registration_number {
                    write_leaf(writer, "iktatoszam", number);
                }
                write_end(writer, "alap");
            }
            if let Some(control) = self.control {
                write_leaf(writer, "hibakod", control.as_wire());
            }
        }))
    }
}

/// Acknowledgement of a pushed bank transaction or receipt batch: bare
/// success, or a control code.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct Ack {
    control: Option<ControlCode>,
}

impl Ack {
    /// Accepts the document.
    pub fn accept() -> Self {
        Self { control: None }
    }

    /// Answers `KEY_ERR`: the key is unknown; the record is not resent.
    pub fn key_unknown() -> Self {
        Self {
            control: Some(ControlCode::KeyUnknown),
        }
    }

    /// Answers `KEY_DEL`: sever the connection.
    pub fn disconnect() -> Self {
        Self {
            control: Some(ControlCode::Disconnect),
        }
    }

    /// The control code, for merging (fan-out).
    pub(crate) fn control_code(&self) -> Option<ControlCode> {
        self.control
    }

    /// Renders the `<banktranzvalasz>` Ack.
    #[must_use]
    pub fn to_bank_transaction_xml(&self) -> Vec<u8> {
        self.to_xml(RootKind::BankTransaction)
    }

    /// Renders the `<nyugtavalasz>` Ack.
    #[must_use]
    pub fn to_receipts_xml(&self) -> Vec<u8> {
        self.to_xml(RootKind::Receipts)
    }

    fn to_xml(&self, kind: RootKind) -> Vec<u8> {
        render(kind, |writer| {
            if let Some(control) = self.control {
                write_leaf(writer, "hibakod", control.as_wire());
            }
        })
    }
}

/// Writes the Ack envelope of `kind` (declaration, namespaced root) around
/// whatever `build` writes.
fn render(kind: RootKind, build: impl FnOnce(&mut Writer<Vec<u8>>)) -> Vec<u8> {
    let root = kind.ack_root_element();
    let mut writer = Writer::new(Vec::new());
    writer
        .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
        .expect(WRITE_EXPECT);
    let mut start = BytesStart::new(root);
    start.push_attribute(("xmlns", format!("http://www.szamlazz.hu/{root}").as_str()));
    writer.write_event(Event::Start(start)).expect(WRITE_EXPECT);
    build(&mut writer);
    writer
        .write_event(Event::End(BytesEnd::new(root)))
        .expect(WRITE_EXPECT);

    writer.into_inner()
}

fn write_start(writer: &mut Writer<Vec<u8>>, name: &str) {
    writer
        .write_event(Event::Start(BytesStart::new(name)))
        .expect(WRITE_EXPECT);
}

fn write_end(writer: &mut Writer<Vec<u8>>, name: &str) {
    writer
        .write_event(Event::End(BytesEnd::new(name)))
        .expect(WRITE_EXPECT);
}

fn write_leaf(writer: &mut Writer<Vec<u8>>, name: &str, value: &str) {
    write_start(writer, name);
    writer
        .write_event(Event::Text(BytesText::new(value)))
        .expect(WRITE_EXPECT);
    write_end(writer, name);
}

fn validate_xml_10(value: &str) -> Result<(), AckError> {
    if let Some(character) = value
        .chars()
        .find(|&character| !is_xml_10_character(character))
    {
        return Err(AckError::InvalidXmlCharacter {
            codepoint: character as u32,
        });
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

    fn text(xml: Vec<u8>) -> String {
        String::from_utf8(xml).expect("utf-8")
    }

    #[test]
    fn invoice_ack_echoes_id_and_registration_number() {
        let xml = InvoiceAck::accept(1001)
            .with_registration_number("IKT-20260704")
            .to_xml(InvoiceDirection::Outgoing)
            .expect("valid Ack");
        assert_eq!(
            text(xml),
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <szamlavalasz xmlns=\"http://www.szamlazz.hu/szamlavalasz\">\
             <alap><id>1001</id><iktatoszam>IKT-20260704</iktatoszam></alap>\
             </szamlavalasz>"
        );
    }

    #[test]
    fn incoming_direction_switches_root() {
        let xml = text(
            InvoiceAck::accept(7)
                .to_xml(InvoiceDirection::Incoming)
                .expect("valid Ack"),
        );
        assert!(xml.contains("<szamlabevalasz xmlns=\"http://www.szamlazz.hu/szamlabevalasz\">"));
        assert!(!xml.contains("iktatoszam"));
    }

    #[test]
    fn key_unknown_renders_control_code_only() {
        let xml = text(
            InvoiceAck::key_unknown()
                .to_xml(InvoiceDirection::Outgoing)
                .expect("valid Ack"),
        );
        assert!(xml.contains("<hibakod>KEY_ERR</hibakod>"));
        assert!(!xml.contains("<alap>"));
    }

    #[test]
    fn for_document_binds_the_pushed_id_and_strips_control_acks() {
        let bound = InvoiceAck::accept(-1)
            .with_registration_number("IKT-1")
            .for_document(42);
        assert_eq!(bound.parts(), (Some(42), Some("IKT-1"), None));

        let control = InvoiceAck::key_unknown().for_document(42);
        assert_eq!(control.parts(), (None, None, Some(ControlCode::KeyUnknown)));
        let control = InvoiceAck::disconnect()
            .with_registration_number("dropped")
            .for_document(42);
        assert_eq!(control.parts(), (None, None, Some(ControlCode::Disconnect)));
    }

    #[test]
    fn bare_acks() {
        let xml = text(Ack::accept().to_bank_transaction_xml());
        assert!(xml.contains("<banktranzvalasz xmlns=\"http://www.szamlazz.hu/banktranzvalasz\">"));
        assert!(!xml.contains("hibakod"));
        let xml = text(Ack::disconnect().to_receipts_xml());
        assert!(xml.contains("<nyugtavalasz"));
        assert!(xml.contains("<hibakod>KEY_DEL</hibakod>"));
    }

    #[test]
    fn control_code_renders_the_ack_of_every_kind() {
        for (kind, root) in [
            (RootKind::OutgoingInvoice, "szamlavalasz"),
            (RootKind::IncomingInvoice, "szamlabevalasz"),
            (RootKind::BankTransaction, "banktranzvalasz"),
            (RootKind::Receipts, "nyugtavalasz"),
        ] {
            let xml = text(ControlCode::KeyUnknown.to_xml(kind));
            assert_eq!(
                xml,
                format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
                     <{root} xmlns=\"http://www.szamlazz.hu/{root}\">\
                     <hibakod>KEY_ERR</hibakod></{root}>"
                )
            );
            assert!(
                text(ControlCode::Disconnect.to_xml(kind)).contains("<hibakod>KEY_DEL</hibakod>")
            );
        }

        // The same bytes as the typed Acks render for the same code.
        assert_eq!(
            ControlCode::KeyUnknown.to_xml(RootKind::OutgoingInvoice),
            InvoiceAck::key_unknown()
                .to_xml(InvoiceDirection::Outgoing)
                .expect("valid Ack")
        );
        assert_eq!(
            ControlCode::Disconnect.to_xml(RootKind::Receipts),
            Ack::disconnect().to_receipts_xml()
        );
    }

    #[test]
    fn rejects_xml_10_forbidden_registration_number() {
        let error = InvoiceAck::accept(1)
            .with_registration_number("invalid\0number")
            .to_xml(InvoiceDirection::Outgoing)
            .expect_err("invalid XML text");
        assert_eq!(error, AckError::InvalidXmlCharacter { codepoint: 0 });
    }
}
