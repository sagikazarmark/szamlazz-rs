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

/// Shared with the owner so even an unexpected exit of the serving task cannot
/// lose failures already observed in its connection or stream tasks.
#[derive(Clone, Default)]
pub(super) struct Failures(Arc<Mutex<Vec<String>>>);

impl Failures {
    pub(super) fn record(
        &self,
        task: &str,
        result: Result<(), tokio::task::JoinError>,
        shutting_down: bool,
    ) {
        if let Err(error) = result {
            // Forced shutdown deliberately cancels unfinished work. A panic
            // already in the join queue still counts, even during shutdown.
            if !(shutting_down && error.is_cancelled()) {
                self.0
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(format!("{task}: {error}"));
            }
        }
    }

    pub(super) fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

/// Hyper runs response bodies (and thus SDK handlers) in separate tasks.
/// Weak ownership prevents a task holding its executor from retaining its own
/// task registry when the serving future is dropped.
#[derive(Clone)]
struct Executor {
    tasks: Weak<Mutex<JoinSet<()>>>,
    failures: Failures,
}

impl<F> hyper::rt::Executor<F> for Executor
where
    F: Future<Output = ()> + Send + 'static,
{
    fn execute(&self, future: F) {
        if let Some(tasks) = self.tasks.upgrade() {
            let mut tasks = tasks.lock().unwrap_or_else(PoisonError::into_inner);
            while let Some(result) = tasks.try_join_next() {
                self.failures.record("stream task", result, false);
            }
            tasks.spawn(future);
        }
    }
}

pub(super) async fn serve(
    endpoint: Endpoint,
    listener: TcpListener,
    stop: impl Future,
    failures: Failures,
) {
    let endpoint = HyperEndpoint::new(endpoint);
    let streams = Arc::new(Mutex::new(JoinSet::new()));
    let executor = Executor {
        tasks: Arc::downgrade(&streams),
        failures: failures.clone(),
    };
    let graceful = GracefulShutdown::new();
    let mut connections = JoinSet::new();
    tokio::pin!(stop);
    // Keep the registries outside the fallible serving loop so unwinding it
    // cannot discard child joins. Cleanup below is also used after a panic.
    let serving = async {
        loop {
            tokio::select! {
                biased;
                _ = &mut stop => break,
                Some(result) = connections.join_next(), if !connections.is_empty() => {
                    failures.record("connection task", result, false);
                }
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
    };
    if let Err(panic) = catch_panic(serving).await {
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or("non-text panic payload");
        failures
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(format!("serving task: {message}"));
    }
    drop(listener);
    let _ = tokio::time::timeout(DRAIN, graceful.shutdown()).await;
    // Finish cancelling connections before taking the stream registry: no
    // connection can submit more stream work after this point.
    connections.abort_all();
    while let Some(result) = connections.join_next().await {
        failures.record("connection task", result, true);
    }
    let mut streams = std::mem::take(&mut *streams.lock().unwrap_or_else(PoisonError::into_inner));
    streams.abort_all();
    while let Some(result) = streams.join_next().await {
        failures.record("stream task", result, true);
    }
}

/// Catch only unwinding of the serving loop; its children remain owned by the
/// caller and are cancelled and joined normally. No process-wide panic hook.
async fn catch_panic(future: impl Future<Output = ()>) -> std::thread::Result<()> {
    tokio::pin!(future);
    std::future::poll_fn(|cx| {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| future.as_mut().poll(cx))) {
            Ok(poll) => poll.map(Ok),
            Err(panic) => std::task::Poll::Ready(Err(panic)),
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn serving_failure_still_drains_an_existing_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");
        let failures = Failures::default();
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let serving = tokio::spawn(serve(
            Endpoint::builder().build(),
            listener,
            async {
                stopped.await.expect("stop");
                panic!("serving loop panic");
            },
            failures.clone(),
        ));
        let socket = tokio::net::TcpStream::connect(address)
            .await
            .expect("socket");
        let (mut client, connection) = h2::client::handshake(socket).await.expect("handshake");
        let driver = tokio::spawn(connection);
        let request = http::Request::builder()
            .uri(format!("http://{address}/discover"))
            .body(())
            .expect("request");
        let (response, _) = client.send_request(request, true).expect("stream");
        let mut response = response.await.expect("the loop accepted a connection");
        while response.body_mut().data().await.is_some() {}
        stop.send(()).expect("panic the serving loop");
        tokio::time::timeout(Duration::from_secs(5), serving)
            .await
            .expect("serving cleanup completes")
            .expect("panic is collected after cleanup");
        let failures = failures.take();
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(failures[0].contains("serving loop panic"));
        tokio::time::timeout(Duration::from_secs(5), driver)
            .await
            .expect("connection closes")
            .expect("driver task")
            .expect("graceful close");
    }
}
