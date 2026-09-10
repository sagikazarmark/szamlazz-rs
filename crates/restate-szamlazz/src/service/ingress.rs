//! Caller-side decoding of the worker's structured faults.

use restate_sdk::ingress::ClientError;
use serde::Deserialize;

use crate::contract::Fault;

/// Attempt to decode a worker fault from an SDK-generated ingress client's
/// error, retaining the original error and its buffered response in the caller.
///
/// Checks the actual HTTP error status and `x-restate-error-source`, then the
/// Restate envelope's source/status and inner JSON string. Native cancellation
/// or kill messages, transport/ingress failures, unfamiliar shapes and
/// inconsistent known statuses return `None`. Unknown fault codes and causes
/// are preserved: use the actual response status and leave them unclassified.
/// No retry decision is inferred from either a status or English prose.
///
/// `Fault`'s optional `JsonSchema` derive does not publish it through Restate's
/// success-output discovery schema. Generated clients therefore need this
/// additional decoding step:
///
/// ```no_run
/// use restate_szamlazz::restate_sdk::ingress::{Client, ClientError, RequestExecutor};
/// use restate_szamlazz::service::{AgentIngressClient, decode_fault};
///
/// async fn check<E: RequestExecutor>(client: Client<E>) -> Result<(), ClientError> {
///     let agent = AgentIngressClient::from_client(client);
///     match agent.check_account().call().await {
///         Ok(reply) => { let _check = reply.into_body()?; }
///         Err(error) => {
///             if let Some(fault) = decode_fault(&error) {
///                 // Cancellation is independent of write uncertainty. Do not
///                 // automatically retry; reconcile a cancelled write first.
///                 let cancelled = fault.is_cancelled();
///                 let uncertain = fault.code.is_outcome_unknown();
///                 let _classification = (cancelled, uncertain);
///             }
///             // Includes native cancellation/kill and non-invocation errors.
///             // Keep the source, actual status, headers and raw body intact.
///             return Err(error);
///         }
///     }
///     Ok(())
/// }
/// ```
#[must_use]
pub fn decode_fault(error: &ClientError) -> Option<Fault> {
    let response = error.response()?;
    let status = response.status();
    if !(status.is_client_error() || status.is_server_error())
        || response.headers().get("x-restate-error-source")? != "invocation"
    {
        return None;
    }
    let envelope: Envelope = serde_json::from_slice(response.body()).ok()?;
    if envelope.source != "invocation" || envelope.code != status.as_u16() {
        return None;
    }
    let fault: Fault = serde_json::from_str(&envelope.message).ok()?;
    if fault
        .status()
        .is_some_and(|expected| expected != status.as_u16())
    {
        return None;
    }
    Some(fault)
}

#[derive(Deserialize)]
struct Envelope {
    code: u16,
    source: String,
    message: String,
}
