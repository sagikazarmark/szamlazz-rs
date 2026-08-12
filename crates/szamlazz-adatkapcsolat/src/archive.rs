//! [`Archiver`]: a ready-made [`Handler`] that persists every pushed document
//! through an [OpenDAL](https://opendal.apache.org/) operator (feature
//! `opendal`).
//!
//! Three artifacts can be written per document: the exact pushed **XML**
//! (`.xml`), the embedded invoice **PDF** (`.pdf`), and typed document data as
//! JSON (`.json`, with the embedded PDF stripped). Each is toggleable.
//! Storage failures surface as handler errors → HTTP 500 → szamlazz.hu
//! retries the delivery for up to 72 hours, so durability rides the protocol.
//!
//! ```no_run
//! use opendal::{Operator, services};
//! use szamlazz_adatkapcsolat::archive::{Archiver, Redelivery};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let operator = Operator::new(services::Memory::default())?;
//! let handler = Archiver::builder(operator)
//!     .redelivery(Redelivery::Both)
//!     .build();
//! # let _ = handler;
//! # Ok(())
//! # }
//! ```
//!
//! In production, replace the memory service with a configured durable
//! `OpenDAL` service; that service's root controls the archive path prefix.
//!
//! To combine archiving with your own logic, run the archiver alongside your
//! handler with a [`Fanout`](crate::Fanout).

use jiff::civil::Date;
use opendal::Operator;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::ack::{Ack, InvoiceAck, InvoiceDirection};
use crate::document::{BankTransaction, InvoiceDocument, ReceiptBatch, ReceiptDocument};
use crate::handler::Handler;

/// Which document stream an archived object came from; selects the directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocumentKind {
    OutgoingInvoice,
    IncomingInvoice,
    BankTransaction,
    Receipt,
}

impl DocumentKind {
    fn directory(self) -> &'static str {
        match self {
            Self::OutgoingInvoice => "outgoing-invoices",
            Self::IncomingInvoice => "incoming-invoices",
            Self::BankTransaction => "bank-transactions",
            Self::Receipt => "receipts",
        }
    }
}

/// How objects are laid out within a document-type directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Layout {
    /// `{type}/{YYYY}/{MM}/{name}` — the month comes from the document's own
    /// date (so re-deliveries land in the same place); undated documents go
    /// to `{type}/undated/{name}`.
    #[default]
    Monthly,
    /// `{type}/{name}`.
    Flat,
}

/// What happens when the same document is delivered again (retries, or an
/// outgoing invoice re-pushed because it changed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Redelivery {
    /// Write `name.ext`; a re-delivery overwrites it — latest state wins.
    #[default]
    Overwrite,
    /// Write `name.{timestamp}.ext` only; every delivery is kept.
    Timestamped,
    /// Write both: `name.ext` always holds the latest state, and each
    /// delivery is also kept as `name.{timestamp}.ext`.
    Both,
}

