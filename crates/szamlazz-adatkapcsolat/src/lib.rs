//! Receiver for the szamlazz.hu **Online Pénzügyi Adatkapcsolat** (Financial
//! Data Connection) push protocol.
//!
//! szamlazz.hu POSTs XML documents (outgoing invoices, incoming invoices,
//! bank transactions, and daily receipt batches) to a single registered URL,
//! authenticated by the `X-Szamlazzhu-Key` header. The document type is
//! identified by the XML root element. The receiver must answer HTTP 200 with
//! a small response XML (an [`ack`](InvoiceAck)) echoing the document id;
//! any other status makes szamlazz.hu retry for up to 72 hours.
//!
//! `KEY_ERR` / `KEY_DEL` are *deliberate protocol speech*, not errors: they
//! tell szamlazz.hu the key is wrong (stop sending until it changes) or that
//! the connection should be severed. A bank transaction or receipt answered
//! `KEY_ERR` is never resent, an invoice only when it next changes, so they
//! are for a *definite* verdict, and anything uncertain is a non-200 that
//! keeps the retry window alive. Express them via the ack constructors.
//!
//! A push is at-most-N-times delivery, and a non-200 szamlazz.hu retries
//! identically for 72 hours loses the record. So [`Document::parse`] refuses
//! only what the receiver cannot Ack (shape, never content): an element the
//! XSD requires but the push omits is `None`, an unknown enumeration token is
//! kept, a PDF that does not decode is `None` beside the raw XML. The XSD's
//! verdict is a signal a receiver can ask for ([`Document::validate`]) or
//! make a gate of ([`Document::parse_strict`]).
//!
//! The core is framework-free and `wasm32`-clean: [`Document::parse`] takes
//! raw body bytes, ack types render response bodies. Implement [`Handler`]
//! for your business logic; with the `axum` feature, `axum::router` wires
//! everything (key check included) into a ready `Router`.
//!
//! # Quick start
//!
//! Parse the pushed body and dispatch on its root element:
//!
//! ```
//! use szamlazz_adatkapcsolat::Document;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let body = br#"<banktranz xmlns="http://www.szamlazz.hu/banktranz">
//!   <id>987</id><bankszamla>11111111-22222222</bankszamla>
//!   <erteknap>2026-07-03</erteknap><irany>BE</irany><technikai>false</technikai>
//!   <osszeg>12700</osszeg><devizanem>HUF</devizanem>
//! </banktranz>"#;
//! let Document::BankTransaction(transaction) = Document::parse(body)? else {
//!     return Err("unexpected document kind".into());
//! };
//!
//! assert_eq!(transaction.id, 987);
//! # Ok(())
//! # }
//! ```
//!
//! # Features
//!
//! Default features are empty and provide document parsing, Acks, handlers,
//! and fan-out on native and `wasm32-unknown-unknown` targets.
//!
//! - `axum` adds router wiring for authentication, parsing, dispatch, and Ack
//!   rendering, with a 64 MiB request-body cap by default. It supports native
//!   servers and single-threaded wasm runtimes; wasm handler futures are
//!   protected by `send_wrapper` thread checks.
//! - `opendal` adds the archival handler and JSON persistence. Applications
//!   enable the required storage services on their own `opendal` dependency.
//!   The selected service determines platform support; timestamped archive
//!   layouts on wasm also require jiff's `js` feature.
// docs.rs builds with all features on nightly and sets `--cfg docsrs`;
// current rustdoc's doc_cfg automatically annotates feature- and target gates.
#![cfg_attr(docsrs, feature(doc_cfg))]

mod ack;
mod document;
mod error;
mod fanout;
mod handler;

#[cfg(feature = "opendal")]
pub mod archive;
#[cfg(feature = "axum")]
pub mod axum;

pub use ack::{Ack, ControlCode, InvoiceAck, InvoiceDirection};
pub use document::{
    Address, Bank, BankTransaction, BuyerLedger, Document, FinancialItem, InvoiceAppearance,
    InvoiceDocument, InvoiceInfo, InvoiceItem, InvoiceItemLedger, Party, Pdf, ReceiptBatch,
    ReceiptDocument, ReceiptInfo, ReceiptItem, ReceiptItemLedger, ReceiptPayment, RecordedPayment,
    Totals, TransactionDirection, TransactionPartner, VatRate, VatTotal,
};
pub use error::{AckError, ParseError, ValidationError, XmlError};
pub use fanout::{Fanout, FanoutError, HandlerFailure};
pub use handler::{Handler, MaybeSend, MaybeSync};

/// The header carrying the connection's identifier key.
pub const KEY_HEADER: &str = "X-Szamlazzhu-Key";
