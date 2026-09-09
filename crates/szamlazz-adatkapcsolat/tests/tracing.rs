//! The `tracing` feature: the router logs a handler's or a resolver's error
//! at `warn`, and the response stays the bare status.
//!
//! Its own binary, on purpose: `tracing` caches each callsite's interest the
//! first time the callsite is hit, and a test thread hitting the router's
//! `warn!` with no subscriber installed while this test installs one can
//! leave the callsite cached as never interesting. With nothing else in the
//! process, the callsites are first hit under the subscriber.

#![cfg(all(feature = "axum", feature = "tracing"))]

mod common;

use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use common::axum::{UnavailableResolver, call, request, send};
use common::{BANK_TRANSACTION, OUTGOING_INVOICE};

/// Everything the `tracing` feature logs while `run` runs, as text.
async fn logged<F: Future>(run: F) -> (F::Output, String) {
    use std::io::Write;

    use tracing_subscriber::fmt::MakeWriter;

    #[derive(Clone, Default)]
    struct Sink(Arc<Mutex<Vec<u8>>>);

    impl Write for Sink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("sink").extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Sink {
        type Writer = Self;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    let sink = Sink::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(sink.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::WARN)
        .finish();
    // A thread-local default: the test runtime is current-thread, so every
    // poll of `run` happens under it.
    let guard = tracing::subscriber::set_default(subscriber);
    let output = run.await;
    drop(guard);
    let log = String::from_utf8(sink.0.lock().expect("sink").clone()).expect("utf-8 log");

    (output, log)
}

// With the `tracing` feature the router is where a handler's or a resolver's
// error is logged, at `warn`, with the pushed kind; the response stays the
// bare status either way.
#[tokio::test]
async fn tracing_feature_logs_handler_and_resolver_errors_at_warn() {
    let ((status, body), log) = logged(call(Some("secret-key"), OUTGOING_INVOICE, true)).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body, "handler error");
    assert!(log.contains("WARN"), "{log}");
    assert!(log.contains("handler failed"), "{log}");
    assert!(log.contains("error=database down"), "{log}");
    assert!(log.contains("kind=szamla"), "{log}");
    assert_eq!(log.lines().count(), 1, "{log}");

    let ((status, body), log) = logged(async {
        let app = szamlazz_adatkapcsolat::axum::router_with_resolver(UnavailableResolver);
        send(app, request(Some("any-key"), BANK_TRANSACTION.as_bytes())).await
    })
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body, "key resolver unavailable");
    assert!(log.contains("WARN"), "{log}");
    assert!(
        log.contains("key resolver could not check the key"),
        "{log}"
    );
    assert!(
        log.contains("error=key store timed out: connection timed out"),
        "{log}"
    );
    assert!(log.contains("kind=banktranz"), "{log}");
    assert_eq!(log.lines().count(), 1, "{log}");

    // A push that is answered without an error logs nothing at `warn`.
    let ((status, _), log) = logged(call(Some("secret-key"), OUTGOING_INVOICE, false)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(log.is_empty(), "{log}");
    let ((status, _), log) = logged(call(Some("not-the-key"), OUTGOING_INVOICE, false)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(log.is_empty(), "{log}");
}
