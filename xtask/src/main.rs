//! Repository automation entry point.
use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Read-only Restate 1.7.8 inventory. Quiesce producers before running.
    CheckOrderMigration {
        #[arg(long)]
        admin_url: String,
    },
    /// Offline XSD 1.0 validation; requires xmllint. Generates requests if omitted.
    CheckAgentSchemas {
        #[arg(long)]
        requests: Option<PathBuf>,
        #[arg(long)]
        corpus: Option<PathBuf>,
    },
    /// Export requests and executables for a separate schema-validation container.
    PrepareAgentSchemas {
        #[arg(long)]
        output: PathBuf,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let result = match Args::parse().command {
        Command::CheckOrderMigration { admin_url } => {
            return ExitCode::from(xtask::migration::run(&admin_url).await);
        }
        Command::CheckAgentSchemas { requests, corpus } => {
            xtask::schemas::check(requests.as_deref(), corpus.as_deref()).await
        }
        Command::PrepareAgentSchemas { output } => xtask::schemas::prepare(&output).await,
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("SCHEMA CHECK FAILED: {error:#}");
            ExitCode::FAILURE
        }
    }
}
