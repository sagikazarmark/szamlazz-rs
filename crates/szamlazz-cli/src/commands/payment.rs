//! `szamlazz payment` — register credit entries on an invoice.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use rust_decimal::Decimal;
use szamlazz_agent::ops::credit_entry::{CreditEntries, CreditEntry, RegisterCreditEntry};
use szamlazz_agent::types::PaymentMethod;

use crate::output;

/// Payment subcommands.
#[derive(Debug, Subcommand)]
pub enum PaymentCommand {
    /// Register one or more payments (credit entries) on an invoice.
    Register(RegisterArgs),
}

/// Arguments for `payment register`.
#[derive(Debug, Args)]
pub struct RegisterArgs {
    /// The invoice number.
    number: String,
    /// JSON file with an array of credit entries (`-` for stdin); or use
    /// --date/--method/--amount for a single entry.
    #[arg(short = 'f', long = "file", conflicts_with_all = ["date", "method", "amount", "description"])]
    file: Option<PathBuf>,
    /// Payment date (YYYY-MM-DD).
    #[arg(long, requires = "method", requires = "amount")]
    date: Option<jiff::civil::Date>,
    /// Payment method, e.g. "átutalás".
    #[arg(long)]
    method: Option<String>,
    /// Amount paid.
    #[arg(long)]
    amount: Option<Decimal>,
    /// Free-text description of the payment (single-entry form only; entries
    /// in a JSON file carry their own descriptions).
    #[arg(long)]
    description: Option<String>,
    /// Keep the invoice's existing credit entries and add these on top.
    #[arg(long)]
    additive: bool,
    /// Tax number of the invoice issuer.
    #[arg(long)]
    tax_number: Option<String>,
}

/// Runs a payment subcommand.
pub async fn run(cli: &crate::Cli, command: &PaymentCommand) -> anyhow::Result<()> {
    let PaymentCommand::Register(args) = command;
    let entries: Vec<CreditEntry> = match (&args.file, args.date) {
        (Some(file), _) => output::read_json_input(file)?,
        (None, Some(date)) => {
            let method = args.method.clone().expect("clap requires --method");
            let amount = args.amount.expect("clap requires --amount");
            let mut entry = CreditEntry::new(date, PaymentMethod::from_wire(&method), amount);
            entry.description.clone_from(&args.description);
            vec![entry]
        }
        (None, None) => anyhow::bail!("pass -f entries.json or --date/--method/--amount"),
    };

    let mut request = RegisterCreditEntry::new(args.number.as_str());
    request.issuer_tax_number = args.tax_number.clone();
    request.additive = args.additive;
    request.entries = CreditEntries::try_from(entries)?;
    let client = crate::client(cli)?;
    let result = client.send(&request).await?;

    if cli.json {
        return output::json(&result);
    }
    output::field_required("Invoice number", &result.invoice_number);
    output::field("Gross total", result.gross_total.as_ref());
    output::field("Outstanding", result.outstanding.as_ref());
    output::field("Payment method", result.payment_method.as_ref());

    Ok(())
}
