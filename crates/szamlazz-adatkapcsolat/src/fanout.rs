//! [`Fanout`]: a [`Handler`] that delivers every document to several
//! underlying handlers.
//!
//! [`Handler`]'s async methods make the trait not dyn-compatible, so a
//! heterogeneous handler list cannot be built with `Vec<Box<dyn Handler>>`;
//! [`Fanout::with`] does the required type erasure internally:
//!
//! ```no_run
//! use szamlazz_adatkapcsolat::{Fanout, Handler, MaybeSend, MaybeSync};
//!
//! # fn combine<A, B>(archive: A, business_logic: B) -> Fanout
//! # where
//! #     A: Handler + MaybeSend + MaybeSync + 'static,
//! #     B: Handler + MaybeSend + MaybeSync + 'static,
//! # {
//! let handler = Fanout::new()
//!     .with(archive)
//!     .with(business_logic);
//! # handler
//! # }
//! ```
//!
//! Semantics:
//! - Handlers run **sequentially, in registration order**, and all of them
//!   run even when an earlier one fails — each delivery makes as much
//!   progress as possible.
//! - If any handler failed, the fan-out fails with a per-handler report →
//!   HTTP 500 → szamlazz.hu re-delivers **to every handler**. Members must
//!   therefore tolerate re-delivery — which the push protocol demands of any
//!   receiver anyway (a lost acknowledgement causes re-delivery too).
//! - Acks are merged: the strongest control code wins (`KEY_DEL` over
//!   `KEY_ERR` over accept); otherwise the document is accepted with the
//!   first registration number any handler supplied.

use std::future::Future;
use std::pin::Pin;

use crate::ack::{Ack, ControlCode, InvoiceAck};
use crate::document::{BankTransaction, InvoiceDocument, ReceiptBatch};
use crate::handler::{Handler, MaybeSend, MaybeSync};

#[cfg(not(target_arch = "wasm32"))]
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
#[cfg(target_arch = "wasm32")]
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

#[cfg(not(target_arch = "wasm32"))]
type BoxedHandler = Box<dyn ErasedHandler + Send + Sync>;
#[cfg(target_arch = "wasm32")]
type BoxedHandler = Box<dyn ErasedHandler>;

/// Dyn-compatible mirror of [`Handler`] with the error stringified.
trait ErasedHandler {
    fn outgoing_invoice(
        &self,
        invoice: InvoiceDocument,
    ) -> BoxFuture<'_, Result<InvoiceAck, String>>;
    fn incoming_invoice(
        &self,
        invoice: InvoiceDocument,
    ) -> BoxFuture<'_, Result<InvoiceAck, String>>;
    fn bank_transaction(&self, transaction: BankTransaction) -> BoxFuture<'_, Result<Ack, String>>;
    fn receipts(&self, batch: ReceiptBatch) -> BoxFuture<'_, Result<Ack, String>>;
}

impl<H> ErasedHandler for H
where
    H: Handler + MaybeSend + MaybeSync,
{
    fn outgoing_invoice(
        &self,
        invoice: InvoiceDocument,
    ) -> BoxFuture<'_, Result<InvoiceAck, String>> {
        Box::pin(async move {
            Handler::outgoing_invoice(self, invoice)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn incoming_invoice(
        &self,
        invoice: InvoiceDocument,
    ) -> BoxFuture<'_, Result<InvoiceAck, String>> {
        Box::pin(async move {
            Handler::incoming_invoice(self, invoice)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn bank_transaction(&self, transaction: BankTransaction) -> BoxFuture<'_, Result<Ack, String>> {
        Box::pin(async move {
            Handler::bank_transaction(self, transaction)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn receipts(&self, batch: ReceiptBatch) -> BoxFuture<'_, Result<Ack, String>> {
        Box::pin(async move {
            Handler::receipts(self, batch)
                .await
                .map_err(|error| error.to_string())
        })
    }
}

/// One member's failure inside a fan-out delivery.
#[derive(Debug)]
#[non_exhaustive]
pub struct HandlerFailure {
    /// The failing handler's type name.
    pub handler: &'static str,
    /// Its error, stringified.
    pub error: String,
}

/// One or more fan-out members failed; the delivery will be retried for all.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub struct FanoutError {
    /// The individual failures, in registration order.
    pub failures: Vec<HandlerFailure>,
}

impl std::fmt::Display for FanoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} fan-out handler(s) failed: ", self.failures.len())?;

        for (index, failure) in self.failures.iter().enumerate() {
            if index > 0 {
                write!(f, "; ")?;
            }
            write!(f, "{}: {}", failure.handler, failure.error)?;
        }

        Ok(())
    }
}

/// A [`Handler`] delivering every document to all registered handlers.
///
/// See the module docs for ordering, failure, and ack-merging semantics. An
/// empty fan-out rejects delivery so documents are not silently discarded.
#[derive(Default)]
#[must_use]
pub struct Fanout {
    handlers: Vec<(&'static str, BoxedHandler)>,
}

impl std::fmt::Debug for Fanout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.handlers.iter().map(|(name, _)| name))
            .finish()
    }
}

