//! `szamlazz invoice`: create, fetch, download, reverse.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use szamlazz_agent::InvoiceSelector;
use szamlazz_agent::ops::invoice::{CreateInvoice, CreatedInvoice, CreationOutcome};
use szamlazz_agent::ops::query_pdf::QueryInvoicePdf;
use szamlazz_agent::ops::query_xml::{InvoiceAppearance, QueryInvoiceXml};
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

fn print_created(out: &output::Report, created: &CreatedInvoice) {
    out.field_required("Invoice number", &created.invoice_number);
    out.field("Net total", created.net_total.as_ref());
    out.field("Gross total", created.gross_total.as_ref());
    out.field("Outstanding", created.outstanding.as_ref());
    out.field(
        "Customer account URL",
        created.customer_account_url.as_ref(),
    );
}

/// The create's outcome: the issued document, or the preview that issued
/// nothing (`header.preview_pdf` in the JSON description).
fn print_creation_outcome(out: &output::Report, outcome: &CreationOutcome) {
    match outcome {
        CreationOutcome::Issued(created) => {
            out.field_required("Outcome", &"issued");
            print_created(out, created);
        }
        CreationOutcome::Preview(_) => {
            out.field_required("Preview", &"rendered; no document was issued");
        }
        _ => {
            out.field_required("Outcome", &"no document number in the reply");
        }
    }
}

/// A successful exchange alone does not establish that a storno reversed its
/// original. Keep the returned document even when the verdict is inconclusive.
fn print_storno(
    cli: &crate::Cli,
    args: &StornoArgs,
    original: &szamlazz_agent::InvoiceNumber,
    created: &CreatedInvoice,
) -> anyhow::Result<()> {
    let (outcome, explanation) = if created.reverses(original) {
        ("reversed", "reversal confirmed (may be an existing storno)")
    } else if created.invoice_number == *original {
        ("noop", "same invoice number returned; nothing was reversed")
    } else if created.gross_total.is_none() {
        (
            "unconfirmed",
            "gross total missing; reversal is not confirmed",
        )
    } else {
        (
            "unconfirmed",
            "positive gross total; reversal is not confirmed",
        )
    };
    let remote = serde_json::json!({
        "outcome": outcome,
        "message": explanation,
        "original_number": original,
        "document": created,
    });
    output::document(
        cli.json,
        &remote,
        created.pdf.as_ref(),
        args.pdf.as_deref(),
        |out| {
            out.field_required("Outcome", &outcome);
            out.field_required("Storno", &explanation);
            out.field_required("Original number", original);
            print_created(out, created);
        },
    )?;
    anyhow::ensure!(
        outcome == "reversed",
        "{explanation}; inspect the returned document before retrying"
    );
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
            let outcome = client.send(&request).await?;
            output::document(
                cli.json,
                &outcome,
                outcome.pdf(),
                args.pdf.as_deref(),
                |out| {
                    print_creation_outcome(out, &outcome);
                },
            )
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
            let original = client
                .send(&QueryInvoiceXml::new(InvoiceSelector::InvoiceNumber(
                    args.number.as_str().into(),
                )))
                .await?;
            let request = storno_request(args, &original.info)?;
            let created = client.send(&request).await?;
            print_storno(cli, args, &request.invoice_number, &created)
        }
    }
}

fn storno_request(
    args: &StornoArgs,
    original: &szamlazz_agent::ops::query_xml::InvoiceInfo,
) -> anyhow::Result<StornoInvoice> {
    anyhow::ensure!(
        original.invoice_number.as_str() == args.number,
        "queried original number does not match; no storno sent"
    );
    let e_invoice = match original.appearance {
        InvoiceAppearance::Paper => false,
        InvoiceAppearance::Electronic(_) => true,
        appearance => anyhow::bail!(
            "cannot derive storno appearance from {appearance:?}; inspect the original in szamlazz.hu; no storno sent"
        ),
    };
    let fulfillment_date = original
        .fulfillment_date
        .ok_or_else(|| anyhow::anyhow!("original has no fulfillment date; no storno sent"))?;
    Ok(StornoInvoice {
        e_invoice,
        fulfillment_date: Some(fulfillment_date),
        download_pdf: args.pdf.is_some(),
        comment: args.comment.clone(),
        ..StornoInvoice::new(args.number.as_str())
    })
}
