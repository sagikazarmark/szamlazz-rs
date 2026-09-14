//! Malformed records and dates through actual Restate ingress, before any run
//! or provider request. Direct Serde/Body coverage lives in `tests/contract_input.rs`.

use restate_e2e_harness::{Call, Restate, ReusePolicy, ServerSpec, launcher_or_skip};
use restate_sdk::prelude::Endpoint;
use restate_szamlazz::contract::{Fault, TerminalCode};
use rust_decimal::dec;
use serde_json::{Value, json};
use wiremock::{MockServer, matchers::body_string_contains};

use crate::harness::accounts::{MutableAccounts, multi_account_services};
use crate::harness::szamlazz::{credit_of, credited};
use crate::harness::{MAIN_SERVER, create_body};

async fn malformed(
    server: &Restate,
    mock: &MockServer,
    accounts: &MutableAccounts,
    call: &Call<'_>,
    body: &Value,
    case: &str,
) {
    let reply = server.invoke(call, Some(body), Some(case)).await;
    assert_eq!(reply.status, 400, "{case}: {}", reply.body);
    // Checks the real Restate error envelope and decodes the JSON fault inside
    // its message; an SDK plain-text 400 cannot satisfy this assertion.
    let fault = reply.fault::<Fault>();
    assert_eq!(fault.code, TerminalCode::InvalidInput, "{case}: {fault:?}");
    assert!(
        fault.message.starts_with("malformed request body: "),
        "{case}: {fault:?}"
    );
    assert!(
        server.admin().runs(reply.invocation_id()).await.is_empty(),
        "{case}: malformed input must stop before the prologue"
    );
    assert_eq!(accounts.resolutions("acme"), 0, "{case}");
    assert_eq!(accounts.fetches("acme"), 0, "{case}");
    assert!(
        mock.received_requests().await.expect("requests").is_empty(),
        "{case}: ZERO provider calls, including reads"
    );
}

