//! axum integration: [`PaymentNotification`] as an extractor.

use axum::extract::{FromRequest, Request};
use axum::response::{IntoResponse, Response};

use crate::{IpnParseError, PaymentNotification};

/// Rejection returned when a request is not a valid IPN message.
///
/// Responds with `400 Bad Request` (or forwards the body-extraction status,
/// e.g. `413`). Only malformed form encoding or missing document identity is
/// refused by the parser; unreadable or missing content is accepted. szamlazz.hu
/// retries non-200 answers up to 10 times and then discards the notification.
/// This extractor does not log or durably accept it on the application's behalf.
/// Implements [`Error`](std::error::Error), and
/// [`IpnRejection::parse_error`] exposes the underlying [`IpnParseError`] when
/// the body parsed as bytes but not as an IPN message.
#[derive(Debug)]
pub struct IpnRejection(RejectionKind);

#[derive(Debug)]
enum RejectionKind {
    Body(axum::extract::rejection::BytesRejection),
    Parse(IpnParseError),
}

impl IpnRejection {
    /// The parse failure, when the request body was read but did not form a
    /// valid IPN message. `None` when the body itself could not be extracted
    /// (too large, disconnected, …).
    #[must_use]
    pub fn parse_error(&self) -> Option<&IpnParseError> {
        match &self.0 {
            RejectionKind::Parse(error) => Some(error),
            RejectionKind::Body(_) => None,
        }
    }
}

impl std::fmt::Display for IpnRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            RejectionKind::Body(rejection) => write!(f, "invalid IPN request body: {rejection}"),
            RejectionKind::Parse(error) => std::fmt::Display::fmt(error, f),
        }
    }
}

impl std::error::Error for IpnRejection {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.0 {
            RejectionKind::Body(rejection) => Some(rejection),
            RejectionKind::Parse(error) => Some(error),
        }
    }
}

impl IntoResponse for IpnRejection {
    fn into_response(self) -> Response {
        let message = match self.0 {
            RejectionKind::Body(rejection) => return rejection.into_response(),
            RejectionKind::Parse(error) => error.to_string(),
        };
        (http::StatusCode::BAD_REQUEST, message).into_response()
    }
}

impl<S> FromRequest<S> for PaymentNotification
where
    S: Send + Sync,
{
    type Rejection = IpnRejection;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let body = axum::body::Bytes::from_request(req, state)
            .await
            .map_err(|rejection| IpnRejection(RejectionKind::Body(rejection)))?;

        Self::from_form_bytes(&body).map_err(|error| IpnRejection(RejectionKind::Parse(error)))
    }
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::http::{Request, StatusCode, header};
    use axum::routing::post;
    use tower::util::ServiceExt as _;

    use super::*;

    fn app() -> Router {
        Router::new().route(
            "/ipn",
            post(async |ipn: PaymentNotification| {
                assert_eq!(ipn.document_number, "E-2026-123");
                StatusCode::OK
            }),
        )
    }

    #[tokio::test]
    async fn extracts_valid_notification() {
        let request = Request::post("/ipn")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(axum::body::Body::from(
                "szlahu_szamlaszam=E-2026-123&szlahu_bruttovegosszeg=1&\
                 szlahu_kifizetettbrutto=1&szlahu_fizetesmod=kp",
            ))
            .expect("request");
        let response = app().oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn accepts_unknown_content_with_raw_text_intact() {
        let app = Router::new().route(
            "/ipn",
            post(async |ipn: PaymentNotification| {
                assert_eq!(ipn.document_number, "E-2026-123");
                assert_eq!(ipn.gross_total, None);
                assert_eq!(ipn.paid_gross, None);
                assert_eq!(ipn.payment_method, None);
                assert_eq!(ipn.payment_date, None);
                assert_eq!(ipn.raw_gross_total.as_deref(), Some(""));
                assert_eq!(ipn.raw_paid_gross.as_deref(), Some("bad"));
                assert_eq!(ipn.raw_payment_method, None);
                assert_eq!(ipn.raw_payment_date.as_deref(), Some("2026-07-04T12:30:00"));
                assert_eq!(ipn.is_fully_paid(), None);
                StatusCode::OK
            }),
        );
        let request = Request::post("/ipn")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(axum::body::Body::from(
                "szlahu_szamlaszam=E-2026-123&szlahu_bruttovegosszeg=&\
                 szlahu_kifizetettbrutto=bad&szlahu_kifizdat=2026-07-04T12%3A30%3A00",
            ))
            .expect("request");
        assert_eq!(
            app.oneshot(request).await.expect("response").status(),
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn accepts_identity_without_content() {
        let request = Request::post("/ipn")
            .body(axum::body::Body::from("szlahu_szamlaszam=E-2026-123"))
            .expect("request");
        assert_eq!(
            app().oneshot(request).await.expect("response").status(),
            StatusCode::OK
        );
    }

    #[test]
    fn rejection_exposes_parse_error_and_source() {
        let error = PaymentNotification::from_form_bytes(b"nonsense=1").expect_err("error");
        let rejection = IpnRejection(RejectionKind::Parse(error));
        // The typed cause is reachable for logging/branching...
        assert!(matches!(
            rejection.parse_error(),
            Some(IpnParseError::Missing("szlahu_szamlaszam"))
        ));
        // ...and the rejection participates in the std error chain.
        assert!(std::error::Error::source(&rejection).is_some());
        assert!(!rejection.to_string().is_empty());
    }

    #[tokio::test]
    async fn rejects_invalid_notification() {
        for body in [
            "nonsense=1",
            "szlahu_szamlaszam=+",
            "szlahu_szamlaszam=E-2026-123&future=%GG",
            "szlahu_szamlaszam=E-2026-123&future=%FF",
        ] {
            let request = Request::post("/ipn")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(axum::body::Body::from(body))
                .expect("request");
            let response = app().oneshot(request).await.expect("response");
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
        }
    }
}