impl Fanout {
    /// An empty fan-out.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a handler; boxing and type erasure happen here.
    pub fn with<H>(mut self, handler: H) -> Self
    where
        H: Handler + MaybeSend + MaybeSync + 'static,
    {
        self.handlers
            .push((std::any::type_name::<H>(), Box::new(handler)));
        self
    }

    fn require_handler(&self) -> Result<(), FanoutError> {
        if self.handlers.is_empty() {
            Err(FanoutError {
                failures: vec![HandlerFailure {
                    handler: "Fanout",
                    error: "no handlers configured".to_owned(),
                }],
            })
        } else {
            Ok(())
        }
    }
}

/// Runs every handler's `$method` sequentially, giving the last one the
/// original document and everyone else a clone; collects failures.
macro_rules! dispatch {
    ($fanout:expr, $document:expr, $method:ident) => {{
        let mut acks = Vec::with_capacity($fanout.handlers.len());
        let mut failures = Vec::new();
        let count = $fanout.handlers.len();
        let mut document = Some($document);

        for (index, (name, handler)) in $fanout.handlers.iter().enumerate() {
            // Clone for all but the last handler, which gets the original.
            let doc = if index + 1 == count {
                document
                    .take()
                    .expect("document is present until the last handler")
            } else {
                document
                    .as_ref()
                    .expect("document is present until the last handler")
                    .clone()
            };

            match handler.$method(doc).await {
                Ok(ack) => acks.push(ack),
                Err(error) => failures.push(HandlerFailure {
                    handler: name,
                    error,
                }),
            }
        }

        if failures.is_empty() {
            Ok(acks)
        } else {
            Err(FanoutError { failures })
        }
    }};
}

/// Strongest control code across acks: `KEY_DEL` > `KEY_ERR` > none.
fn escalate(current: Option<ControlCode>, next: Option<ControlCode>) -> Option<ControlCode> {
    match (current, next) {
        (Some(ControlCode::Disconnect), _) | (_, Some(ControlCode::Disconnect)) => {
            Some(ControlCode::Disconnect)
        }
        (Some(ControlCode::KeyError), _) | (_, Some(ControlCode::KeyError)) => {
            Some(ControlCode::KeyError)
        }
        (None, None) => None,
    }
}

fn merge_invoice_acks(document_id: i32, acks: &[InvoiceAck]) -> InvoiceAck {
    let mut control = None;
    let mut registration: Option<String> = None;

    for ack in acks {
        let (_, reg, code) = ack.parts();
        control = escalate(control, code);
        if registration.is_none() {
            registration = reg.map(str::to_owned);
        }
    }

    match control {
        Some(ControlCode::Disconnect) => InvoiceAck::disconnect(),
        Some(ControlCode::KeyError) => InvoiceAck::key_error(),
        None => {
            let ack = InvoiceAck::accept(document_id);

            match registration {
                Some(registration) => ack.with_registration_number(registration),
                None => ack,
            }
        }
    }
}

fn merge_acks(acks: &[Ack]) -> Ack {
    let control = acks
        .iter()
        .fold(None, |current, ack| escalate(current, ack.control_code()));

    match control {
        Some(ControlCode::Disconnect) => Ack::disconnect(),
        Some(ControlCode::KeyError) => Ack::key_error(),
        None => Ack::accept(),
    }
}

impl Handler for Fanout {
    type Error = FanoutError;

    async fn outgoing_invoice(&self, invoice: InvoiceDocument) -> Result<InvoiceAck, Self::Error> {
        self.require_handler()?;
        let id = invoice.info.id;
        let acks = dispatch!(self, invoice, outgoing_invoice)?;

        Ok(merge_invoice_acks(id, &acks))
    }

    async fn incoming_invoice(&self, invoice: InvoiceDocument) -> Result<InvoiceAck, Self::Error> {
        self.require_handler()?;
        let id = invoice.info.id;
        let acks = dispatch!(self, invoice, incoming_invoice)?;

        Ok(merge_invoice_acks(id, &acks))
    }

    async fn bank_transaction(&self, transaction: BankTransaction) -> Result<Ack, Self::Error> {
        self.require_handler()?;
        let acks = dispatch!(self, transaction, bank_transaction)?;

        Ok(merge_acks(&acks))
    }

    async fn receipts(&self, batch: ReceiptBatch) -> Result<Ack, Self::Error> {
        self.require_handler()?;
        let acks = dispatch!(self, batch, receipts)?;

        Ok(merge_acks(&acks))
    }
}