fn recovery(endpoint: &str) -> Value {
    json!({
        "operator":"operator-1",
        "marker":{
            "version":1, "token":"inv-1", "owner_invocation":"inv-1",
            "created_at":"2026-09-14T12:00:00Z", "scope":"acme", "order":"INPUT-RECOVERY",
            "namespace":"acct", "external_id":"acct:INPUT-RECOVERY:invoice",
            "account_id":"acme", "endpoint":endpoint, "credential_ref":"acme",
            "operation":{"type":"create", "kind":"invoice", "expected_number":null, "corrected_number":null}
        },
        "evidence":{"type":"completed", "audit_reference":"incident-1",
            "completion":{"type":"issued", "number":"SZ-1"}, "completed_and_cannot_execute_later":true}
    })
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; object-only input at real Restate ingress"]
#[allow(
    clippy::too_many_lines,
    reason = "one malformed-object matrix on an isolated deployment"
)]
async fn e2e_contract_input_positional_objects_never_reach_provider() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let server = launcher
        .launch(&ServerSpec {
            name: "input-objects",
            ..MAIN_SERVER
        })
        .await;
    let mock = MockServer::start().await;
    let (accounts, order, agent) = multi_account_services(&mock.uri()).await;
    server
        .deploy(Endpoint::builder().bind(order).bind(agent).build())
        .await;

    let call = Call::object("Szamlazz.Order", "INPUT-CREATE", "create_invoice").scoped("acme");
    let mut valid = create_body(dec!(100));
    valid["options"]["reissue"] = json!({"expected_number":"SZ-OLD"});
    valid["document"]["buyer"]["postal_address"] = json!({});
    valid["document"]["overrides"] = json!({"exchange_rate":{"bank":"MNB"}});
    valid["document"]["items"][0]["amounts"] = json!({"net":"100", "vat":"27", "gross":"127"});
    valid["document"]["expected_totals"] = json!({"net":"100", "vat":"27", "gross":"127"});
    // Full positional records, not just [] that could fail for missing fields.
    for (index, (path, array)) in [
        ("", json!([valid["document"], valid["options"]])),
        ("/options", json!([null, "auto"])),
        ("/options/reissue", json!(["SZ-OLD"])),
        (
            "/document",
            json!([
                valid["document"]["buyer"],
                valid["document"]["items"],
                "2026-09-14",
                "2026-09-21",
                "transfer",
                false,
                null,
                null,
                {},
                null
            ]),
        ),
        (
            "/document/buyer",
            json!([
                "A", "1", "B", "C", null, null, null, null, null, null, null, null, null, null
            ]),
        ),
        (
            "/document/buyer/postal_address",
            json!([null, null, null, null, null]),
        ),
        (
            "/document/items/0",
            json!(["x", "1", "db", "100", "27", null, null, null]),
        ),
        ("/document/items/0/amounts", json!(["100", "27", "127"])),
        ("/document/expected_totals", json!(["100", "27", "127"])),
        (
            "/document/overrides",
            json!([null, null, null, null, null, null, null]),
        ),
        ("/document/overrides/exchange_rate", json!(["MNB", null])),
    ]
    .into_iter()
    .enumerate()
    {
        let mut body = valid.clone();
        *body.pointer_mut(path).expect("input path") = array;
        malformed(
            &server,
            &mock,
            &accounts,
            &call,
            &body,
            &format!("create-{index}"),
        )
        .await;
    }

    let credit = Call::service("Szamlazz.Agent", "set_credit_entries").scoped("acme");
    for (index, body) in [
        json!(["SZ-INPUT", [{"date":"2026-09-14", "title":"cash", "amount":"1"}], false]),
        json!({"invoice_number":"SZ-INPUT", "entries":[["2026-09-14", "cash", "1", null]]}),
    ]
    .into_iter()
    .enumerate()
    {
        malformed(
            &server,
            &mock,
            &accounts,
            &credit,
            &body,
            &format!("credit-{index}"),
        )
        .await;
    }

    let recover = Call::object("Szamlazz.Order", "INPUT-RECOVERY", "recover").scoped("acme");
    let valid = recovery(&mock.uri());
    for (index, (path, array)) in [
        (
            "",
            json!([valid["operator"], valid["marker"], valid["evidence"]]),
        ),
        (
            "/marker",
            json!([
                1,
                "inv-1",
                "inv-1",
                "2026-09-14T12:00:00Z",
                "acme",
                "INPUT-RECOVERY",
                "acct",
                "acct:INPUT-RECOVERY:invoice",
                "acme",
                mock.uri(),
                "acme",
                valid["marker"]["operation"]
            ]),
        ),
        (
            "/marker/operation",
            json!(["create", "invoice", null, null]),
        ),
        ("/evidence", json!(["document", "SZ-1"])),
        ("/evidence", json!(["not_executed", "incident-1", true])),
        (
            "/evidence",
            json!([
                "completed",
                "incident-1",
                valid["evidence"]["completion"],
                true
            ]),
        ),
        ("/evidence/completion", json!(["issued", "SZ-1"])),
    ]
    .into_iter()
    .enumerate()
    {
        let mut body = valid.clone();
        *body.pointer_mut(path).expect("recovery path") = array;
        malformed(
            &server,
            &mock,
            &accounts,
            &recover,
            &body,
            &format!("recovery-{index}"),
        )
        .await;
    }
    server.finish().await;
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; canonical dates at real Restate ingress"]
async fn e2e_contract_input_noncanonical_dates_never_reach_provider() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let server = launcher
        .launch(&ServerSpec {
            name: "input-dates",
            ..MAIN_SERVER
        })
        .await;
    let mock = MockServer::start().await;
    let (accounts, order, agent) = multi_account_services(&mock.uri()).await;
    server
        .deploy(Endpoint::builder().bind(order).bind(agent).build())
        .await;
    let create = Call::object("Szamlazz.Order", "INPUT-DATES", "create_invoice").scoped("acme");
    let credit = Call::service("Szamlazz.Agent", "set_credit_entries").scoped("acme");
    for (index, date) in [
        "0000-01-01",
        "10000-01-01",
        "-0001-01-01",
        "+002026-09-14",
        "20260914",
        "2026-9-14",
        "2026-09-14T12:34:56",
        "2026-09-14T12:34:56Z",
        "2026-09-14[Europe/Budapest]",
        "2026-09-14[u-ca=iso8601]",
        " 2026-09-14",
        "2026-09-14 ",
        "2026-02-29",
    ]
    .into_iter()
    .enumerate()
    {
        for field in ["fulfillment_date", "due_date", "issue_date"] {
            let mut body = create_body(dec!(100));
            body["document"][field] = json!(date);
            malformed(
                &server,
                &mock,
                &accounts,
                &create,
                &body,
                &format!("date-{index}-{field}"),
            )
            .await;
        }
        let body = json!({"invoice_number":"SZ-DATE", "entries":[{"date":date, "title":"transfer", "amount":"1"}]});
        malformed(
            &server,
            &mock,
            &accounts,
            &credit,
            &body,
            &format!("date-{index}-credit"),
        )
        .await;
    }

    // After the ZERO-call assertions, prove the same deployed input path accepts
    // a canonical date and preserves an exact numeric token beyond f64 precision.
    credit_of("SZ-DATE")
        .and(body_string_contains("<datum>2026-09-14</datum>"))
        .and(body_string_contains("7922816251426433759354395033.5"))
        .respond_with(credited("SZ-DATE", "127", "0"))
        .expect(1)
        .mount(&mock)
        .await;
    let body: Value = serde_json::from_str(r#"{"invoice_number":"SZ-DATE","entries":[{"date":"2026-09-14","title":"transfer","amount":7922816251426433759354395033.5}]}"#).expect("exact JSON token");
    let reply = server
        .invoke(&credit, Some(&body), Some("canonical-date"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "SZ-DATE");
    assert_eq!(
        server.admin().runs(reply.invocation_id()).await,
        ["namespace", "account", "set-credit-entries-SZ-DATE"]
    );
    assert_eq!(accounts.resolutions("acme"), 1);
    assert_eq!(accounts.fetches("acme"), 1);
    assert_eq!(mock.received_requests().await.expect("requests").len(), 1);
    mock.verify().await;
    server.finish().await;
}
