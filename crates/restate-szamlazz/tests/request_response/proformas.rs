//! Shared proforma lifecycle through buffered real Restate ingress.
use super::*;

pub async fn create(server: &Restate, mock: &wiremock::MockServer) {
    common::create_for("new-proforma")
        .and(wiremock::matchers::body_string_contains(
            "<dijbekero>true</dijbekero>",
        ))
        .respond_with(common::created("D-CREATED", "1000", "1270"))
        .expect(1)
        .mount(mock)
        .await;
    let mut body = request();
    body["options"] = json!({});
    let response = server
        .invoke(
            &Call::object("Szamlazz.Order", "new-proforma", "create_proforma"),
            Some(&body),
            None,
        )
        .await;
    assert_eq!(response.body["outcome"], "issued", "{response:?}");
    assert_eq!(response.body["kind"], "proforma");
    assert_eq!(marker(server, "new-proforma").await["state"], "absent");
}

pub async fn delete(server: &Restate, mock: &wiremock::MockServer) {
    common::external_id_query("rr:delete-proforma:proforma")
        .respond_with(common::Doc::of("D-DELETE", "D", "delete-proforma").response())
        .with_priority(1)
        .mount(mock)
        .await;
    common::number_query("D-DELETE")
        .respond_with(common::Doc::of("D-DELETE", "D", "delete-proforma").response())
        .with_priority(1)
        .mount(mock)
        .await;
    common::delete_of("D-DELETE")
        .respond_with(common::proforma_deleted())
        .expect(1)
        .mount(mock)
        .await;
    let response = server
        .invoke(
            &Call::object("Szamlazz.Order", "delete-proforma", "delete_proforma"),
            Some(&json!({"expected_number":"D-DELETE"})),
            None,
        )
        .await;
    assert_eq!(response.body["outcome"], "deleted", "{response:?}");
    assert_eq!(marker(server, "delete-proforma").await["state"], "absent");
}

pub async fn guards(server: &Restate, mock: &wiremock::MockServer) {
    let entries = [common::CreditRecord::transfer("1270")];
    let mut paid = common::Doc::of("D-PAID", "D", "paid-proforma");
    paid.credit_entries = &entries;
    common::external_id_query("rr:paid-proforma:proforma")
        .respond_with(paid.response())
        .with_priority(1)
        .mount(mock)
        .await;
    common::number_query("D-PAID")
        .respond_with(paid.response())
        .with_priority(1)
        .mount(mock)
        .await;
    common::delete_of("D-PAID")
        .respond_with(common::proforma_deleted())
        .expect(1)
        .mount(mock)
        .await;
    let call = Call::object("Szamlazz.Order", "paid-proforma", "delete_proforma");
    let response = server
        .invoke(&call, Some(&json!({"expected_number":"D-PAID"})), None)
        .await;
    assert_eq!(response.body["outcome"], "conflict", "{response:?}");
    assert_eq!(marker(server, "paid-proforma").await["state"], "absent");
    assert_eq!(
        server
            .invoke(
                &call,
                Some(&json!({"expected_number":"D-PAID","force":true})),
                None
            )
            .await
            .body["outcome"],
        "deleted"
    );
    common::external_id_query("rr:named-delete:proforma")
        .respond_with(common::Doc::of("D-COEXIST", "D", "named-delete").response())
        .with_priority(1)
        .mount(mock)
        .await;
    common::number_query("D-NAMED-DELETE")
        .respond_with(common::Doc::of("D-NAMED-DELETE", "D", "named-delete").response())
        .with_priority(1)
        .mount(mock)
        .await;
    common::delete_of("D-NAMED-DELETE")
        .respond_with(common::proforma_deleted())
        .expect(1)
        .mount(mock)
        .await;
    common::delete_of("D-COEXIST")
        .respond_with(common::proforma_deleted())
        .expect(0)
        .mount(mock)
        .await;
    let response = server
        .invoke(
            &Call::object("Szamlazz.Order", "named-delete", "delete_proforma"),
            Some(&json!({"expected_number":"D-NAMED-DELETE","mode":"named_target"})),
            None,
        )
        .await;
    assert_eq!(response.body["outcome"], "deleted", "{response:?}");
}

