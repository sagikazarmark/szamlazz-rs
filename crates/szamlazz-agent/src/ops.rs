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
//! - **Written only when `true`** (`bool`, `#[serde(default)]`): the element
//!   is a flag whose absence is exactly `false` on the server, and a request
//!   that does not set it should read like one that never knew of it
//!   (`fizetve`, the `InvoiceKind` flags `dijbekero` / `elolegszamla` / …).
//! - **Tri-state** (`Option<bool>`): the server has a default the caller may
//!   want to leave in effect, so `None` omits the element and `Some(false)`
//!   sends an explicit `false` (`sendEmail`, `arresAfa`, `eusAfa`, `guardian`,
//!   `cikkazoninvoice`, `elonezetpdf`, `folyamatosTelj`).

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
