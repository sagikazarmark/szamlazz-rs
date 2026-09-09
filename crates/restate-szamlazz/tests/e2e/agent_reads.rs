//! The `Szamlazz.Agent` reads under the multi-account deployment: the scope
//! selecting the account for every read. `check_account` under each scope
//! (the deploy-pipeline proof), `query` under a scope (the projection as the
//! go-live check reads it), `query_taxpayer` under each scope (NAV asked with
//! that scope's key). The reads' decisions (`credentials: rejected` as data,
//! 7 as `not_found`, a NAV code passed through, the tax number's two forms)
//! are unit tests of `service::agent` and `tests/gateway.rs`.

use serde_json::json;

use restate_szamlazz::contract::TerminalCode;

use crate::harness::Harness;
use crate::harness::accounts::{AGENT_KEY, KEY_B};
use crate::harness::szamlazz::{
    Doc, not_found, number_query, probe_with_key, taxpayer_known, taxpayer_query_with_key,
    taxpayer_unknown,
};

/// The scope on the ingress path is the one channel that selects an account,
/// and every `Szamlazz.Agent` read runs on the account it selects.
/// `check_account` under `acme` and under `beta` names that scope's account
/// with `credentials: ok` and `scope` as the SDK saw it, the probe on the wire
/// carrying that account's key and nothing else (the proof a deploy pipeline
/// reads: the scope reaches the worker, resolves to the intended account and
/// its key works); unscoped it is `unknown_account` with `namespace` and
/// `account` journaled and no request. `query` under `acme` answers the
/// projection of what it finds, `test` as szamlazz.hu reported it and no
/// `supplier_id` (the go-live check reads `test` and the seller block off a
/// known document this way). `query_taxpayer` under `acme` with the full tax
/// number and under `beta` with the bare stem asks NAV with each account's own
/// key, both journaling the one `taxpayer-{prefix}` step; `valid: false` is
/// data.
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the three reads under each scope"
)]
pub(crate) async fn the_scope_selects_the_account_for_every_agent_read(h: &Harness) {
    h.reset().await;
    probe_with_key(AGENT_KEY)
        .respond_with(not_found())
        .expect(1)
        .mount(&h.mock)
        .await;
    probe_with_key(KEY_B)
        .respond_with(not_found())
        .expect(1)
        .mount(&h.mock)
        .await;
    number_query("SZ-26")
        .respond_with(
            Doc {
                test: Some(false),
                ..Doc::of("SZ-26", "SZ", "E2E-26")
            }
            .response(),
        )
        .expect(1)
        .mount(&h.mock)
        .await;
    taxpayer_query_with_key("12345678", AGENT_KEY)
        .respond_with(taxpayer_known())
        .expect(1)
        .mount(&h.mock)
        .await;
    taxpayer_query_with_key("12345678", KEY_B)
        .respond_with(taxpayer_unknown())
        .expect(1)
        .mount(&h.mock)
        .await;

    // `check_account` under each scope, then unscoped.
    for (scope, id) in [("acme", "acme"), ("beta", "beta")] {
        let reply = h.check_account(Some(scope)).await;
        assert_eq!(reply.status, 200, "{scope}: {}", reply.body);
        assert_eq!(
            reply.body,
            json!({
                "scope": scope,
                "account": { "id": id },
                "namespace": "acct",
                "credentials": { "state": "ok" },
            }),
            "{scope}"
        );
        assert_eq!(
            h.runs(reply.invocation_id()).await,
            ["namespace", "account", "probe"],
            "{scope}"
        );
        let invocation = h.invocation(reply.invocation_id()).await;
        assert_eq!(invocation.scope.as_deref(), Some(scope), "{invocation:?}");
        assert_eq!(invocation.handler, "check_account");
    }
    assert_eq!(
        h.requests_mentioning("acct:check-account").await.len(),
        2,
        "one probe per account, each with its own key"
    );
    let reply = h.check_account(None).await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, TerminalCode::UnknownAccount, "{fault:?}");
    assert!(fault.message.contains("unscoped"), "{fault:?}");
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account"]
    );
    assert_eq!(
        h.requests_mentioning("acct:check-account").await.len(),
        2,
        "nothing reached szamlazz.hu unscoped"
    );

    // `query` under a scope: the projection, `test` as reported.
    let reply = h
        .call_agent_scoped(
            "acme",
            "query",
            &json!({ "selector": { "invoice_number": "SZ-26" } }),
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "SZ-26", "{}", reply.body);
    assert_eq!(reply.body["document_type"], "SZ", "{}", reply.body);
    assert_eq!(reply.body["order_number"], "E2E-26", "{}", reply.body);
    assert_eq!(reply.body["test"], false, "{}", reply.body);
    assert!(
        reply.body.get("supplier_id").is_none(),
        "the seller record's id is not projected: {}",
        reply.body
    );
    assert_eq!(reply.body["gross_total"], "1270", "{}", reply.body);
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account", "query"]
    );
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.scope.as_deref(), Some("acme"), "{invocation:?}");

    // `query_taxpayer` under each scope: the full number and the stem are
    // one step, each asked with its account's key.
    let reply = h
        .call_agent_scoped(
            "acme",
            "query_taxpayer",
            &json!({ "tax_number": "12345678-2-42" }),
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["valid"], true, "{}", reply.body);
    assert_eq!(
        reply.body["name"], "SYNTHETIC SOFTWARE KFT.",
        "{}",
        reply.body
    );
    assert_eq!(reply.body["tax_number"], "12345678", "{}", reply.body);
    assert_eq!(reply.body["vat_code"], "2", "{}", reply.body);
    assert_eq!(reply.body["addresses"][0]["kind"], "HQ", "{}", reply.body);
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account", "taxpayer-12345678"]
    );
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.scope.as_deref(), Some("acme"), "{invocation:?}");
    assert_eq!(invocation.handler, "query_taxpayer");

    let reply = h
        .call_agent_scoped(
            "beta",
            "query_taxpayer",
            &json!({ "tax_number": "12345678" }),
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(
        reply.body,
        json!({ "valid": false, "name": null, "tax_number": null, "vat_code": null, "addresses": [] }),
        "valid: false is data"
    );
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account", "taxpayer-12345678"]
    );
    assert_eq!(
        h.requests_mentioning("<torzsszam>12345678</torzsszam>")
            .await
            .len(),
        2,
        "one taxpayer query per account, each with its own key"
    );
}
