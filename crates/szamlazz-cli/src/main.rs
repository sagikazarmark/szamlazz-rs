//! `szamlazz` — command line tool for szamlazz.hu.

mod commands;
mod output;

use clap::{Parser, Subcommand};

/// Interact with szamlazz.hu: issue and query documents, register credit
/// entries, and run local IPN and Adatkapcsolat receivers for development.
#[derive(Debug, Parser)]
#[command(name = "szamlazz", version, about)]
struct Cli {
    /// Számla Agent key; generate one on the szamlazz.hu dashboard. Prefer the
    /// `SZAMLAZZ_AGENT_KEY` environment variable — a key passed as a flag is
    /// visible in the process list and shell history.
    #[arg(
        long,
        env = "SZAMLAZZ_AGENT_KEY",
        global = true,
        hide_env_values = true
    )]
    agent_key: Option<String>,

    /// Override the API endpoint (for testing against a mock server).
    #[arg(long, env = "SZAMLAZZ_ENDPOINT", global = true, hide = true)]
    endpoint: Option<String>,

    /// Print machine-readable JSON instead of human-readable output.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create, fetch, download, or reverse invoices.
    #[command(subcommand)]
    Invoice(commands::invoice::InvoiceCommand),
    /// Register payments (credit entries) on an invoice.
    #[command(subcommand)]
    Payment(commands::payment::PaymentCommand),
    /// Manage proformas (díjbekérő).
    #[command(subcommand)]
    Proforma(commands::proforma::ProformaCommand),
    /// Create, fetch, reverse, or email receipts (nyugta).
    #[command(subcommand)]
    Receipt(commands::receipt::ReceiptCommand),
    /// Look up a taxpayer by the first 8 digits of its tax number.
    Taxpayer(commands::taxpayer::TaxpayerArgs),
    /// Run local IPN and Adatkapcsolat receivers that pretty-print incoming
    /// messages.
    Listen(commands::listen::ListenArgs),
}

fn client(cli: &Cli) -> anyhow::Result<szamlazz_agent::Client> {
    let key = cli.agent_key.clone().ok_or_else(|| {
        anyhow::anyhow!("no agent key: pass --agent-key or set SZAMLAZZ_AGENT_KEY")
    })?;
    let mut builder =
        szamlazz_agent::Client::builder().credentials(szamlazz_agent::Credentials::agent_key(key));

    if let Some(endpoint) = &cli.endpoint {
        builder = builder.endpoint(endpoint.clone());
    }

    Ok(builder.build()?)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Command::Invoice(command) => commands::invoice::run(&cli, command).await,
        Command::Payment(command) => commands::payment::run(&cli, command).await,
        Command::Proforma(command) => commands::proforma::run(&cli, command).await,
        Command::Receipt(command) => commands::receipt::run(&cli, command).await,
        Command::Taxpayer(args) => commands::taxpayer::run(&cli, args).await,
        Command::Listen(args) => commands::listen::run(args).await,
    }
}
