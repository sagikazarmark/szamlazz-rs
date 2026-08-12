//! Sans-IO client for the [szamlazz.hu Számla Agent](https://docs.szamlazz.hu/) XML API.
//!
//! The core of this crate performs no I/O: request types serialize into a
//! ready-to-send [`WireRequest`](wire::WireRequest) and responses are parsed
//! from raw headers and body bytes, so any HTTP client on any platform —
//! including `wasm32-unknown-unknown` and Cloudflare Workers — can drive it.
//! Enable the `client-reqwest` feature for a ready-made async client.
//!
//! Identifiers are English; every type documents its Hungarian wire name and
//! is findable in rustdoc search by that name via doc aliases.
//!
//! # Quick start
//!
//! Build a complete HTTP request with the framework-free core:
//!
//! ```
//! use szamlazz_agent::ops::taxpayer::QueryTaxpayer;
//! use szamlazz_agent::wire::{AgentRequest, ENDPOINT};
//! use szamlazz_agent::Credentials;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let request = QueryTaxpayer::new("12345678")?;
//! let wire = request.to_wire(&Credentials::agent_key("your-agent-key"))?;
//!
//! assert_eq!(wire.url, ENDPOINT);
//! assert!(wire.content_type.starts_with("multipart/form-data"));
//! # Ok(())
//! # }
//! ```
//!
//! Send the URL, content type, and body through your HTTP stack, then pass its
//! headers and body to [`AgentRequest::parse`](wire::AgentRequest::parse).
//!
//! # Features
//!
//! Default features are empty and provide the sans-IO request/response core.
//! The core works on native targets and `wasm32-unknown-unknown`; filesystem
//! helpers such as `Pdf::save_to` are available only on non-wasm targets.
//!
//! - `client-reqwest` adds the ready-made async `Client`. It supports native
//!   and browser wasm targets. On native targets it manages the session cookie,
//!   timeout, TLS, and redirect policy; on wasm the browser controls cookies
//!   and redirects.
// docs.rs builds with all features on nightly and sets `--cfg docsrs`;
// current rustdoc's doc_cfg automatically annotates feature- and target gates.
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "client-reqwest")]
pub mod client;
pub mod credentials;
pub mod error;
pub mod item;
pub mod ops;
pub mod types;
pub mod wire;
mod xml;

#[cfg(feature = "client-reqwest")]
pub use client::{Client, ClientError};

pub use credentials::{AgentKey, Credentials};
pub use error::{ApiError, ErrorCode, ParseError, RequestError, ResponseError, XmlError};
pub use item::{LineItem, LineItemLedger, MAX_ERASURE_CODE_COUNT};
pub use types::{
    Currency, InvoiceNumber, Language, PaymentMethod, Pdf, ReceiptNumber, TaxpayerStatus, VatRate,
};

/// Calendar date type used across the API (re-exported from [`jiff`]).
pub use jiff::civil::Date;