pub async fn interruptions(server: &Restate, mock: &wiremock::MockServer, lab: &Arc<Lab>) {
    for scenario in [
        "proforma-visible",
        "proforma-invisible",
        "proforma-guard",
        "delete-absent",
        "delete-still-present",
        "delete-paid",
        "delete-refused",
        "delete-recorded",
    ] {
        let creating = scenario.starts_with("proforma");
        let sends = Arc::new(AtomicUsize::new(0));
        let changed = Arc::new(AtomicBool::new(false));
        let count = sends.clone();
        let change = changed.clone();
        let number = format!("D-{scenario}");
        let query_number = number.clone();
        common::external_id_query(&format!("rr:{scenario}:proforma"))
            .respond_with(move |_: &wiremock::Request| {
                if creating {
                    return if scenario == "proforma-visible" && count.load(Ordering::SeqCst) > 0 {
                        common::Doc::of(&query_number, "D", scenario).response()
                    } else {
                        common::not_found()
                    };
                }
                if scenario == "delete-absent" && change.load(Ordering::SeqCst) {
                    common::not_found()
                } else {
                    common::Doc::of(&query_number, "D", scenario).response()
                }
            })
            .with_priority(1)
            .mount(mock)
            .await;
        let change = changed.clone();
        let query_number = number.clone();
        common::number_query(&number)
            .respond_with(move |_: &wiremock::Request| {
                let mut doc = common::Doc::of(&query_number, "D", scenario);
                let entries = [common::CreditRecord::transfer("1270")];
                if scenario == "delete-paid" && change.load(Ordering::SeqCst) {
                    doc.credit_entries = &entries;
                }
                doc.response()
            })
            .with_priority(1)
            .mount(mock)
            .await;
        if scenario == "proforma-guard" {
            let change = changed.clone();
            common::external_id_query(&format!("rr:{scenario}:final"))
                .respond_with(move |_: &wiremock::Request| {
                    if change.load(Ordering::SeqCst) {
                        common::Doc::of("VS-NEW", "VS", scenario).response()
                    } else {
                        common::not_found()
                    }
                })
                .with_priority(1)
                .mount(mock)
                .await;
        }
        let count = sends.clone();
        let reply_number = number.clone();
        let writer = if creating {
            common::create_for(scenario)
        } else {
            common::delete_of(&number)
        };
        writer
            .respond_with(move |_: &wiremock::Request| {
                let n = count.fetch_add(1, Ordering::SeqCst);
                if creating {
                    common::created(&reply_number, "1000", "1270")
                } else if scenario == "delete-recorded" {
                    wiremock::ResponseTemplate::new(500)
                } else if scenario == "delete-refused" && n > 0 {
                    common::proforma_gone()
                } else {
                    common::proforma_deleted()
                }
            })
            .mount(mock)
            .await;
        let body = if creating {
            let mut body = request();
            body["options"] = json!({});
            body
        } else {
            json!({"expected_number":number})
        };
        let call = Call::object(
            "Szamlazz.Order",
            scenario,
            if creating {
                "create_proforma"
            } else {
                "delete_proforma"
            },
        );
        if scenario != "delete-recorded" {
            lab.change(scenario, |s| s.hold = Some(WriteCheckpoint::Sent));
        }
        let started = server
            .invoke(&call.send(), Some(&body), Some(scenario))
            .await;
        if scenario != "delete-recorded" {
            tokio::time::timeout(Duration::from_secs(30), lab.reached.notified())
                .await
                .expect("held send");
            changed.store(true, Ordering::SeqCst);
            lab.cut.notify_one();
        }
        let unresolved = matches!(
            scenario,
            "proforma-guard"
                | "delete-absent"
                | "delete-paid"
                | "delete-refused"
                | "delete-recorded"
        );
        let expected = if matches!(
            scenario,
            "proforma-invisible" | "delete-still-present" | "delete-refused"
        ) {
            2
        } else {
            1
        };
        if unresolved {
            pause(server, started.invocation_id()).await;
            assert_eq!(sends.load(Ordering::SeqCst), expected, "{scenario}");
            let observed = marker(server, scenario).await;
            assert_eq!(observed["state"], "unresolved");
            server.admin().resume(started.invocation_id()).await;
            pause(server, started.invocation_id()).await;
            assert_eq!(
                sends.load(Ordering::SeqCst),
                expected,
                "recorded uncertainty must not resend"
            );
            server.admin().kill(started.invocation_id()).await;
            server
                .admin()
                .await_status(started.invocation_id(), &["completed"])
                .await;
            assert_eq!(
                server
                    .invoke(&call, Some(&body), Some(&format!("new-{scenario}")))
                    .await
                    .status,
                500
            );
            if !creating {
                let recover = Call::object("Szamlazz.Order", scenario, "recover");
                let mut evidence = json!({"operator":"operator","marker":observed["marker"],"evidence":{"type":"document","number":number}});
                evidence["marker"]["execution_contract"]["request_response_delete_v1"]["force"] =
                    json!(true);
                assert_eq!(
                    server.invoke(&recover, Some(&evidence), None).await.status,
                    400,
                    "force is part of exact marker echo"
                );
                evidence["marker"] = observed["marker"].clone();
                assert_eq!(
                    server.invoke(&recover, Some(&evidence), None).await.status,
                    500,
                    "queries cannot prove deletion"
                );
                evidence["evidence"] = json!({"type":"completed","audit_reference":"INC-DELETE","completion":{"type":"deleted","number":number},"completed_and_cannot_execute_later":true});
                let response = server.invoke(&recover, Some(&evidence), None).await;
                assert_eq!(response.status, 200, "{response:?}");
                assert_eq!(marker(server, scenario).await["state"], "absent");
            }
        } else {
            server
                .admin()
                .await_status(started.invocation_id(), &["completed"])
                .await;
            let response = server.invoke(&call, Some(&body), Some(scenario)).await;
            assert_eq!(
                response.body["outcome"],
                if creating { "issued" } else { "deleted" },
                "{scenario}: {response:?}"
            );
            assert_eq!(sends.load(Ordering::SeqCst), expected);
        }
    }
}
