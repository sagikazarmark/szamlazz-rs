//! What the contract refuses and how a fault travels: a malformed body, an
//! untrimmed order key, bounded inputs (#64), and every fault carrying a
//! `TerminalCode` with the szamlazz.hu code beside it (#67).

use std::time::{Duration, Instant};

use rust_decimal::{Decimal, dec};
use serde_json::json;
use wiremock::matchers::body_string_contains;

use crate::harness::szamlazz::{
    Doc, api_error, create, created, credit, not_found, number_query, order_query,
    storno_never_sent, storno_of,
};
use crate::harness::{Harness, create_body, document};

/// (x-b) a malformed body — one carrying a field the contract does not
/// know, a misspelt `reissue` — is refused as the structured `invalid_input`
/// fault (400, `{code, message}` naming the field), not accepted as
/// `reissue: false` and not the SDK's plain-text `Cannot decode input
/// payload`. Refused before the prologue: nothing journaled, nothing sent,
/// the create mock `expect(0)`. A nested misspelling and a wrong type are
/// the same fault.
pub(crate) async fn a_malformed_body_is_a_structured_invalid_input(h: &Harness) {
    h.reset().await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let before = h.requests_seen().await;
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
    assert_eq!(fault.code, "invalid_input", "{fault:?}");
    assert!(
        fault.message.contains("unknown field `resissue`"),
        "names the field: {fault:?}"
    );
    assert_eq!(fault.order, None, "{fault:?}");
    assert_eq!(
        h.requests_seen().await,
        before,
        "nothing reached szamlazz.hu"
    );
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

    // A nested one — `buyer.tax_numer` — and a wrong type are the same
    // fault; the caller never gets an invoice without the tax number.
    let mut body = json!({ "document": document(dec!(1000)) });
    body["document"]["buyer"]["tax_numer"] = json!("12345678-2-42");
    let reply = h
        .call("E2E-10b", "create_invoice", &body, "e2e-10b-k2")
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "invalid_input", "{fault:?}");
    assert!(fault.message.contains("`tax_numer`"), "{fault:?}");

    let reply = h
        .call(
            "E2E-10b",
            "delete_proforma",
            &json!({ "force": "yes" }),
            "e2e-10b-k3",
        )
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    assert_eq!(reply.fault().code, "invalid_input", "{}", reply.body);
    assert_eq!(
        h.requests_seen().await,
        before,
        "nothing reached szamlazz.hu"
    );
    eprintln!("(x-b) malformed body → structured invalid_input, nothing issued: pass");
}

/// (x-c) a Virtual Object key that is not trimmed — `%20E2E-10c`, which the
/// ingress decodes to ` E2E-10c` — is refused as `invalid_input` naming the
/// rule. Restate's per-key lock is on the *raw* key, so ` E2E-10c` and
/// `E2E-10c` would be two instances with two locks mapping to one szamlazz.hu
/// order and identical external ids, and two concurrent creates under them
/// would both pass their lookup and both send; the caller trims (design §3).
/// Refused after the body decode and before the prologue: nothing journaled,
/// nothing sent, the create mock `expect(0)`. A trailing space is the same
/// fault; the trimmed key is accepted as before.
pub(crate) async fn an_untrimmed_order_key_is_refused(h: &Harness) {
    h.reset().await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let before = h.requests_seen().await;
    for (i, key) in ["%20E2E-10c", "E2E-10c%20", "%20E2E-10c%20"]
        .into_iter()
        .enumerate()
    {
        let reply = h
            .call(
                key,
                "create_invoice",
                &create_body(dec!(1000), false),
                &format!("e2e-10c-k{i}"),
            )
            .await;
        assert_eq!(reply.status, 400, "{key}: {}", reply.body);
        let fault = reply.fault();
        assert_eq!(fault.code, "invalid_input", "{key}: {fault:?}");
        assert!(
            fault
                .message
                .contains("must not have leading or trailing whitespace"),
            "{key}: names the rule: {fault:?}"
        );
        assert_eq!(fault.order, None, "{key}: {fault:?}");
        assert!(
            h.runs(reply.invocation_id()).await.is_empty(),
            "{key}: refused before the prologue: nothing journaled"
        );
        let invocation = h.invocation(reply.invocation_id()).await;
        assert_eq!(invocation.handler, "create_invoice");
        assert!(
            invocation
                .completion_failure
                .as_deref()
                .is_some_and(|failure| failure.contains("invalid_input")),
            "{key}: {invocation:?}"
        );
    }
    assert_eq!(
        h.requests_seen().await,
        before,
        "nothing reached szamlazz.hu"
    );
    eprintln!(
        "(x-c) untrimmed order key → invalid_input naming the rule, nothing journaled or issued: pass"
    );
}

