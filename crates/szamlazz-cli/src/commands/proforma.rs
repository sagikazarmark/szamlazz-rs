//! `szamlazz proforma` — proforma (díjbekérő) management.

use clap::{Args, Subcommand};
use szamlazz_agent::ops::proforma::{DeleteProforma, ProformaSelector};

use crate::output;

/// Proforma subcommands.
#[derive(Debug, Subcommand)]
pub enum ProformaCommand {
    /// Delete a proforma (issued invoices cannot be deleted, only reversed).
    Delete(DeleteArgs),
}

/// Arguments for `proforma delete`.
#[derive(Debug, Args)]
pub struct DeleteArgs {
    /// The proforma document number; alternatively use --order.
    number: Option<String>,
    /// Select by order number instead.
    #[arg(long, conflicts_with = "number")]
    order: Option<String>,
}

/// Runs a proforma subcommand.
pub async fn run(cli: &crate::Cli, command: &ProformaCommand) -> anyhow::Result<()> {
    let ProformaCommand::Delete(args) = command;
    let selector = match (&args.number, &args.order) {
        (Some(number), None) => ProformaSelector::InvoiceNumber(number.as_str().into()),
        (None, Some(order)) => ProformaSelector::OrderNumber(order.clone()),
        _ => anyhow::bail!("pass a proforma number or --order"),
    };
    let client = crate::client(cli)?;
    client.send(&DeleteProforma::new(selector)).await?;

    if cli.json {
        output::json(&serde_json::json!({ "deleted": true }))?;
    } else {
        println!("Proforma deleted.");
    }

    Ok(())
}
