//! Prepayment/final issuance through the shared Order ingress.
use super::*;

pub async fn issue(server: &Restate, mock: &wiremock::MockServer) {
    common::external_id_query("rr:chain:proforma")
        .respond_with(common::Doc::of("D-CHAIN", "D", "chain").response())
        .with_priority(1)
        .mount(mock)
        .await;
    common::number_query("D-CHAIN")
        .respond_with(common::Doc::of("D-CHAIN", "D", "chain").response())
        .with_priority(1)
        .mount(mock)
        .await;
    common::create_for("chain")
        .and(wiremock::matchers::body_string_contains(
            "<elolegszamla>true</elolegszamla>",
        ))
        .and(wiremock::matchers::body_string_contains(
            "<dijbekeroSzamlaszam>D-CHAIN</dijbekeroSzamlaszam>",
        ))
        .respond_with(common::created("ES-CHAIN", "1000", "1270"))
        .expect(1)
        .mount(mock)
        .await;
    let mut body = request();
    body["options"] = json!({"proforma":"auto"});
    let response = server
        .invoke(
            &Call::object("Szamlazz.Order", "chain", "create_prepayment"),
            Some(&body),
            None,
        )
        .await;
    assert_eq!(response.body["outcome"], "issued", "{response:?}");
    assert_eq!(response.body["kind"], "prepayment");
}

pub async fn final_invoice(server: &Restate, mock: &wiremock::MockServer) {
    common::external_id_query("rr:final-chain:prepayment")
        .respond_with(common::Doc::of("ES-FINAL", "ES", "final-chain").response())
        .with_priority(1)
        .mount(mock)
        .await;
    common::number_query("ES-FINAL")
        .respond_with(common::Doc::of("ES-FINAL", "ES", "final-chain").response())
        .with_priority(1)
        .mount(mock)
        .await;
    common::order_query("final-chain")
        .respond_with(common::Doc::of("ES-FINAL", "ES", "final-chain").response())
        .with_priority(1)
        .mount(mock)
        .await;
    common::create_for("final-chain")
        .and(wiremock::matchers::body_string_contains(
            "<elolegSzamlaszam>ES-FINAL</elolegSzamlaszam>",
        ))
        .and(wiremock::matchers::body_string_contains(
            "<vegszamla>true</vegszamla>",
        ))
        .respond_with(common::created("VS-CHAIN", "500", "635"))
        .expect(1)
        .mount(mock)
        .await;
    let mut body = request();
    body["options"] = json!({});
    body["document"]["items"][0]["unit_price"] = json!("1500");
    body["document"]["items"].as_array_mut().expect("items").push(json!({"name":"Prepayment deduction","quantity":"1","unit":"db","unit_price":"-1000","vat_rate":"27"}));
    let response = server
        .invoke(
            &Call::object("Szamlazz.Order", "final-chain", "create_final"),
            Some(&body),
            None,
        )
        .await;
    assert_eq!(response.body["outcome"], "issued", "{response:?}");
    assert_eq!(response.body["kind"], "final");
    common::external_id_query("rr:final-chain:proforma")
        .respond_with(common::Doc::of("D-UNEXPECTED", "D", "final-chain").response())
        .with_priority(1)
        .mount(mock)
        .await;
    let response = server
        .invoke(
            &Call::object("Szamlazz.Order", "final-chain", "create_final"),
            Some(&body),
            None,
        )
        .await;
    assert_eq!(
        response.body["conflict_reason"], "proforma_live",
        "{response:?}"
    );
    assert_eq!(marker(server, "final-chain").await["state"], "absent");
}