/// A storage or serialization failure while archiving.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ArchiveError {
    /// The `OpenDAL` write failed.
    #[error("storage error: {0}")]
    Storage(#[from] opendal::Error),
    /// The document could not be rendered as JSON.
    #[error("serialization error: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// A [`Handler`] that archives every pushed document through an `OpenDAL`
/// operator and acknowledges it.
///
/// [`Archiver::new`] applies the defaults: monthly layout, overwrite on
/// re-delivery, source XML, PDFs, and typed JSON all saved. Anything else goes through
/// [`Archiver::builder`]. Objects land relative to the operator's root — set
/// `root` on the `OpenDAL` service to put everything under a prefix.
#[derive(Debug)]
pub struct Archiver {
    op: Operator,
    layout: Layout,
    redelivery: Redelivery,
    save_xml: bool,
    save_pdf: bool,
    save_data: bool,
}

/// Configures and builds an [`Archiver`]; created by [`Archiver::builder`].
#[derive(Debug)]
#[must_use]
pub struct ArchiverBuilder {
    archiver: Archiver,
}

impl ArchiverBuilder {
    /// Selects the directory layout.
    pub fn layout(mut self, layout: Layout) -> Self {
        self.archiver.layout = layout;
        self
    }

    /// Selects the re-delivery behavior.
    ///
    /// The timestamped variants read the wall clock (`jiff::Timestamp::now`),
    /// which on `wasm32-unknown-unknown` is only available with jiff's `js`
    /// feature enabled by your application. They also require the `OpenDAL`
    /// service to support conditional `if_not_exists` writes.
    pub fn redelivery(mut self, redelivery: Redelivery) -> Self {
        self.archiver.redelivery = redelivery;
        self
    }

    /// Whether the exact pushed XML is written (default `true`).
    pub fn save_xml(mut self, save: bool) -> Self {
        self.archiver.save_xml = save;
        self
    }

    /// Whether embedded invoice PDFs are written (default `true`).
    pub fn save_pdf(mut self, save: bool) -> Self {
        self.archiver.save_pdf = save;
        self
    }

    /// Whether document data is written as JSON (default `true`). The
    /// embedded PDF is always stripped from the JSON — it is controlled by
    /// [`ArchiverBuilder::save_pdf`] alone. The exact source XML is controlled
    /// separately by [`ArchiverBuilder::save_xml`].
    pub fn save_data(mut self, save: bool) -> Self {
        self.archiver.save_data = save;
        self
    }

    /// The configured archiver.
    #[must_use]
    pub fn build(self) -> Archiver {
        self.archiver
    }
}

impl Archiver {
    /// An archiver with default settings writing through `op`.
    #[must_use]
    pub fn new(op: Operator) -> Self {
        Self {
            op,
            layout: Layout::default(),
            redelivery: Redelivery::default(),
            save_xml: true,
            save_pdf: true,
            save_data: true,
        }
    }

    /// A builder for an archiver writing through `op`.
    pub fn builder(op: Operator) -> ArchiverBuilder {
        ArchiverBuilder {
            archiver: Self::new(op),
        }
    }

    /// Archives a pushed invoice: its PDF (when present and enabled) and its
    /// data (when enabled).
    async fn archive_invoice(
        &self,
        direction: InvoiceDirection,
        invoice: &InvoiceDocument,
    ) -> Result<(), ArchiveError> {
        let kind = match direction {
            InvoiceDirection::Outgoing => DocumentKind::OutgoingInvoice,
            InvoiceDirection::Incoming => DocumentKind::IncomingInvoice,
        };
        let name = invoice.info.id.to_string();
        let date = invoice.info.issue_date;

        if self.save_xml
            && let Some(xml) = invoice.raw_xml()
        {
            self.write(kind, &name, date, "xml", xml.as_bytes().to_vec())
                .await?;
        }
        if self.save_pdf
            && let Some(pdf) = &invoice.pdf
        {
            self.write(kind, &name, date, "pdf", pdf.as_bytes().to_vec())
                .await?;
        }
        if self.save_data {
            let json = document_json(invoice)?;
            self.write(kind, &name, date, "json", json).await?;
        }

        Ok(())
    }

    /// Archives a pushed bank transaction as data.
    async fn archive_bank_transaction(
        &self,
        transaction: &BankTransaction,
    ) -> Result<(), ArchiveError> {
        let kind = DocumentKind::BankTransaction;
        let name = transaction.id.to_string();
        let date = Some(transaction.value_date);

        if self.save_xml
            && let Some(xml) = transaction.raw_xml()
        {
            self.write(kind, &name, date, "xml", xml.as_bytes().to_vec())
                .await?;
        }
        if self.save_data {
            let json = serde_json::to_vec_pretty(transaction)?;
            self.write(kind, &name, date, "json", json).await?;
        }

        Ok(())
    }

    /// Archives one receipt of a pushed batch as data.
    async fn archive_receipt(&self, receipt: &ReceiptDocument) -> Result<(), ArchiveError> {
        if !self.save_data {
            return Ok(());
        }
        let name = match &receipt.info.receipt_number {
            Some(number) if !number.trim().is_empty() => sanitize(number),
            _ => receipt.info.id.to_string(),
        };
        let json = serde_json::to_vec_pretty(receipt)?;

        self.write(
            DocumentKind::Receipt,
            &name,
            receipt.info.issue_date,
            "json",
            json,
        )
        .await
    }

    /// Archives the exact receipt-batch delivery once. Individual receipts
    /// still get their own typed JSON files.
    async fn archive_receipt_batch(&self, batch: &ReceiptBatch) -> Result<(), ArchiveError> {
        if !self.save_xml {
            return Ok(());
        }
        let Some(xml) = batch.raw_xml() else {
            return Ok(());
        };
        let first_id = batch
            .receipts
            .iter()
            .map(|receipt| receipt.info.id)
            .min()
            .expect("validated receipt batches are non-empty");
        let last_id = batch
            .receipts
            .iter()
            .map(|receipt| receipt.info.id)
            .max()
            .expect("validated receipt batches are non-empty");
        let date = batch
            .receipts
            .iter()
            .find_map(|receipt| receipt.info.issue_date);

        self.write(
            DocumentKind::Receipt,
            &format!("batch-{first_id}-{last_id}"),
            date,
            "xml",
            xml.as_bytes().to_vec(),
        )
        .await
    }

    async fn write(
        &self,
        kind: DocumentKind,
        name: &str,
        date: Option<Date>,
        extension: &str,
        bytes: Vec<u8>,
    ) -> Result<(), ArchiveError> {
        let base = self.path(kind, name, date);

        match self.redelivery {
            Redelivery::Overwrite => {
                self.op.write(&format!("{base}.{extension}"), bytes).await?;
            }
            Redelivery::Timestamped => {
                self.write_version(&base, extension, bytes.into()).await?;
            }
            Redelivery::Both => {
                let bytes = opendal::Buffer::from(bytes);
                self.write_version(&base, extension, bytes.clone()).await?;
                self.op.write(&format!("{base}.{extension}"), bytes).await?;
            }
        }

        Ok(())
    }

    async fn write_version(
        &self,
        base: &str,
        extension: &str,
        bytes: opendal::Buffer,
    ) -> Result<(), opendal::Error> {
        self.write_version_with(base, extension, bytes, timestamp)
            .await
    }

    async fn write_version_with(
        &self,
        base: &str,
        extension: &str,
        bytes: opendal::Buffer,
        mut next_version: impl FnMut() -> String,
    ) -> Result<(), opendal::Error> {
        loop {
            let path = format!("{base}.{}.{extension}", next_version());

            match self
                .op
                .write_with(&path, bytes.clone())
                .if_not_exists(true)
                .await
            {
                Ok(_) => return Ok(()),
                Err(error)
                    if matches!(
                        error.kind(),
                        opendal::ErrorKind::AlreadyExists | opendal::ErrorKind::ConditionNotMatch
                    ) => {}
                Err(error) => return Err(error),
            }
        }
    }

    fn path(&self, kind: DocumentKind, name: &str, date: Option<Date>) -> String {
        let mut path = String::new();
        path.push_str(kind.directory());
        path.push('/');

        if self.layout == Layout::Monthly {
            match date {
                Some(date) => {
                    write!(path, "{:04}/{:02}/", date.year(), date.month())
                        .expect("writing to a String cannot fail");
                }
                None => path.push_str("undated/"),
            }
        }
        path.push_str(name);

        path
    }
}

/// The document as pretty JSON with the embedded PDF stripped: the PDF is
/// archived as a file, not as base64 inside the data.
fn document_json(invoice: &InvoiceDocument) -> Result<Vec<u8>, serde_json::Error> {
    let mut value = serde_json::to_value(invoice)?;

    if let Some(object) = value.as_object_mut() {
        object.remove("pdf");
    }

    serde_json::to_vec_pretty(&value)
}

/// A sortable UTC stamp plus nanosecond time and a process-local sequence.
fn timestamp() -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    let now = jiff::Timestamp::now();
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);

    format!(
        "{}-{}-{sequence:016x}",
        now.strftime("%Y%m%dT%H%M%SZ"),
        now.as_nanosecond()
    )
}

