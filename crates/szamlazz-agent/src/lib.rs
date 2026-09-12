//! Client for the [szamlazz.hu Számla Agent](https://docs.szamlazz.hu/) XML API.
//!
//! The core of this crate performs no I/O: request types serialize into a
//! ready-to-send [`WireRequest`](wire::WireRequest) and responses are parsed
//! from raw headers and body bytes, so any HTTP client on any platform
//! (including `wasm32-unknown-unknown` and Cloudflare Workers) can drive it.
//! Enable the `client-reqwest` feature for a ready-made async client.
//!
//! Identifiers are English; a type or field documents its Hungarian wire
//! name, and the wire-facing ones carry it as a doc alias too, so rustdoc
//! search finds `szállító`, `qutet` or `vevoifiokurl`.
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
//! Every wire code set is open: a token the crate does not know is kept in
//! an `Other(String)`, a numeric code in an `Unknown(n)` (see [`types`]).
//!
//! # Monetary Serde input
//!
//! Public decimal fields accept decimal strings and JSON number tokens, including
//! exponents, through [`parse_decimal`]: values must fit exactly, without rounding.
//! Optional fields also accept null or omission. Serialization is pinned to exact
//! strings, independently of downstream `rust_decimal` Serde features. Decode JSON directly from text, bytes or a reader
//! to preserve both numeric precision and the distinction between numbers and objects.
//!
//! Serde's buffered adapters (such as untagged or internally tagged enums and
//! flattened fields) erase that distinction for arbitrary-precision numbers.
//! Such ambiguous maps are refused; use decimal strings or integers within the
//! JSON deserializer's i64/u64 range in those wrappers. A prebuilt JSON `Value`
//! may already have interpreted a private-number
//! lookalike object as a number; decoding cannot reconstruct its original shape.
//! Other self-describing formats accept strings and integers, and interpret floats
//! by their shortest decimal spelling; precision lost before decoding is unrecoverable.
//! Non-human-readable formats use string encoding too. Raw-token dispatch is
//! restricted to `serde_json`'s concrete text/bytes/reader and `Value` deserializers;
//! monetary scalars also support `serde_ignored` 0.1 directly wrapping a JSON
//! parser, whose forwarding preserves raw tokens and unknown-field callbacks.
//! Other deserializer wrappers take the conservative scalar path above.
//! In particular, Axum 0.8's `Json<T>` uses `serde_path_to_error`: send money as
//! strings (for example, `"amount": "12.34"`) through that extractor. A
//! `serde_ignored`-wrapped [`ops::storno::StornoResponse`] also needs strings for
//! order-independent decoding: content before the adjacent tag is buffered.
//! The scalar exception does not bypass whole-envelope unknown-field callbacks.
//!
//! # Features
//!
//! Default features are empty and provide the I/O-free request/response core.
//! The core works on native targets and `wasm32-unknown-unknown`; filesystem
//! helpers such as `Pdf::save_to` are available only on non-wasm targets.
//!
//! - `client-reqwest` adds the ready-made async `Client`. It supports native
//!   and browser wasm transport compilation. On native targets it manages the session cookie,
//!   timeout (`client::REQUEST_TIMEOUT`), TLS, and redirect policy; on wasm the
//!   browser controls cookies and redirects. The feature re-exports [`reqwest`]
//!   so that a caller supplying its own HTTP client
//!   ([`client::ClientBuilder::http_client`]) names the one version this crate
//!   is built against.
//!
//! Browser transport availability does not establish direct access to the
//! Számla Agent endpoint: vendor CORS policy must permit the request and expose
//! the `szlahu_*` response headers. Browsers control cookies and do not expose
//! `Set-Cookie` to application code. Reqwest's Fetch requests default to
//! same-origin credentials unless overridden per request; `Client::send` keeps
//! that default, and injecting another reqwest client does not enable inclusion.
//! XML authentication may work without cookies, but direct browser feasibility
//! remains a vendor/platform question, not something native loopback tests prove.
//! The [vendor authentication guidance](https://docs.szamlazz.hu/agent/basics/authentication)
//! explicitly forbids including agent keys in client-side code. Keep account
//! keys on a trusted server (including server-side wasm); browser applications
//! call that server rather than receiving the key.
// docs.rs builds with all features on nightly and sets `--cfg docsrs`;
// current rustdoc's doc_cfg automatically annotates feature- and target gates.
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "client-reqwest")]
pub mod client;
pub mod credentials;
pub mod error;
pub mod item;
mod number;
pub use number::parse as parse_decimal;
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
/// crate's public API through that method, `ClientError::Transport` and
/// [`client::IncompleteResponse`].
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
    Currency, DocumentType, ExchangeRate, GrandTotal, InvoiceNumber, InvoiceSelector,
    InvoiceTemplate, Language, PaymentMethod, Pdf, ReceiptNumber, ReceiptType, SellerEmail,
    TaxpayerStatus, Totals, VatRate, VatTotal,
};

/// Calendar date type used across the API (re-exported from [`jiff`]).
pub use jiff::civil::Date;
