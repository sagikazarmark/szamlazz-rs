//! `szamlazz invoice` — create, fetch, download, reverse.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use szamlazz_agent::ops::invoice::{CreateInvoice, CreatedInvoice, InvoiceCreationResult};
use szamlazz_agent::ops::query_pdf::{InvoiceSelector, QueryInvoicePdf};
use szamlazz_agent::ops::query_xml::QueryInvoiceXml;
use szamlazz_agent::ops::storno::StornoInvoice;

use crate::output;

/// Invoice subcommands.
#[derive(Debug, Subcommand)]
pub enum InvoiceCommand {
    /// Issue a document from a JSON description (see the repository's
    /// examples; `-f -` reads stdin).
    Create(CreateArgs),
    /// Fetch an invoice's full data.
    Get(GetArgs),
    /// Download an invoice's PDF.
    Download(DownloadArgs),
    /// Reverse (storno) an invoice.
    Storno(StornoArgs),
}

/// Arguments for `invoice create`.
#[derive(Debug, Args)]
pub struct CreateArgs {
    /// JSON file describing the `CreateInvoice` request (`-` for stdin).
    #[arg(short = 'f', long = "file")]
    file: PathBuf,
    /// Also download the PDF to this path (`-` for stdout).
    #[arg(long)]
    pdf: Option<PathBuf>,
}

/// Arguments for `invoice get`.
#[derive(Debug, Args)]
pub struct GetArgs {
    /// Invoice number; alternatively use --order or --external-id.
    number: Option<String>,
    /// Select by order number instead (returns the last invoice with it).
    #[arg(long, conflicts_with_all = ["number", "external_id"])]
    order: Option<String>,
    /// Select by the external identifier supplied when the invoice was created.
    #[arg(long, conflicts_with_all = ["number", "order"])]
    external_id: Option<String>,
}

/// Arguments for `invoice download`.
#[derive(Debug, Args)]
pub struct DownloadArgs {
    /// Invoice number; alternatively use --order or --external-id.
    number: Option<String>,
    /// Select by order number instead (returns the last invoice with it).
    #[arg(long, conflicts_with_all = ["number", "external_id"])]
    order: Option<String>,
    /// Select by the external identifier supplied when the invoice was created.
    #[arg(long, conflicts_with_all = ["number", "order"])]
    external_id: Option<String>,
    /// Where to write the PDF (`-` for stdout).
    #[arg(short, long)]
    output: PathBuf,
}

/// Arguments for `invoice storno`.
#[derive(Debug, Args)]
pub struct StornoArgs {
    /// The invoice number to reverse.
    number: String,
    /// Free-text comment (e.g. the reason).
    #[arg(long)]
    comment: Option<String>,
    /// Download the storno invoice PDF to this path (`-` for stdout).
    #[arg(long)]
    pdf: Option<PathBuf>,
}

fn selector(
    number: Option<&str>,
    order: Option<&str>,
    external_id: Option<&str>,
) -> anyhow::Result<InvoiceSelector> {
    match (number, order, external_id) {
        (Some(number), None, None) => Ok(InvoiceSelector::InvoiceNumber(number.into())),
        (None, Some(order), None) => Ok(InvoiceSelector::OrderNumber(order.to_owned())),
        (None, None, Some(id)) => Ok(InvoiceSelector::ExternalId(id.to_owned())),
        _ => anyhow::bail!("pass an invoice number, --order, or --external-id"),
    }
}

fn print_created(
    cli: &crate::Cli,
    out: &output::Report,
    created: &CreatedInvoice,
) -> anyhow::Result<()> {
    if cli.json {
        return out.json(created);
    }
    out.field_required("Invoice number", &created.invoice_number);
    out.field("Net total", created.net_total.as_ref());
    out.field("Gross total", created.gross_total.as_ref());
    out.field("Outstanding", created.outstanding.as_ref());
    out.field(
        "Customer account URL",
        created.customer_account_url.as_ref(),
    );

    Ok(())
}