/// Makes a business identifier safe as a single path segment.
fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|c| if c == '/' || c == '\\' { '-' } else { c })
        .filter(|c| !c.is_control())
        .collect();
    let trimmed = cleaned.trim_matches('.').trim();

    if trimmed.is_empty() {
        "unnamed".to_owned()
    } else {
        trimmed.to_owned()
    }
}

impl Handler for Archiver {
    type Error = ArchiveError;

    async fn outgoing_invoice(&self, invoice: InvoiceDocument) -> Result<InvoiceAck, Self::Error> {
        self.archive_invoice(InvoiceDirection::Outgoing, &invoice)
            .await?;

        Ok(InvoiceAck::accept(invoice.info.id))
    }

    async fn incoming_invoice(&self, invoice: InvoiceDocument) -> Result<InvoiceAck, Self::Error> {
        self.archive_invoice(InvoiceDirection::Incoming, &invoice)
            .await?;

        Ok(InvoiceAck::accept(invoice.info.id))
    }

    async fn bank_transaction(&self, transaction: BankTransaction) -> Result<Ack, Self::Error> {
        self.archive_bank_transaction(&transaction).await?;

        Ok(Ack::accept())
    }

    async fn receipts(&self, batch: ReceiptBatch) -> Result<Ack, Self::Error> {
        self.archive_receipt_batch(&batch).await?;

        for receipt in &batch.receipts {
            self.archive_receipt(receipt).await?;
        }

        Ok(Ack::accept())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn timestamped_write_retries_without_replacing_an_existing_version() {
        let op = Operator::new(opendal::services::Memory::default()).expect("memory operator");
        op.write("invoice.duplicate.xml", "existing")
            .await
            .expect("existing version");
        let archiver = Archiver::new(op.clone());
        let mut versions = ["duplicate", "unique"].into_iter();

        archiver
            .write_version_with("invoice", "xml", opendal::Buffer::from("new"), || {
                versions.next().expect("version available").to_owned()
            })
            .await
            .expect("write unique version");

        assert_eq!(
            op.read("invoice.duplicate.xml")
                .await
                .expect("read existing")
                .to_vec(),
            b"existing"
        );
        assert_eq!(
            op.read("invoice.unique.xml")
                .await
                .expect("read unique")
                .to_vec(),
            b"new"
        );
    }
}
