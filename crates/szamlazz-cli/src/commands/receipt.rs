//! `szamlazz receipt` — receipt (nyugta) operations.

use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use szamlazz_agent::ops::receipt::{
    CreateReceipt, QueryReceipt, ReceiptEmail, ReceiptResult, ReceiptSelector, SendReceipt,
    StornoReceipt,
};

use crate::output;

/// Receipt subcommands.
#[derive(Debug, Subcommand)]
pub enum ReceiptCommand {
    /// Issue a receipt from a JSON description (`-f -` reads stdin).
    Create(CreateArgs),
    /// Reverse (storno) a receipt.
    Storno(StornoArgs),
    /// Fetch a receipt.
    Get(GetArgs),
    /// Email a receipt to the customer.
    Send(SendArgs),
}

/// Arguments for `receipt create`.
#[derive(Debug, Args)]
pub struct CreateArgs {
    /// JSON file describing the `CreateReceipt` request (`-` for stdin).
    #[arg(short = 'f', long = "file")]
    file: PathBuf,
    /// Also download the PDF to this path (`-` for stdout).
    #[arg(long)]
    pdf: Option<PathBuf>,
}

/// Arguments for `receipt storno`.
#[derive(Debug, Args)]
pub struct StornoArgs {
    /// The receipt number to reverse.
    number: String,
    /// Download the storno receipt PDF to this path (`-` for stdout).
    #[arg(long)]
    pdf: Option<PathBuf>,
}

/// Arguments for `receipt get`.
#[derive(Debug, Args)]
pub struct GetArgs {
    /// Receipt number; alternatively use --order.
    number: Option<String>,
    /// Select by order number instead.
    #[arg(long, conflicts_with = "number")]
    order: Option<String>,
    /// Also download the PDF to this path (`-` for stdout).
    #[arg(long)]
    pdf: Option<PathBuf>,
}

/// Arguments for `receipt send`.
#[derive(Debug, Args)]
pub struct SendArgs {
    /// The receipt number to email.
    number: String,
    /// Recipient email address (omit to resend to the previous recipient).
    #[arg(long)]
    to: Option<String>,
    /// Reply-to address.
    #[arg(long)]
    reply_to: Option<String>,
    /// Email subject.
    #[arg(long)]
    subject: Option<String>,
    /// Email body.
    #[arg(long)]
    body: Option<String>,
}

fn print_result(
    cli: &crate::Cli,
    result: &ReceiptResult,
    pdf_target: Option<&Path>,
) -> anyhow::Result<()> {
    output::warn_missing_pdf(pdf_target.is_some(), result.pdf.is_some());
    let pdf_on_stdout = pdf_target.is_some_and(output::is_stdout);

    if let (Some(target), Some(pdf)) = (pdf_target, &result.pdf) {
        output::write_pdf(pdf.as_bytes(), target)?;
    }
    let out = output::report(pdf_on_stdout);

    if cli.json {
        return out.json(result);
    }
    let receipt = &result.receipt;
    out.field_required("Receipt number", &receipt.receipt_number);
    out.field_required("Type", &receipt.kind);
    out.field_required("Issued", &receipt.issue_date);
    out.field_required("Payment method", &receipt.payment_method);
    out.field_required("Currency", &receipt.currency);
    out.field_required("Cancelled", &receipt.cancelled);
    out.field("Cancels", receipt.cancelled_receipt_number.as_ref());

    Ok(())
}

/// Runs a receipt subcommand.
pub async fn run(cli: &crate::Cli, command: &ReceiptCommand) -> anyhow::Result<()> {
    let client = crate::client(cli)?;

    match command {
        ReceiptCommand::Create(args) => {
            let mut request: CreateReceipt = output::read_json_input(&args.file)?;

            if args.pdf.is_some() {
                request.download_pdf = true;
            }
            let result = client.send(&request).await?;

            print_result(cli, &result, args.pdf.as_deref())
        }
        ReceiptCommand::Storno(args) => {
            let mut request = StornoReceipt::new(args.number.as_str());
            request.download_pdf = args.pdf.is_some();
            let result = client.send(&request).await?;

            print_result(cli, &result, args.pdf.as_deref())
        }
        ReceiptCommand::Get(args) => {
            let selector = match (&args.number, &args.order) {
                (Some(number), None) => ReceiptSelector::ReceiptNumber(number.as_str().into()),
                (None, Some(order)) => ReceiptSelector::OrderNumber(order.clone()),
                _ => anyhow::bail!("pass a receipt number or --order"),
            };
            let mut request = QueryReceipt::new(selector);
            request.download_pdf = args.pdf.is_some();
            let result = client.send(&request).await?;

            print_result(cli, &result, args.pdf.as_deref())
        }
        ReceiptCommand::Send(args) => {
            let email = (args.to.is_some()
                || args.reply_to.is_some()
                || args.subject.is_some()
                || args.body.is_some())
            .then(|| {
                let mut email = ReceiptEmail::default();
                email.to.clone_from(&args.to);
                email.reply_to.clone_from(&args.reply_to);
                email.subject.clone_from(&args.subject);
                email.body.clone_from(&args.body);
                email
            });
            let mut request = SendReceipt::new(args.number.as_str());
            request.email = email;
            client.send(&request).await?;
            if cli.json {
                output::json(&serde_json::json!({ "sent": true }))?;
            } else {
                println!("Receipt emailed.");
            }

            Ok(())
        }
    }
}