pub async fn interrupted(server: &Restate, mock: &wiremock::MockServer, lab: &Arc<Lab>) {
    for scenario in [
        "final-visible",
        "final-reversed",
        "final-replaced",
        "final-missing",
        "final-recorded",
        "prepay-visible",
        "prepay-recorded",
        "final-resend",
        "prepay-resend",
        "final-reissue-resend",
        "prepay-reissue-resend",
        "final-number-reversed",
        "final-number-missing",
        "final-number-collision",
    ] {
        let final_invoice = scenario.starts_with("final");
        let recorded = scenario.ends_with("recorded");
        let resend = scenario.ends_with("resend");
        let reissue = scenario.contains("reissue");
        let sent = Arc::new(AtomicUsize::new(0));
        let changed = Arc::new(AtomicBool::new(false));
        let visible = Arc::new(AtomicBool::new(false));
        let kind = if final_invoice { "final" } else { "prepayment" };
        let prerequisite = if final_invoice {
            "prepayment"
        } else {
            "proforma"
        };
        let number = format!("REF-{scenario}");
        let seen = visible.clone();
        common::external_id_query(&format!("rr:{scenario}:{kind}"))
            .respond_with(move |_: &wiremock::Request| {
                if seen.load(Ordering::SeqCst) {
                    common::Doc::of(
                        "CHAIN-ISSUED",
                        if final_invoice { "VS" } else { "ES" },
                        scenario,
                    )
                    .response()
                } else {
                    if reissue {
                        let mut old = common::Doc::of(
                            "CHAIN-OLD",
                            if final_invoice { "VS" } else { "ES" },
                            scenario,
                        );
                        old.reversed = true;
                        return old.response();
                    }
                    common::not_found()
                }
            })
            .with_priority(1)
            .mount(mock)
            .await;
        let change = changed.clone();
        let pinned = number.clone();
        common::external_id_query(&format!("rr:{scenario}:{prerequisite}"))
            .respond_with(move |_: &wiremock::Request| {
                if !resend
                    && change.load(Ordering::SeqCst)
                    && (scenario == "final-missing" || !final_invoice)
                {
                    return common::not_found();
                }
                let mut doc = common::Doc::of(
                    if change.load(Ordering::SeqCst) && scenario == "final-replaced" {
                        "NEW-REFERENCE"
                    } else {
                        &pinned
                    },
                    if final_invoice { "ES" } else { "D" },
                    scenario,
                );
                doc.reversed = !resend
                    && !scenario.contains("number-")
                    && scenario != "final-replaced"
                    && change.load(Ordering::SeqCst);
                doc.response()
            })
            .with_priority(1)
            .mount(mock)
            .await;
        let change = changed.clone();
        let pinned = number.clone();
        common::number_query(&number)
            .respond_with(move |_: &wiremock::Request| {
                if scenario == "final-number-missing" && change.load(Ordering::SeqCst) {
                    return common::not_found();
                }
                let mut doc = common::Doc::of(
                    &pinned,
                    if final_invoice { "ES" } else { "D" },
                    if scenario == "final-number-collision" && change.load(Ordering::SeqCst) {
                        "ANOTHER"
                    } else {
                        scenario
                    },
                );
                doc.reversed = !resend
                    && scenario != "final-replaced"
                    && scenario != "final-number-collision"
                    && change.load(Ordering::SeqCst);
                doc.response()
            })
            .with_priority(1)
            .mount(mock)
            .await;
        let count = sent.clone();
        let pinned = number.clone();
        common::create_for(scenario)
            .respond_with(move |r: &wiremock::Request| {
                let element = if final_invoice {
                    "elolegSzamlaszam"
                } else {
                    "dijbekeroSzamlaszam"
                };
                assert!(
                    String::from_utf8_lossy(&r.body)
                        .contains(&format!("<{element}>{pinned}</{element}>"))
                );
                count.fetch_add(1, Ordering::SeqCst);
                if recorded {
                    wiremock::ResponseTemplate::new(500)
                } else {
                    common::created("CHAIN-ISSUED", "1000", "1270")
                }
            })
            .mount(mock)
            .await;
        let mut body = request();
        body["options"] = json!({});
        if reissue {
            body["options"]["reissue"] = json!({"expected_number":"CHAIN-OLD"});
        }
        let call = Call::object(
            "Szamlazz.Order",
            scenario,
            if final_invoice {
                "create_final"
            } else {
                "create_prepayment"
            },
        );
        if !recorded {
            lab.change(scenario, |s| s.hold = Some(WriteCheckpoint::Sent));
        }
        let started = server
            .invoke(&call.send(), Some(&body), Some(scenario))
            .await;
        if recorded {
            pause(server, started.invocation_id()).await;
        } else {
            tokio::time::timeout(Duration::from_secs(30), lab.reached.notified())
                .await
                .expect("chain send held");
        }
        changed.store(true, Ordering::SeqCst);
        let immediately_visible = scenario.ends_with("visible");
        visible.store(immediately_visible, Ordering::SeqCst);
        if !recorded {
            lab.cut.notify_one();
        }
        if !immediately_visible && !resend {
            pause(server, started.invocation_id()).await;
            assert_eq!(sent.load(Ordering::SeqCst), 1, "{scenario}");
            let observed = marker(server, scenario).await;
            assert_eq!(
                observed["marker"][if final_invoice {
                    "prepayment_number"
                } else {
                    "proforma_number"
                }],
                number
            );
            server.admin().resume(started.invocation_id()).await;
            pause(server, started.invocation_id()).await;
            assert_eq!(
                sent.load(Ordering::SeqCst),
                1,
                "recorded uncertainty must remain read-only"
            );
            visible.store(true, Ordering::SeqCst);
            server.admin().resume(started.invocation_id()).await;
        }
        server
            .admin()
            .await_status(started.invocation_id(), &["completed"])
            .await;
        let response = server.invoke(&call, Some(&body), Some(scenario)).await;
        assert_eq!(
            response.body["outcome"],
            if immediately_visible || resend {
                "issued"
            } else {
                "reconciled"
            },
            "{scenario}: {response:?}"
        );
        assert_eq!(sent.load(Ordering::SeqCst), if resend { 2 } else { 1 });
        assert_eq!(marker(server, scenario).await["state"], "absent");
    }
}

