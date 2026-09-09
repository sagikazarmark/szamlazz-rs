//! How a fault travels the wire under Restate: the contract's refusals
//! reaching the caller as the structured `invalid_input` fault (not the SDK's
//! plain-text decode error), a URL-encoded key reaching the handler as
//! decoded, and the ingress envelope around a fault carrying a szamlazz.hu
//! code. Which bodies and keys are refused, and which fault each answer maps
//! onto, are unit tests of `service` (`Body<T>`, the key check, the fault →
//! status table); one case of each is enough to prove the envelope.

use rust_decimal::dec;
use serde_json::json;

use restate_szamlazz::contract::TerminalCode;

use crate::harness::szamlazz::{api_error, create_never_sent, number_query};
use crate::harness::{Harness, create_body, document};

/// A malformed body (a misspelt `reissue`) is refused as the structured
/// `invalid_input` fault (400, `{code, message}` naming the field), not
/// accepted as `reissue: false` and not the SDK's plain-text `Cannot decode
/// input payload`; refused before the prologue, nothing journaled, nothing
/// sent. An untrimmed Virtual Object key (`%20E2E-10c`, which the ingress
/// decodes to ` E2E-10c`) is refused likewise, naming the rule: Restate's
/// per-key lock is on the *raw* key, so ` E2E-10c` and `E2E-10c` would be two
/// instances with two locks mapping to one szamlazz.hu order. And a
/// szamlazz.hu code on `Szamlazz.Agent.query` reaches the caller as 422
/// `szamlazz_error` with the code in `szamlazz_code` beside `code`, inside
/// Restate's error envelope (`{code: <status>, message, source:
/// "invocation"}` under `x-restate-error-source: invocation`, the fault the
/// JSON string in `message`; asserted by `Reply::fault`).
pub(crate) async fn refusals_and_szamlazz_codes_travel_as_structured_faults(h: &Harness) {
    create_never_sent(&h.mock, "E2E-10b").await;
    create_never_sent(&h.mock, "E2E-10c").await;
    number_query("SZ-28")
        .respond_with(api_error("57", "Hibás számlaszám."))
        .expect(1)
        .mount(&h.mock)
        .await;

    // The malformed body.
    let reply = h
        .call(
            "E2E-10b",
            "create_invoice",
            &json!({ "document": document(dec!(1000)), "options": { "resissue": true } }),
            "e2e-10b-k1",
        )
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, TerminalCode::InvalidInput, "{fault:?}");
    assert!(
        fault.message.contains("unknown field `resissue`"),
        "names the field: {fault:?}"
    );
    assert_eq!(fault.order, None, "{fault:?}");
    assert!(
        h.runs(reply.invocation_id()).await.is_empty(),
        "refused before the prologue: nothing journaled"
    );
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.handler, "create_invoice");
    assert!(
        invocation
            .completion_failure
            .as_deref()
            .is_some_and(|failure| failure.contains("invalid_input")),
        "{invocation:?}"
    );
    assert!(
        h.requests_of_order("E2E-10b").await.is_empty(),
        "nothing reached szamlazz.hu"
    );

    // The untrimmed key.
    let reply = h
        .call(
            "%20E2E-10c",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-10c-k1",
        )
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, TerminalCode::InvalidInput, "{fault:?}");
    assert!(
        fault
            .message
            .contains("must not have leading or trailing whitespace"),
        "names the rule: {fault:?}"
    );
    assert!(
        h.runs(reply.invocation_id()).await.is_empty(),
        "refused before the prologue: nothing journaled"
    );
    assert!(
        h.requests_of_order("E2E-10c").await.is_empty(),
        "nothing reached szamlazz.hu"
    );

    // The envelope around a szamlazz.hu code.
    let reply = h
        .call_agent(
            "query",
            &json!({ "selector": { "invoice_number": "SZ-28" } }),
        )
        .await;
    assert_eq!(reply.status, 422, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, TerminalCode::SzamlazzError, "{fault:?}");
    assert_eq!(fault.szamlazz_code.as_deref(), Some("57"), "{fault:?}");
    assert!(fault.message.contains("Hibás számlaszám."), "{fault:?}");
    assert_eq!(fault.order, None, "{fault:?}");
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account", "query"],
        "the answer was journaled as data"
    );
}
