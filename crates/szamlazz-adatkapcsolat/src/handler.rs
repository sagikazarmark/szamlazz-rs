//! The [`Handler`] trait: your business logic behind the push protocol.

use std::future::Future;

use crate::ack::{Ack, InvoiceAck};
use crate::document::{BankTransaction, InvoiceDocument, ReceiptBatch};

/// `Send` on native targets, nothing on `wasm32` — JavaScript futures are
/// `!Send`, so requiring `Send` would make the trait unimplementable on
/// Cloudflare Workers.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSend: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send> MaybeSend for T {}

/// `Send` on native targets, nothing on `wasm32` — JavaScript futures are
/// `!Send`, so requiring `Send` would make the trait unimplementable on
/// Cloudflare Workers.
#[cfg(target_arch = "wasm32")]
pub trait MaybeSend {}
#[cfg(target_arch = "wasm32")]
impl<T> MaybeSend for T {}

/// `Sync` on native targets, nothing on `wasm32` — the counterpart of
/// [`MaybeSend`] for shared handler state holding JS objects.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSync: Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Sync> MaybeSync for T {}

/// `Sync` on native targets, nothing on `wasm32` — the counterpart of
/// [`MaybeSend`] for shared handler state holding JS objects.
#[cfg(target_arch = "wasm32")]
pub trait MaybeSync {}
#[cfg(target_arch = "wasm32")]
impl<T> MaybeSync for T {}

/// Handles pushed documents. Every method is required so adding a stream to a
/// connection cannot silently acknowledge and discard its documents.
///
/// The contract per method:
/// - `Ok(ack)` → HTTP 200 with the proper response XML; the document is
///   considered delivered. Return it only once the document is durably
///   accepted.
/// - `Err(_)` → HTTP 500; szamlazz.hu retries the delivery for up to 72
///   hours. Use this for transient failures (database down, …).
/// - `KEY_ERR` / `KEY_DEL` are protocol speech, not errors: return them via
///   the ack constructors ([`InvoiceAck::key_error`], [`Ack::disconnect`],
///   …). Key *verification* normally happens in the integration layer (see
///   `axum::router` when the `axum` feature is enabled) before your handler
///   runs.
///
/// Methods can be written as `async fn` in implementations; the `MaybeSend`
/// bound keeps the trait implementable on Cloudflare Workers, where futures
/// are `!Send`.
pub trait Handler {
    /// The transient-failure type. It is *not* sent to szamlazz.hu — a handler
    /// failure answers a bare HTTP 500 (the status alone drives the 72-hour
    /// retry), so the error may carry internal detail. Log it yourself for
    /// diagnostics; the `Display` bound is what the integration layer uses to
    /// do so.
    type Error: std::fmt::Display;

    /// An outgoing invoice (`<szamla>`) was pushed.
    fn outgoing_invoice(
        &self,
        invoice: InvoiceDocument,
    ) -> impl Future<Output = Result<InvoiceAck, Self::Error>> + MaybeSend;

    /// An incoming invoice (`<szamlabe>`) was pushed.
    fn incoming_invoice(
        &self,
        invoice: InvoiceDocument,
    ) -> impl Future<Output = Result<InvoiceAck, Self::Error>> + MaybeSend;

    /// A bank transaction (`<banktranz>`) was pushed.
    fn bank_transaction(
        &self,
        transaction: BankTransaction,
    ) -> impl Future<Output = Result<Ack, Self::Error>> + MaybeSend;

    /// A receipt batch (`<xmlnyugtaarchiv>`) was pushed.
    fn receipts(
        &self,
        batch: ReceiptBatch,
    ) -> impl Future<Output = Result<Ack, Self::Error>> + MaybeSend;
}