fn print_creation_result(
    cli: &crate::Cli,
    out: &output::Report,
    result: &InvoiceCreationResult,
) -> anyhow::Result<()> {
    if cli.json {
        return out.json(result);
    }
    out.field("Invoice number", result.invoice_number.as_ref());
    out.field("Net total", result.net_total.as_ref());
    out.field("Gross total", result.gross_total.as_ref());
    out.field("Outstanding", result.outstanding.as_ref());
    out.field("Customer account URL", result.customer_account_url.as_ref());

    Ok(())
}

/// Runs an invoice subcommand.
pub async fn run(cli: &crate::Cli, command: &InvoiceCommand) -> anyhow::Result<()> {
    let client = crate::client(cli)?;

    match command {
        InvoiceCommand::Create(args) => {
            let mut request: CreateInvoice = output::read_json_input(&args.file)?;

            if args.pdf.is_some() {
                request.download_pdf = true;
            }
            let created = client.send(&request).await?;
            output::warn_missing_pdf(args.pdf.is_some(), created.pdf.is_some());
            let pdf_on_stdout = args.pdf.as_deref().is_some_and(output::is_stdout);

            if let (Some(target), Some(pdf)) = (&args.pdf, &created.pdf) {
                output::write_pdf(pdf.as_bytes(), target)?;
            }
            print_creation_result(cli, &output::report(pdf_on_stdout), &created)
        }
        InvoiceCommand::Get(args) => {
            let request = QueryInvoiceXml::new(selector(
                args.number.as_deref(),
                args.order.as_deref(),
                args.external_id.as_deref(),
            )?);
            let invoice = client.send(&request).await?;

            if cli.json {
                return output::json(&invoice);
            }
            output::field_required("Invoice number", &invoice.info.invoice_number);
            output::field_required("Type", &invoice.info.document_type);
            output::field("Issued", invoice.info.issue_date.as_ref());
            output::field("Fulfillment", invoice.info.fulfillment_date.as_ref());
            output::field("Due", invoice.info.due_date.as_ref());
            output::field("Payment method", invoice.info.payment_method.as_ref());
            output::field("Currency", invoice.info.currency.as_ref());
            output::field_required("Buyer", &invoice.buyer.name);
            println!();
            for item in &invoice.items {
                println!(
                    "  {} × {} {} @ {} = {} + VAT {} = {}",
                    item.quantity,
                    item.unit,
                    item.name,
                    item.unit_price,
                    item.net_value,
                    item.vat_value,
                    item.gross_value,
                );
            }
            let total = &invoice.totals.total;
            println!();
            println!(
                "  total: {} + VAT {} = {}",
                total.net, total.vat, total.gross
            );

            Ok(())
        }
        InvoiceCommand::Download(args) => {
            let request = QueryInvoicePdf::new(selector(
                args.number.as_deref(),
                args.order.as_deref(),
                args.external_id.as_deref(),
            )?);
            let fetched = client.send(&request).await?;
            output::write_pdf(fetched.pdf.as_bytes(), &args.output)?;
            if cli.json {
                let out = output::report(output::is_stdout(&args.output));
                out.json(&serde_json::json!({
                    "invoice_number": fetched.invoice_number,
                    "written_to": args.output,
                }))?;
            }

            Ok(())
        }
        InvoiceCommand::Storno(args) => {
            let mut request = StornoInvoice::new(args.number.as_str());
            request.download_pdf = args.pdf.is_some();
            request.comment = args.comment.clone();
            let created = client.send(&request).await?;
            output::warn_missing_pdf(args.pdf.is_some(), created.pdf.is_some());
            let pdf_on_stdout = args.pdf.as_deref().is_some_and(output::is_stdout);

            if let (Some(target), Some(pdf)) = (&args.pdf, &created.pdf) {
                output::write_pdf(pdf.as_bytes(), target)?;
            }

            print_created(cli, &output::report(pdf_on_stdout), &created)
        }
    }
}
