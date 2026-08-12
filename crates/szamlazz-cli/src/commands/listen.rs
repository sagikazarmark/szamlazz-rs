//! `szamlazz listen` — a development receiver for IPN and Adatkapcsolat.
//!
//! Serves the IPN endpoint at `/ipn` and the Adatkapcsolat endpoint at
//! `/adatkapcsolat`, pretty-printing every message. Point szamlazz.hu (or a
//! tunnel like `cloudflared`) at it to watch real traffic during integration
//! work.

use axum::Router;
use axum::http::StatusCode;
use axum::routing::post;
use clap::Args;
use szamlazz_adatkapcsolat::{
    Ack, BankTransaction, Handler, InvoiceAck, InvoiceDocument, ReceiptBatch,
};
use szamlazz_ipn::PaymentNotification;

/// Arguments for the `listen` command.
#[derive(Debug, Args)]
pub struct ListenArgs {
    /// Address to bind.
    #[arg(long, default_value = "127.0.0.1:8080")]
    bind: String,

    /// Expected X-Szamlazzhu-Key for the Adatkapcsolat endpoint; requests
    /// with a different key are answered `KEY_ERR` per protocol. Prefer the
    /// `SZAMLAZZ_ADATKAPCSOLAT_KEY` environment variable — a key passed as a
    /// flag is visible in the process list and shell history.
    #[arg(long, env = "SZAMLAZZ_ADATKAPCSOLAT_KEY", hide_env_values = true)]
    adatkapcsolat_key: Option<String>,
}

#[derive(Clone)]
struct PrintingHandler;

impl Handler for PrintingHandler {
    type Error = std::convert::Infallible;

    async fn outgoing_invoice(&self, invoice: InvoiceDocument) -> Result<InvoiceAck, Self::Error> {
        let id = invoice.info.id;
        println!("── adatkapcsolat: outgoing invoice (id {id}) ──");
        print_json(&invoice);

        Ok(InvoiceAck::accept(id))
    }

    async fn incoming_invoice(&self, invoice: InvoiceDocument) -> Result<InvoiceAck, Self::Error> {
        let id = invoice.info.id;
        println!("── adatkapcsolat: incoming invoice (id {id}) ──");
        print_json(&invoice);

        Ok(InvoiceAck::accept(id))
    }

    async fn bank_transaction(&self, tx: BankTransaction) -> Result<Ack, Self::Error> {
        println!("── adatkapcsolat: bank transaction (id {}) ──", tx.id);
        print_json(&tx);

        Ok(Ack::accept())
    }

    async fn receipts(&self, batch: ReceiptBatch) -> Result<Ack, Self::Error> {
        println!(
            "── adatkapcsolat: receipt batch ({} receipts) ──",
            batch.receipts.len()
        );
        print_json(&batch);

        Ok(Ack::accept())
    }
}

fn print_json<T: serde::Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(json) => println!("{json}"),
        Err(error) => eprintln!("(failed to render: {error})"),
    }
}

async fn ipn(notification: PaymentNotification) -> StatusCode {
    println!(
        "── IPN: payment status for {} ──",
        notification.document_number
    );
    print_json(&notification);

    StatusCode::OK
}

/// Runs the receiver until interrupted.
pub async fn run(args: &ListenArgs) -> anyhow::Result<()> {
    let mut app = Router::new().route("/ipn", post(ipn));

    match &args.adatkapcsolat_key {
        Some(key) => {
            let receiver = szamlazz_adatkapcsolat::axum::router(key.clone(), PrintingHandler);
            app = szamlazz_adatkapcsolat::axum::nest_at(app, "/adatkapcsolat", receiver);
            eprintln!("adatkapcsolat: POST /adatkapcsolat[/] (key configured)");
        }
        None => {
            eprintln!("adatkapcsolat: disabled (pass --adatkapcsolat-key to enable)");
        }
    }

    eprintln!("IPN:            POST /ipn");
    eprintln!("listening on http://{}", args.bind);

    let listener = tokio::net::TcpListener::bind(&args.bind).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;

    Ok(())
}
