//! The SDK-generated client keeps raw errors when a worker fault does not fit.

use bytes::Bytes;
use http::{Request, Response};
use restate_sdk::ingress::{Client, RequestExecutor};
use restate_szamlazz::contract::{FaultCause, TerminalCode};
use restate_szamlazz::service::{AgentIngressClient, decode_fault};
use serde_json::json;

struct Reply {
    status: u16,
    source: &'static str,
    body: Bytes,
}

impl RequestExecutor for Reply {
    type Error = std::convert::Infallible;

    fn execute(
        &self,
        _: Request<Bytes>,
    ) -> impl Future<Output = Result<Response<Bytes>, Self::Error>> + Send {
        std::future::ready(Ok(Response::builder()
            .status(self.status)
            .header("x-restate-error-source", self.source)
            .header("content-type", "application/json")
            .body(self.body.clone())
            .expect("response")))
    }
}

async fn call(
    status: u16,
    source: &'static str,
    body: String,
) -> restate_sdk::ingress::ClientError {
    let client = Client::new(
        "http://localhost:8080".parse().expect("uri"),
        Reply {
            status,
            source,
            body: Bytes::from(body),
        },
    )
    .expect("client");
    AgentIngressClient::from_client(client)
        .check_account()
        .call()
        .await
        .err()
        .expect("fault")
}

fn envelope(status: u16, message: &str) -> String {
    json!({"code": status, "message": message, "source": "invocation"}).to_string()
}

#[tokio::test]
async fn generated_client_decodes_faults_but_keeps_native_and_unfamiliar_errors() {
    let error = call(
        500,
        "invocation",
        envelope(
            500,
            r#"{"code":"outcome_unknown","message":"stopped","cause":"cancelled"}"#,
        ),
    )
    .await;
    let fault = decode_fault(&error).expect("structured worker fault");
    assert_eq!(fault.code, TerminalCode::OutcomeUnknown);
    assert_eq!(fault.cause, Some(FaultCause::Cancelled));
    assert_eq!(fault.is_cancelled(), Some(true));
    assert_eq!(error.response().expect("retained").status(), 500);

    for (status, source, body) in [
        (409, "invocation", envelope(409, "cancelled")),
        (500, "invocation", envelope(500, "killed")),
        (503, "ingress", envelope(503, r#"{"code":"unavailable","message":"not invocation"}"#)),
        (409, "invocation", "not JSON".to_owned()),
        (409, "invocation", json!({"code":409,"message":{},"source":"invocation"}).to_string()),
        (409, "invocation", envelope(500, r#"{"code":"cancelled","message":"mismatched envelope"}"#)),
        (500, "invocation", envelope(500, r#"{"code":"cancelled","message":"mismatched fault status"}"#)),
        (409, "invocation", json!({"code":409,"message":r#"{"code":"cancelled","message":"wrong source"}"#,"source":"ingress"}).to_string()),
    ] {
        let error = call(status, source, body.clone()).await;
        assert!(decode_fault(&error).is_none(), "{error:?}");
        assert_eq!(error.response().expect("raw error retained").body().as_ref(), body.as_bytes());
    }
    let error = call(
        599,
        "invocation",
        envelope(
            599,
            r#"{"code":"future","message":"new fault","cause":"future"}"#,
        ),
    )
    .await;
    let fault = decode_fault(&error).expect("open fault");
    assert_eq!(fault.status(), None);
    assert_eq!(fault.is_cancelled(), None);
    assert_eq!(error.response().expect("actual status").status(), 599);

    let transport = restate_sdk::ingress::ClientError::Transport {
        source: Box::new(std::io::Error::other("disconnected")),
    };
    assert!(decode_fault(&transport).is_none());
}
