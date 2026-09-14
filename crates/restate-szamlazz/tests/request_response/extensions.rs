//! Reissue/conversion/recovery through the same registered Order.
use super::*;

pub async fn reissue(server: &Restate, mock: &wiremock::MockServer) {
    let mut old = common::Doc::of("OLD-1", "SZ", "reissue");
    old.reversed = true;
    common::external_id_query("rr:reissue:invoice")
        .respond_with(old.response())
        .with_priority(1)
        .mount(mock)
        .await;
    common::create_for("reissue")
        .respond_with(common::created("NEW-1", "1000", "1270"))
        .expect(1)
        .mount(mock)
        .await;
    let mut body = request();
    body["options"]["reissue"] = json!({"expected_number":"OLD-1"});
    let response = server
        .invoke(
            &Call::object("Szamlazz.Order", "reissue", "create_invoice"),
            Some(&body),
            None,
        )
        .await;
    assert_eq!(response.body["outcome"], "issued", "{response:?}");
    assert_eq!(response.body["invoice_number"], "NEW-1");
    assert_eq!(marker(server, "reissue").await["state"], "absent");
    body["options"]["reissue"] = json!({"expected_number":"ANOTHER"});
    let response = server
        .invoke(
            &Call::object("Szamlazz.Order", "reissue", "create_invoice"),
            Some(&body),
            None,
        )
        .await;
    assert_eq!(response.body["conflict_reason"], "target_changed");
}

pub async fn conversion(server: &Restate, mock: &wiremock::MockServer) {
    for (key, number, option) in [
        ("convert-auto", "D-AUTO", json!("auto")),
        ("convert-named", "D-NAMED", json!({"number":"D-NAMED"})),
    ] {
        common::external_id_query(&format!("rr:{key}:proforma"))
            .respond_with(common::Doc::of(number, "D", key).response())
            .with_priority(1)
            .mount(mock)
            .await;
        common::number_query(number)
            .respond_with(common::Doc::of(number, "D", key).response())
            .with_priority(1)
            .mount(mock)
            .await;
        common::create_for(key)
            .and(wiremock::matchers::body_string_contains(format!(
                "<dijbekeroSzamlaszam>{number}</dijbekeroSzamlaszam>"
            )))
            .respond_with(common::created("CONVERTED-1", "1000", "1270"))
            .expect(1)
            .mount(mock)
            .await;
        let mut body = request();
        body["options"]["proforma"] = option;
        let response = server
            .invoke(
                &Call::object("Szamlazz.Order", key, "create_invoice"),
                Some(&body),
                None,
            )
            .await;
        assert_eq!(response.body["outcome"], "issued", "{key}: {response:?}");
        assert_eq!(marker(server, key).await["state"], "absent");
    }
}

pub async fn recovery(server: &Restate, mock: &wiremock::MockServer) {
    common::create_for("recover-new")
        .respond_with(wiremock::ResponseTemplate::new(500))
        .expect(1)
        .mount(mock)
        .await;
    let call = Call::object("Szamlazz.Order", "recover-new", "create_invoice");
    let started = server
        .invoke(&call.send(), Some(&request()), Some("recover-new"))
        .await;
    pause(server, started.invocation_id()).await;
    server.admin().kill(started.invocation_id()).await;
    server
        .admin()
        .await_status(started.invocation_id(), &["completed"])
        .await;
    let observed = marker(server, "recover-new").await;
    let recover = Call::object("Szamlazz.Order", "recover-new", "recover");
    let mut body = json!({"operator":"operator-1","marker":observed["marker"],"evidence":{"type":"document","number":"RECOVER-1"}});
    let response = server.invoke(&recover, Some(&body), None).await;
    assert_eq!(response.status, 500, "absence cannot settle: {response:?}");
    common::external_id_query("rr:recover-new:invoice")
        .respond_with(common::Doc::of("RECOVER-1", "SZ", "recover-new").response())
        .with_priority(1)
        .mount(mock)
        .await;
    body["marker"]["proforma_number"] = json!("wrong");
    assert_eq!(server.invoke(&recover, Some(&body), None).await.status, 400);
    body["marker"]["proforma_number"] = Value::Null;
    assert_eq!(
        server.invoke(&recover, Some(&body), None).await.status,
        400,
        "explicit null is not an exact echo of an omitted member"
    );
    body["marker"] = observed["marker"].clone();
    let response = server
        .invoke(&recover, Some(&body), Some("recovery-evidence"))
        .await;
    assert_eq!(response.status, 200, "{response:?}");
    assert_eq!(marker(server, "recover-new").await["state"], "absent");
    let before = mock.received_requests().await.expect("requests").len();
    assert_eq!(
        server
            .invoke(&recover, Some(&body), Some("recovery-evidence"))
            .await
            .body,
        response.body
    );
    assert_eq!(
        mock.received_requests().await.expect("requests").len(),
        before
    );
}

