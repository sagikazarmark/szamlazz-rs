//! Client for the [szamlazz.hu Számla Agent](https://docs.szamlazz.hu/) XML API.
//!
//! The core of this crate performs no I/O: request types serialize into a
//! ready-to-send [`WireRequest`](wire::WireRequest) and responses are parsed
//! from raw headers and body bytes, so any HTTP client on any platform
//! (including `wasm32-unknown-unknown` and Cloudflare Workers) can drive it.
//! Enable the `client-reqwest` feature for a ready-made async client.
//!
//! Identifiers are English; every type documents its Hungarian wire name and
//! is findable in rustdoc search by that name via doc aliases.
//!
//! # Quick start
//!
//! Build a complete HTTP request body with the framework-free core:
//!
//! ```
//! use szamlazz_agent::ops::taxpayer::QueryTaxpayer;
//! use szamlazz_agent::wire::AgentRequest;
//! use szamlazz_agent::Credentials;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let request = QueryTaxpayer::new("12345678")?;
//! let wire = request.to_wire(&Credentials::agent_key("your-agent-key"))?;
//!
//! assert!(wire.content_type.starts_with("multipart/form-data"));
//! assert!(!wire.body.is_empty());
//! # Ok(())
//! # }
//! ```
//!
//! POST the body with that content type to [`wire::ENDPOINT`] through your
//! HTTP stack, then pass the response's headers and body to
//! [`RawResponse::new`](wire::RawResponse::new) and
//! [`AgentRequest::parse`](wire::AgentRequest::parse). The README shows the
//! full round trip with a non-reqwest client.
//!
//! Request types are plain data: build them as struct literals, or extend a
//! constructor's result with functional update
//! (`CreateInvoice { external_id: Some(..), ..CreateInvoice::new(..) }`).
//! Response types are `#[non_exhaustive]`, since szamlazz.hu grows them.
//!
//! # Features
//!
//! Default features are empty and provide the I/O-free request/response core.
//! The core works on native targets and `wasm32-unknown-unknown`; filesystem
//! helpers such as `Pdf::save_to` are available only on non-wasm targets.
//!
//! - `client-reqwest` adds the ready-made async `Client`. It supports native
//!   and browser wasm targets. On native targets it manages the session cookie,
//!   timeout (`client::REQUEST_TIMEOUT`), TLS, and redirect policy; on wasm the
//!   browser controls cookies and redirects. The feature re-exports [`reqwest`]
//!   so that a caller supplying its own HTTP client
//!   ([`client::ClientBuilder::http_client`]) names the one version this crate
//!   is built against.
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
/// The HTTP crate the ready-made [`Client`] is built on, re-exported so that a
/// caller building the client's transport itself
/// ([`client::ClientBuilder::http_client`]: a proxy, a custom TLS setup) has
/// the one version this crate compiles against without a second dependency.
/// No new coupling: `reqwest::Client` and `reqwest::Error` are already in this
/// crate's public API through that method and `ClientError::Transport`.
#[cfg(feature = "client-reqwest")]
pub use reqwest;

/// The README's examples, compiled as doctests: the quick start (issue, query
/// by external id, fetch the PDF) and the failure branch need `client-reqwest`,
/// the bring-your-own-client round trip only the `ureq` dev-dependency.
#[cfg(all(doctest, feature = "client-reqwest"))]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;

pub use credentials::{AgentKey, Credentials};
pub use error::{
    ApiError, ArithmeticError, ErrorCode, OutcomeClass, ParseError, RequestError, ResponseError,
    XmlError,
};
pub use item::{LineItem, LineItemLedger, MAX_ERASURE_CODE_COUNT, Rounding};
pub use types::{
    Currency, InvoiceNumber, Language, PaymentMethod, Pdf, ReceiptNumber, TaxpayerStatus, VatRate,
};

/// Calendar date type used across the API (re-exported from [`jiff`]).
pub use jiff::civil::Date;