pub async fn reissue_and_recover(server: &Restate, mock: &wiremock::MockServer) {
    for (key, kind, handler) in [
        ("prepay-reissue", "ES", "create_prepayment"),
        ("final-reissue", "VS", "create_final"),
    ] {
        let slot = if kind == "ES" { "prepayment" } else { "final" };
        let mut old = common::Doc::of("CHAIN-OLD", kind, key);
        old.reversed = true;
        common::external_id_query(&format!("rr:{key}:{slot}"))
            .respond_with(old.response())
            .with_priority(1)
            .mount(mock)
            .await;
        if kind == "VS" {
            let reference = format!("ES-{key}");
            common::external_id_query(&format!("rr:{key}:prepayment"))
                .respond_with(common::Doc::of(&reference, "ES", key).response())
                .with_priority(1)
                .mount(mock)
                .await;
            common::number_query(&reference)
                .respond_with(common::Doc::of(&reference, "ES", key).response())
                .with_priority(1)
                .mount(mock)
                .await;
        }
        // Old-number acknowledgement never establishes a replacement.
        common::create_for(key)
            .respond_with(common::created("CHAIN-OLD", "1000", "1270"))
            .expect(1)
            .mount(mock)
            .await;
        let mut body = request();
        body["options"] = json!({"reissue":{"expected_number":"CHAIN-OLD"}});
        let call = Call::object("Szamlazz.Order", key, handler);
        let started = server.invoke(&call.send(), Some(&body), Some(key)).await;
        pause(server, started.invocation_id()).await;
        server.admin().kill(started.invocation_id()).await;
        server
            .admin()
            .await_status(started.invocation_id(), &["completed"])
            .await;
        let observed = marker(server, key).await;
        let recover = Call::object("Szamlazz.Order", key, "recover");
        let mut evidence = json!({"operator":"operator","marker":observed["marker"],"evidence":{"type":"completed","audit_reference":"INC-CHAIN","completion":{"type":"issued","number":"CHAIN-OLD"},"completed_and_cannot_execute_later":true}});
        assert_eq!(
            server.invoke(&recover, Some(&evidence), None).await.status,
            400,
            "old number is not positive recovery"
        );
        evidence["evidence"]["completion"]["number"] = json!("CHAIN-NEW");
        evidence["marker"]["prepayment_number"] = json!("substituted");
        assert_eq!(
            server.invoke(&recover, Some(&evidence), None).await.status,
            400,
            "pinned reference is exact echo"
        );
        evidence["marker"] = observed["marker"].clone();
        let result = server.invoke(&recover, Some(&evidence), None).await;
        assert_eq!(result.status, 200, "{result:?}");
        assert_eq!(marker(server, key).await["state"], "absent");
    }
}
