//! axum integration: [`PaymentNotification`] as an extractor.

use axum::extract::{FromRequest, Request};
use axum::response::{IntoResponse, Response};

use crate::{IpnParseError, PaymentNotification};

/// Rejection returned when a request is not a valid IPN message.
///
/// Responds with `400 Bad Request` (or forwards the body-extraction status,
/// e.g. `413`). szamlazz.hu will retry the delivery, so a malformed message
/// shows up in your logs up to 10 times — which is the desired signal for a
/// misconfigured integration. Implements [`Error`](std::error::Error), and
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
        let request = Request::post("/ipn")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(axum::body::Body::from("nonsense=1"))
            .expect("request");
        let response = app().oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
