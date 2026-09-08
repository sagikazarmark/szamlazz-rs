//! The `Szamlazz.Agent` reads: `check_account` on the single-account
//! deployment and under each scope, `query` and `query_taxpayer`.

use serde_json::json;

use restate_szamlazz::contract::TerminalCode;

use crate::harness::Harness;
use crate::harness::accounts::{AGENT_KEY, KEY_B};
use crate::harness::szamlazz::{
    Doc, api_error, not_found, number_query, probe_with_key, taxpayer_known,
    taxpayer_query_with_key, taxpayer_unknown,
};

/// (xii-b) `check_account` on the single-account deployment: unscoped, the
/// probe answers the configured account with `scope: null` and
/// `credentials: ok` after exactly one szamlazz.hu request (the query of the
/// sentinel id, carrying the account's key) with `probe` as its one step
/// after the prologue's; a wrong key is `credentials: rejected` as data, not
/// a fault. `scope` is what the SDK saw: under a scoped call it is the
/// deploy-time signal that the server forwards the scope (protocol v7).
pub(crate) async fn check_account_names_the_account_and_reports_the_credentials(h: &Harness) {
    h.reset().await;
    probe_with_key(AGENT_KEY)
        .respond_with(not_found())
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h.check_account(None).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(
        reply.body,
        json!({
            "scope": null,
            "account": { "id": "acct" },
            "namespace": "acct",
            "credentials": { "state": "ok" },
        })
    );
    assert_eq!(h.requests_seen().await, 1, "one query, nothing else");
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account", "probe"]
    );
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert_eq!(invocation.handler, "check_account");
    assert_eq!(invocation.scope, None);

    // A wrong key: szamlazz.hu's code 3 on the probe is reported, not raised.
    h.reset().await;
    probe_with_key(AGENT_KEY)
        .respond_with(api_error("3", "Sikertelen bejelentkezés."))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h.check_account(None).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["account"]["id"], "acct");
    assert_eq!(
        reply.body["credentials"],
        json!({ "state": "rejected", "code": "3", "message": "Sikertelen bejelentkezés." })
    );
    assert_eq!(h.requests_seen().await, 1, "one query, nothing else");
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.status, "completed", "{invocation:?}");

    // A scoped probe on the single-account deployment: no account to probe,
    // nothing sent; the scope is refused by the `account` step (xii), and
    // the probe reports it the same way.
    h.reset().await;
    let reply = h.check_account(Some("acme-events")).await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    assert_eq!(
        reply.fault().code,
        TerminalCode::UnknownAccount,
        "{}",
        reply.body
    );
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account"]
    );
    assert_eq!(h.requests_seen().await, 0, "nothing reached szamlazz.hu");
    eprintln!(
        "(xii-b) check_account unscoped → the account, credentials ok | rejected as data; scoped → unknown_account: pass"
    );
}

/// (xvii-c) `check_account` under each scope of the multi-account deployment
/// names that scope's account with `credentials: ok` and `scope` as the SDK
/// saw it, and the probe on the wire carries that account's key and nothing
/// else: the deploy-pipeline proof that a scope reaches the worker, resolves
/// to the intended account and its key works. Unscoped it is
/// `unknown_account` with `namespace` and `account` journaled and no
/// szamlazz.hu request.
pub(crate) async fn check_account_under_each_scope_names_its_account(h: &Harness) {
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
        h.requests_seen().await,
        2,
        "one probe per account, each with its own key, nothing else"
    );

    // Unscoped on the multi-account deployment: no account to probe.
    h.reset().await;
    let reply = h.check_account(None).await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, TerminalCode::UnknownAccount, "{fault:?}");
    assert!(fault.message.contains("unscoped"), "{fault:?}");
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account"]
    );
    assert_eq!(h.requests_seen().await, 0, "nothing reached szamlazz.hu");

    eprintln!(
        "(xvii-c) check_account under acme and beta → each its account with its key on the probe, credentials ok; unscoped → unknown_account: pass"
    );
}