pub async fn marker_compatibility(server: &Restate, mock: &wiremock::MockServer) {
    for (key, contract, valid) in [
        ("legacy-recover", None, true),
        (
            "ordinary-recover",
            Some("request_response_ordinary_v1"),
            true,
        ),
        ("unknown-recover", Some("future"), false),
    ] {
        let mut retained = json!({"version":1,"token":"owner","owner_invocation":"owner","created_at":"2026-09-14T12:00:00Z","scope":null,"order":key,"namespace":"rr","external_id":format!("rr:{key}:invoice"),"account_id":"experiment","endpoint":mock.uri(),"credential_ref":"experiment","operation":{"type":"create","kind":"invoice","expected_number":"OLD-1","corrected_number":null}});
        if let Some(contract) = contract {
            retained["execution_contract"] = json!(contract);
        }
        if key == "ordinary-recover" {
            retained["proforma_number"] = Value::Null;
        }
        let raw = serde_json::to_vec(&retained).expect("marker");
        common::http_client()
            .post(format!(
                "{}/services/Szamlazz.Order/state",
                server.admin().base()
            ))
            .json(&json!({"object_key":key,"new_state":{"unresolved-write":raw}}))
            .send()
            .await
            .expect("state")
            .error_for_status()
            .expect("state status");
        tokio::time::timeout(Duration::from_secs(10), async {
            while marker(server, key).await["state"] == "absent" {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("state visible");
        let observed = marker(server, key).await;
        let decoded: restate_szamlazz::contract::recovery::UnresolvedObservation =
            serde_json::from_value(observed.clone()).expect("observation");
        assert_eq!(
            serde_json::to_value(decoded).expect("echo"),
            observed,
            "typed observations retain member presence"
        );
        let mut body = json!({"operator":"operator-1","marker":retained,"evidence":{"type":"completed","audit_reference":"INC-247","completion":{"type":"issued","number":"OLD-1"},"completed_and_cannot_execute_later":true}});
        let call = Call::object("Szamlazz.Order", key, "recover");
        assert_eq!(
            server.invoke(&call, Some(&body), None).await.status,
            400,
            "old reissue target is never replacement evidence"
        );
        body["evidence"]["completion"]["number"] = json!("NEW-1");
        let response = server.invoke(&call, Some(&body), None).await;
        assert_eq!(
            response.status,
            if valid { 200 } else { 400 },
            "{key}: {response:?}"
        );
        assert_eq!(
            marker(server, key).await["state"],
            if valid { "absent" } else { "unresolved" }
        );
    }
}

pub async fn interrupted(server: &Restate, mock: &wiremock::MockServer, lab: &Arc<Lab>) {
    for scenario in [
        "reissue-visible",
        "reissue-invisible",
        "reissue-target-gone",
        "conversion-consumed",
        "conversion-replaced",
        "no-link-appeared",
    ] {
        println!("extension interruption: {scenario}");
        let sends = Arc::new(AtomicUsize::new(0));
        let changed = Arc::new(AtomicBool::new(false));
        let reissue = scenario.starts_with("reissue");
        let conversion = scenario.starts_with("conversion");
        let pinned = format!("D-{scenario}");
        let counter = sends.clone();
        let change = changed.clone();
        common::external_id_query(&format!("rr:{scenario}:invoice"))
            .respond_with(move |_: &wiremock::Request| {
                if counter.load(Ordering::SeqCst) > 0
                    && matches!(scenario, "reissue-visible" | "conversion-consumed")
                {
                    return common::Doc::of("NEW-INTERRUPTED", "SZ", scenario).response();
                }
                if reissue && !(scenario == "reissue-target-gone" && change.load(Ordering::SeqCst))
                {
                    let mut doc = common::Doc::of("OLD-INTERRUPTED", "SZ", scenario);
                    doc.reversed = true;
                    return doc.response();
                }
                common::not_found()
            })
            .with_priority(1)
            .mount(mock)
            .await;
        let change = changed.clone();
        let pinned_query = pinned.clone();
        common::external_id_query(&format!("rr:{scenario}:proforma"))
            .respond_with(move |_: &wiremock::Request| {
                if change.load(Ordering::SeqCst)
                    && matches!(scenario, "conversion-replaced" | "no-link-appeared")
                {
                    return common::Doc::of("D-NEW", "D", scenario).response();
                }
                if conversion
                    && !(scenario == "conversion-consumed" && change.load(Ordering::SeqCst))
                {
                    return common::Doc::of(&pinned_query, "D", scenario).response();
                }
                common::not_found()
            })
            .with_priority(1)
            .mount(mock)
            .await;
        common::number_query(&pinned)
            .respond_with(common::Doc::of(&pinned, "D", scenario).response())
            .with_priority(1)
            .mount(mock)
            .await;
        let counter = sends.clone();
        let submitted = pinned.clone();
        common::create_for(scenario)
            .respond_with(move |req: &wiremock::Request| {
                let body = String::from_utf8_lossy(&req.body);
                assert_eq!(
                    body.contains(&format!(
                        "<dijbekeroSzamlaszam>{submitted}</dijbekeroSzamlaszam>"
                    )),
                    conversion
                );
                counter.fetch_add(1, Ordering::SeqCst);
                common::created("NEW-INTERRUPTED", "1000", "1270")
            })
            .mount(mock)
            .await;
        lab.change(scenario, |s| s.hold = Some(WriteCheckpoint::Sent));
        let mut body = request();
        if reissue {
            body["options"]["reissue"] = json!({"expected_number":"OLD-INTERRUPTED"});
        }
        if conversion {
            body["options"]["proforma"] = json!("auto");
        }
        let call = Call::object("Szamlazz.Order", scenario, "create_invoice");
        let started = server
            .invoke(&call.send(), Some(&body), Some(scenario))
            .await;
        tokio::time::timeout(Duration::from_secs(30), lab.reached.notified())
            .await
            .expect("send held");
        changed.store(true, Ordering::SeqCst);
        lab.cut.notify_one();
        if matches!(
            scenario,
            "reissue-target-gone" | "conversion-replaced" | "no-link-appeared"
        ) {
            pause(server, started.invocation_id()).await;
            assert_eq!(sends.load(Ordering::SeqCst), 1, "{scenario}");
            let observed = marker(server, scenario).await;
            assert_eq!(observed["state"], "unresolved");
            assert_eq!(
                observed["marker"]["proforma_number"],
                if conversion {
                    json!(pinned)
                } else {
                    Value::Null
                }
            );
            server.admin().resume(started.invocation_id()).await;
            pause(server, started.invocation_id()).await;
            assert_eq!(sends.load(Ordering::SeqCst), 1);
            server.admin().kill(started.invocation_id()).await;
        } else {
            server
                .admin()
                .await_status(started.invocation_id(), &["completed", "paused"])
                .await;
            assert_eq!(
                server
                    .admin()
                    .invocation(started.invocation_id())
                    .await
                    .status,
                "completed"
            );
            let response = server.invoke(&call, Some(&body), Some(scenario)).await;
            assert_eq!(
                response.body["outcome"], "issued",
                "{scenario}: {response:?}"
            );
            assert_eq!(
                sends.load(Ordering::SeqCst),
                if scenario == "reissue-invisible" {
                    2
                } else {
                    1
                }
            );
        }
    }
}
