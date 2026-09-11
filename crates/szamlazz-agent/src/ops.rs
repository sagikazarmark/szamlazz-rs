//! One module per Számla Agent operation: the request type, its hand-written
//! XML writer, and its response parser live together. What more than one
//! operation shares is modelled once and imported by each: the verdict
//! envelope and the `osszegek` totals block in `crate::xml`, the
//! `xmlszamlavalasz` reply in the crate-private `envelope` module, and the
//! request vocabulary more than one operation sends in [`crate::types`]
//! (`ExchangeRate`, `InvoiceTemplate`, `SellerEmail`, `InvoiceSelector`).
//! No operation imports from a sibling operation's module.
//!
//! # Wire booleans
//!
//! A boolean element is written in one of three ways, by what its absence
//! means on the server:
//!
//! - **Always written** (`bool`): the element is a required part of the
//!   request and the server has no default the crate would want to lean on
//!   (`eszamla`, `szamlaLetoltes`, `pdfLetoltes`, `additiv`, `pdf`).
//! - **Written only when `true`**: the writer selects the `InvoiceKind` flags
//!   only when enabled (`dijbekero` / `elolegszamla` / …).
//! - **Tri-state** (`Option<bool>`): the server has a default the caller may
//!   want to leave in effect, so `None` omits the element and `Some(false)`
//!   sends an explicit `false`, while `Some(true)` sends `true` (`fizetve`,
//!   `sendEmail`, `arresAfa`, `eusAfa`, `guardian`, `cikkazoninvoice`,
//!   `elonezetpdf`, `folyamatosTelj`). Omission and explicit false are not
//!   assumed to be equivalent. See [`invoice::InvoiceHeader::paid`] for the
//!   migration from its former boolean shape.

/// The *Response version* (`valaszVerzio`) every operation that has one
/// requests: `2`, the structured `xmlszamlavalasz` XML with a base64 PDF
/// (`1` is plain text or raw PDF bytes, which the parsers do not read). One
/// constant so the pin is greppable; four writers send it.
pub const RESPONSE_VERSION: &str = "2";

pub mod credit_entry;
mod envelope;
pub mod invoice;
pub mod proforma;
pub mod query_pdf;
pub mod query_xml;
pub mod receipt;
pub mod storno;
pub mod taxpayer;
mod waybill;
