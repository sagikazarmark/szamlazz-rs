//! Owned HTTP/2 serving over the SDK adapter. SDK 0.12's `HttpServer` detaches
//! connection tasks, and Hyper's default executor detaches stream tasks too.
//! Own both: graceful shutdown drains for ten seconds, then cancels the rest.

use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::Duration;

use hyper::server::conn::http2;
use hyper_util::{rt::TokioIo, server::graceful::GracefulShutdown};
use restate_sdk::{hyper::HyperEndpoint, prelude::Endpoint};
use tokio::{net::TcpListener, task::JoinSet};

const DRAIN: Duration = Duration::from_secs(10);

/// Hyper runs response bodies (and thus SDK handlers) in separate tasks.
/// Weak ownership prevents a task holding its executor from retaining its own
/// task registry when the serving future is dropped.
#[derive(Clone)]
struct Executor(Weak<Mutex<JoinSet<()>>>);

impl<F> hyper::rt::Executor<F> for Executor
where
    F: Future<Output = ()> + Send + 'static,
{
    fn execute(&self, future: F) {
        if let Some(tasks) = self.0.upgrade() {
            let mut tasks = tasks.lock().unwrap_or_else(PoisonError::into_inner);
            while tasks.try_join_next().is_some() {}
            tasks.spawn(future);
        }
    }
}

pub(super) async fn serve(endpoint: Endpoint, listener: TcpListener, stop: impl Future) {
    let endpoint = HyperEndpoint::new(endpoint);
    let streams = Arc::new(Mutex::new(JoinSet::new()));
    let executor = Executor(Arc::downgrade(&streams));
    let graceful = GracefulShutdown::new();
    let mut connections = JoinSet::new();
    tokio::pin!(stop);
    loop {
        tokio::select! {
            biased;
            _ = &mut stop => break,
            _ = connections.join_next(), if !connections.is_empty() => {}
            accepted = listener.accept() => {
                let (socket, _) = accepted.expect("accept an endpoint connection");
                let connection = http2::Builder::new(executor.clone())
                    .serve_connection(TokioIo::new(socket), endpoint.clone());
                let connection = graceful.watch(connection);
                connections.spawn(async move {
                    if let Err(error) = connection.await {
                        eprintln!("endpoint connection ended: {error}");
                    }
                });
            }
        }
    }
    drop(listener);
    let _ = tokio::time::timeout(DRAIN, graceful.shutdown()).await;
    // Finish cancelling connections before taking the stream registry: no
    // connection can submit more stream work after this point.
    connections.shutdown().await;
    let mut streams = std::mem::take(&mut *streams.lock().unwrap_or_else(PoisonError::into_inner));
    streams.shutdown().await;
}