/// (xviii-c) `Szamlazz.Agent.query` under a scope answers the projection of
/// whatever it finds: `test` as szamlazz.hu reported it, compared with
/// nothing (the go-live check reads it off a known document here), no
/// `supplier_id`; code 7 is 404 `not_found`.
pub(crate) async fn agent_query_projects_what_it_finds(h: &Harness) {
    let query_of = |number: &str| json!({ "selector": { "invoice_number": number } });

    // A document a live account issued: projected, `test: false` reported
    // as is.
    h.reset().await;
    number_query("SZ-25")
        .respond_with(
            Doc {
                test: false,
                ..Doc::unmanaged("SZ-25", "SZ")
            }
            .response(),
        )
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent_scoped("acme", "query", &query_of("SZ-25"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "SZ-25", "{}", reply.body);
    assert_eq!(reply.body["test"], false, "{}", reply.body);
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account", "query"]
    );

    h.reset().await;
    number_query("SZ-26")
        .respond_with(Doc::new("SZ-26", "SZ", "E2E-26").response())
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent_scoped("acme", "query", &query_of("SZ-26"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "SZ-26", "{}", reply.body);
    assert_eq!(reply.body["document_type"], "SZ", "{}", reply.body);
    assert_eq!(reply.body["order_number"], "E2E-26", "{}", reply.body);
    assert_eq!(reply.body["test"], true, "{}", reply.body);
    assert!(
        reply.body.get("supplier_id").is_none(),
        "the seller record's id is not projected: {}",
        reply.body
    );
    assert_eq!(reply.body["gross_total"], "1270", "{}", reply.body);

    h.reset().await;
    number_query("SZ-27")
        .respond_with(not_found())
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent_scoped("acme", "query", &query_of("SZ-27"))
        .await;
    assert_eq!(reply.status, 404, "{}", reply.body);
    assert_eq!(reply.fault().code, TerminalCode::NotFound, "{}", reply.body);
    eprintln!(
        "(xviii-c) Szamlazz.Agent.query under a scope: the projection with test as reported, no supplier_id; 7 → not_found: pass"
    );
}

/// (xviii-d) `Szamlazz.Agent.query_taxpayer` under a scope asks NAV through
/// that scope's account: the `xmltaxpayer` request on the wire carries that
/// account's key and nothing else reaches szamlazz.hu; the full tax number
/// and its bare stem are one step, `taxpayer-{prefix}`; `valid: false` is
/// data; a malformed tax number is `invalid_input` before the prologue,
/// nothing journaled, nothing sent. The run-wide leak scan (xxi) covers
/// these invocations too.
pub(crate) async fn agent_query_taxpayer_runs_on_the_scoped_account(h: &Harness) {
    let request = |tax_number: &str| json!({ "tax_number": tax_number });

    // `acme` with its key, the full tax number; `beta` with its key, the
    // bare stem: the same prefix, the same step name on both.
    h.reset().await;
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

    let reply = h
        .call_agent_scoped("acme", "query_taxpayer", &request("12345678-2-42"))
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
        reply.body["addresses"][0]["city"], "TESTVAROS",
        "{}",
        reply.body
    );
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account", "taxpayer-12345678"]
    );
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.scope.as_deref(), Some("acme"), "{invocation:?}");
    assert_eq!(invocation.handler, "query_taxpayer");

    let reply = h
        .call_agent_scoped("beta", "query_taxpayer", &request("12345678"))
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
        h.requests_seen().await,
        2,
        "one taxpayer query per account, each with its own key, nothing else"
    );

    // A malformed tax number: refused before the prologue.
    h.reset().await;
    let reply = h
        .call_agent_scoped("acme", "query_taxpayer", &request("12345678-2"))
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, TerminalCode::InvalidInput, "{fault:?}");
    assert!(fault.message.contains("\"12345678-2\""), "{fault:?}");
    assert!(fault.message.contains("12345678-2-42"), "{fault:?}");
    assert!(
        h.runs(reply.invocation_id()).await.is_empty(),
        "nothing journaled before the refusal"
    );
    assert_eq!(h.requests_seen().await, 0, "nothing reached szamlazz.hu");

    eprintln!(
        "(xviii-d) Szamlazz.Agent.query_taxpayer under acme and beta → each asks NAV with its own key, one step taxpayer-{{prefix}} for the full number and the stem, valid: false as data; malformed → invalid_input before the prologue: pass"
    );
}
