//! Exclusive Order prerequisites survive outages under the invocation policy;
//! shared observations and Agent reads retain their bounded completion policy.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use restate_e2e_harness::{Call, ReusePolicy, ServerSpec, launcher_or_skip, run_result};
use restate_sdk::prelude::Endpoint;
use restate_sdk::service::IntoServiceDefinition as _;
use restate_szamlazz::config::WorkerConfig;
use restate_szamlazz::contract::{Fault, TerminalCode};
use rust_decimal::dec;
use serde_json::json;
use wiremock::MockServer;

use crate::common::{
    api_error, create_for, created, external_id_query, not_found, number_query, order_query,
};
use crate::harness::accounts::multi_account_services_with_config;
use crate::harness::{MAIN_SERVER, create_body};

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; retained prerequisites across dependency outages"]
#[allow(
    clippy::too_many_lines,
    reason = "one real invocation journey per dependency, followed by bounded read controls"
)]
async fn e2e_retained_prerequisites_pause_and_resume_the_same_invocation() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "retained-prerequisites",
            ..MAIN_SERVER
        })
        .await;
    let mock = MockServer::start().await;
    // These old run thresholds would end each prerequisite at its first failure.
    // The exclusive invocation instead executes three times, then pauses.
    let config: WorkerConfig = serde_json::from_value(json!({
        "namespace": "acct",
        "read": {"max_attempts": 1, "max_duration": "1s"},
        "resolve": {"max_attempts": 1, "max_duration": "1s"}
    }))
    .expect("config");
    let (accounts, order, agent) =
        multi_account_services_with_config(&mock.uri(), config.validate().expect("valid policies"))
            .await;
    let options = restate_sdk::endpoint::ServiceOptions::default().handler(
        "create_invoice",
        restate_sdk::endpoint::HandlerOptions::default()
            .retry_policy_initial_interval(Duration::from_secs(1))
            .retry_policy_max_interval(Duration::from_secs(1))
            .retry_policy_max_attempts(3)
            .retry_policy_pause_on_max_attempts(),
    );
    restate
        .deploy(
            Endpoint::builder()
                .bind(order.into_service_definition().options(options))
                .bind(agent)
                .build(),
        )
        .await;

    for (case, step) in [
        ("resolver", "account"),
        ("credentials", "lookup-invoice"),
        ("maintenance-1", "lookup-prepayment"),
        ("maintenance-55", "lookup-prepayment"),
    ] {
        let key = format!("RETAIN-{case}");
        let number = format!("SZ-{case}");
        let outage = Arc::new(AtomicBool::new(true));
        for kind in ["invoice", "prepayment", "final", "proforma"] {
            let down = outage.clone();
            external_id_query(&format!("acct:{key}:{kind}"))
                .respond_with(move |_: &wiremock::Request| {
                    if kind == "prepayment"
                        && case.starts_with("maintenance")
                        && down.load(Ordering::SeqCst)
                    {
                        api_error(
                            if case == "maintenance-1" { "1" } else { "55" },
                            "private-maintenance-source",
                        )
                    } else {
                        not_found()
                    }
                })
                .mount(&mock)
                .await;
        }
        order_query(&key)
            .respond_with(not_found())
            .mount(&mock)
            .await;
        create_for(&key)
            .respond_with(created(&number, "1000", "1270"))
            .expect(1)
            .mount(&mock)
            .await;
        let resolutions = accounts.resolutions("beta");
        let fetches = accounts.fetches("beta");
        if case == "resolver" {
            accounts.fail_next_resolutions("beta", u32::MAX);
        }
        if case == "credentials" {
            accounts.set_unavailable("beta", true);
        }
        let call = Call::object("Szamlazz.Order", &key, "create_invoice").scoped("beta");
        let body = create_body(dec!(1000));
        let owner = restate.invoke(&call.send(), Some(&body), Some(&key)).await;
        assert_eq!(owner.status, 202, "{}", owner.body);
        assert_eq!(
            restate
                .admin()
                .await_status(owner.invocation_id(), &["paused", "completed"])
                .await,
            "paused",
            "{case}: outage must retain this invocation"
        );
        let paused = restate.admin().invocation(owner.invocation_id()).await;
        assert!(paused.completion_failure.is_none(), "{case}: {paused:?}");
        let attached = restate.invoke(&call.send(), Some(&body), Some(&key)).await;
        assert_eq!(
            attached.invocation_id(),
            owner.invocation_id(),
            "paused request retains its retry identity"
        );
        let journal = restate.admin().journal(owner.invocation_id()).await;
        assert!(
            run_result(&journal, step).is_none(),
            "{case}: failed prerequisite is unfinished"
        );
        let requests = mock.received_requests().await.expect("requests");
        assert!(
            requests
                .iter()
                .all(|r| !String::from_utf8_lossy(&r.body)
                    .contains("name=\"action-xmlagentxmlfile\"")),
            "{case}: prerequisite outage cannot send a create"
        );
        match case {
            "resolver" => assert!(accounts.resolutions("beta") - resolutions >= 3),
            "credentials" => assert!(
                accounts.fetches("beta") - fetches >= 9,
                "three invocation executions each exhaust the three-fetch initialization threshold"
            ),
            _ => {
                assert!(run_result(&journal, "lookup-invoice").is_some());
                assert!(
                    requests
                        .iter()
                        .filter(|r| String::from_utf8_lossy(&r.body).contains(":prepayment</"))
                        .count()
                        >= 3
                );
            }
        }
        for entry in &journal {
            for private in [
                "scripted resolver outage",
                "secret-store-source-sentinel",
                "private-maintenance-source",
            ] {
                assert!(
                    !entry.raw_contains(private),
                    "{case}: source message entered journal"
                );
                assert!(
                    !format!("{paused:?}").contains(private),
                    "{case}: source message entered the retry diagnostic"
                );
            }
        }
        accounts.fail_next_resolutions("beta", 0);
        accounts.set_unavailable("beta", false);
        outage.store(false, Ordering::SeqCst);
        restate.admin().resume(owner.invocation_id()).await;
        assert_eq!(
            restate
                .admin()
                .await_status(owner.invocation_id(), &["completed", "paused"])
                .await,
            "completed"
        );
        let reply = restate.invoke(&call, Some(&body), Some(&key)).await;
        assert_eq!(reply.invocation_id(), owner.invocation_id());
        assert_eq!(reply.status, 200, "{case}: {}", reply.body);
        assert_eq!(reply.body["outcome"], "issued");
        assert_eq!(reply.body["invoice_number"], number);
        let before = mock.received_requests().await.expect("requests").len();
        let retained = restate.invoke(&call, Some(&body), Some(&key)).await;
        assert_eq!(retained.invocation_id(), owner.invocation_id());
        assert_eq!(retained.body, reply.body);
        assert_eq!(
            mock.received_requests().await.expect("requests").len(),
            before
        );
        if case != "resolver" {
            assert_eq!(
                accounts.resolutions("beta") - resolutions,
                1,
                "account replayed"
            );
        }
        mock.verify().await;
        mock.reset().await;
    }

    // Maintenance is retryable, but bounded Agent queries still complete with
    // unavailable; restoration cannot alter the completion retained by its key.
    for code in ["1", "55"] {
        let key = format!("bounded-{code}");
        number_query(&key)
            .respond_with(api_error(code, "private-maintenance-source"))
            .expect(1)
            .mount(&mock)
            .await;
        let call = Call::service("Szamlazz.Agent", "query").scoped("beta");
        let body = json!({"selector":{"invoice_number":key}});
        let reply = restate.invoke(&call, Some(&body), Some(&key)).await;
        assert_eq!(reply.status, 503, "{}", reply.body);
        assert_eq!(reply.fault::<Fault>().code, TerminalCode::Unavailable);
        assert_eq!(
            restate
                .admin()
                .invocation(reply.invocation_id())
                .await
                .status,
            "completed"
        );
        mock.verify().await;
        mock.reset().await;
        let retained = restate.invoke(&call, Some(&body), Some(&key)).await;
        assert_eq!(retained.invocation_id(), reply.invocation_id());
        assert_eq!(retained.body, reply.body);
        assert!(mock.received_requests().await.expect("requests").is_empty());
    }
    external_id_query("acct:BOUNDED-GET:proforma")
        .respond_with(api_error("55", "maintenance"))
        .expect(1)
        .mount(&mock)
        .await;
    let get = restate
        .invoke(
            &Call::object("Szamlazz.Order", "BOUNDED-GET", "get").scoped("beta"),
            None,
            None,
        )
        .await;
    assert_eq!(get.status, 503, "{}", get.body);
    assert_eq!(get.fault::<Fault>().code, TerminalCode::Unavailable);
    assert_eq!(
        restate.admin().invocation(get.invocation_id()).await.status,
        "completed"
    );
    assert!(
        restate
            .admin()
            .sql_or_panic("SELECT * FROM state WHERE service_name = 'Szamlazz.Order'")
            .await
            .is_empty()
    );
    mock.verify().await;
    restate.finish().await;
}