/// (x-e) bounded inputs (#64). An order key outside the alphabet — an
/// internal space (`E2E%2010d`, which the ingress decodes to `E2E 10d`), a
/// `:`, 41 bytes — and an `invoice_number` over 40 bytes are refused as
/// `invalid_input` naming the rule before the prologue: nothing journaled,
/// nothing sent. A body whose line-item arithmetic overflows a decimal is the
/// same fault from the handler's own validation — after the prologue's two
/// entries (`namespace`, `account`), which the check needs for the account's
/// currency defaults, and before any read — never a panic: the request is
/// sent beside a healthy create on another order against the same endpoint,
/// and that create is `issued` without a retry; exactly one create reaches
/// szamlazz.hu, the healthy order's.
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the three bounds, then the overflow beside a healthy create"
)]
pub(crate) async fn bounded_inputs_are_refused_and_disturb_no_other_invocation(h: &Harness) {
    h.reset().await;
    let before = h.requests_seen().await;

    // The order-key alphabet, at the handler's key check.
    let too_long = "x".repeat(41);
    for (i, (key, rule)) in [
        ("E2E%2010d", "must not contain whitespace"),
        ("E2E:10d", "must not contain ':'"),
        (too_long.as_str(), "at most 40 are allowed"),
    ]
    .into_iter()
    .enumerate()
    {
        let reply = h
            .call(
                key,
                "create_invoice",
                &create_body(dec!(1000), false),
                &format!("e2e-10d-k{i}"),
            )
            .await;
        assert_eq!(reply.status, 400, "{key}: {}", reply.body);
        let fault = reply.fault();
        assert_eq!(fault.code, "invalid_input", "{key}: {fault:?}");
        assert!(
            fault.message.contains(rule),
            "{key}: names the rule: {fault:?}"
        );
        assert_eq!(fault.order, None, "{key}: {fault:?}");
        assert!(
            h.runs(reply.invocation_id()).await.is_empty(),
            "{key}: refused before the prologue: nothing journaled"
        );
    }

    // The invoice-number bound, at the body decode (a malformed body).
    let reply = h
        .call(
            "E2E-10d",
            "storno_invoice",
            &json!({ "invoice_number": too_long }),
            "e2e-10d-k3",
        )
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "invalid_input", "{fault:?}");
    assert!(
        fault
            .message
            .contains("invoice number is 41 bytes long, at most 40 are allowed"),
        "names the rule: {fault:?}"
    );
    assert!(
        h.runs(reply.invocation_id()).await.is_empty(),
        "refused before the prologue: nothing journaled"
    );
    assert_eq!(
        h.requests_seen().await,
        before,
        "nothing reached szamlazz.hu"
    );

    // The overflow, beside a healthy create on another order.
    h.absent("E2E-10e", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-10e")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    h.holds_after_misses(
        2,
        &Doc {
            external_id: Some("acct:E2E-10e:invoice"),
            ..Doc::new("SZ-10e", "SZ", "E2E-10e")
        },
    )
    .await;
    create()
        .respond_with(created("SZ-10e", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let mut overflowing = document(Decimal::MAX);
    overflowing.items[0].quantity = dec!(10);
    let overflowing = json!({ "document": overflowing, "options": {} });

    let healthy_body = create_body(dec!(1000), false);
    let started = Instant::now();
    let (healthy, refused) = tokio::join!(
        h.call("E2E-10e", "create_invoice", &healthy_body, "e2e-10e-k1"),
        h.call("E2E-10d", "create_invoice", &overflowing, "e2e-10d-k4"),
    );
    let elapsed = started.elapsed();

    assert_eq!(refused.status, 400, "{}", refused.body);
    let fault = refused.fault();
    assert_eq!(fault.code, "invalid_input", "{fault:?}");
    assert!(
        fault.message.contains("items[0]") && fault.message.contains("overflows a decimal"),
        "names the item and the rule: {fault:?}"
    );
    assert_eq!(
        h.runs(refused.invocation_id()).await,
        ["namespace", "account"],
        "the prologue ran (the check needs the account), no read did"
    );

    assert_eq!(healthy.status, 200, "{}", healthy.body);
    assert_eq!(healthy.body["outcome"], "issued", "{}", healthy.body);
    assert_eq!(healthy.body["invoice_number"], "SZ-10e");
    let creates = h.create_bodies().await;
    assert_eq!(creates.len(), 1, "exactly one create on the wire");
    assert!(
        creates[0].contains("<rendelesSzam>E2E-10e</rendelesSzam>"),
        "the healthy order's: {}",
        creates[0]
    );
    // A torn-down connection would have the healthy create retried no sooner
    // than the handler's two-minute `initial_interval`; it answered at once.
    assert!(
        elapsed < Duration::from_secs(60),
        "the healthy create was not retried: {elapsed:?}"
    );
    eprintln!(
        "(x-e) bounded inputs → invalid_input naming the rule; an overflowing body beside a healthy create issues nothing and disturbs nothing: pass"
    );
}

/// (xviii-c') Every fault either service raises carries a `TerminalCode` token
/// in `code`, and a szamlazz.hu code travels in `szamlazz_code` beside it,
/// never in `code` (#67). Pinned at the ingress, on both services: a
/// szamlazz.hu code on `Szamlazz.Agent.query` is 422 `szamlazz_error` with
/// `szamlazz_code`; a sixth credit entry on `set_payments` never reaches
/// szamlazz.hu and is 400 `invalid_input` without a `szamlazz_code`; a
/// credit entry szamlazz.hu refuses is 422 `szamlazz_error` naming the
/// invoice; an unknown invoice on `Szamlazz.Order.storno_invoice` is 404
/// `not_found` — the same token `Szamlazz.Agent` answers — attaching the
/// order, kind and storno external id.
pub(crate) async fn every_fault_carries_a_terminal_code_and_the_szamlazz_code_beside_it(
    h: &Harness,
) {
    let credit_entry = json!({ "date": "2026-09-05", "method": "transfer", "amount": "1270" });
    let set_payments_of = |number: &str, entries: usize| {
        json!({
            "invoice_number": number,
            "entries": vec![credit_entry.clone(); entries],
        })
    };

    // A szamlazz.hu code on `query`: the pass-through.
    h.reset().await;
    number_query("SZ-28")
        .respond_with(api_error("57", "Hibás számlaszám."))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent_scoped(
            "acme",
            "query",
            &json!({ "selector": { "invoice_number": "SZ-28" } }),
        )
        .await;
    assert_eq!(reply.status, 422, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "szamlazz_error", "{fault:?}");
    assert_eq!(fault.szamlazz_code.as_deref(), Some("57"), "{fault:?}");
    assert!(fault.message.contains("Hibás számlaszám."), "{fault:?}");
    assert_eq!(fault.order, None, "{fault:?}");

    // A sixth credit entry: the caller's request, nothing sent.
    h.reset().await;
    credit()
        .respond_with(api_error("999", "never"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent_scoped("acme", "set_payments", &set_payments_of("SZ-29", 6))
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "invalid_input", "{fault:?}");
    assert_eq!(fault.szamlazz_code, None, "{fault:?}");
    assert!(fault.message.contains("at most five"), "{fault:?}");
    assert_eq!(h.requests_seen().await, 0, "nothing reached szamlazz.hu");

    // A refused credit entry: szamlazz.hu's answer, passed through.
    h.reset().await;
    credit()
        .and(body_string_contains("SZ-30"))
        .respond_with(api_error(
            "463",
            "Sztornózó vagy sztornózott számlához nem tartozhat kifizetettségi információ.",
        ))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent_scoped("acme", "set_payments", &set_payments_of("SZ-30", 1))
        .await;
    assert_eq!(reply.status, 422, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "szamlazz_error", "{fault:?}");
    assert_eq!(fault.szamlazz_code.as_deref(), Some("463"), "{fault:?}");
    assert!(fault.message.contains("SZ-30"), "{fault:?}");
    assert!(fault.message.contains("Sztornózó"), "{fault:?}");

    // An unknown invoice on the order's storno: `not_found`, like the agent's.
    h.reset().await;
    number_query("SZ-34")
        .respond_with(not_found())
        .expect(1)
        .mount(&h.mock)
        .await;
    storno_never_sent(&h.mock).await;
    let reply = h
        .call_scoped(
            "acme",
            "E2E-34",
            "storno_invoice",
            &storno_of("SZ-34"),
            "e2e-34-s1",
        )
        .await;
    assert_eq!(reply.status, 404, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "not_found", "{fault:?}");
    assert_eq!(fault.szamlazz_code, None, "{fault:?}");
    assert!(fault.message.contains("SZ-34"), "{fault:?}");
    assert_eq!(fault.order.as_deref(), Some("E2E-34"), "{fault:?}");
    assert_eq!(fault.kind, None, "{fault:?}");
    assert_eq!(
        fault.external_id.as_deref(),
        Some("acct:E2E-34:storno:SZ-34"),
        "{fault:?}"
    );
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account", "verify-storno-SZ-34"],
        "the verify is the only step journaled"
    );
    eprintln!(
        "(xviii-c') faults: query Api → 422 szamlazz_error{{szamlazz_code}}; sixth entry → 400 invalid_input, nothing sent; refused entry → 422; order storno on 7 → 404 not_found with identity: pass"
    );
}
