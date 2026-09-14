//! Shared storno through real buffered Restate ingress.
use super::*;

pub async fn normal(server: &Restate, mock: &wiremock::MockServer) {
    common::number_query("ORIGINAL-1")
        .respond_with(common::Doc::of("ORIGINAL-1", "SZ", "storno-normal").response())
        .with_priority(1)
        .mount(mock)
        .await;
    common::storno_of_number_repeating_telj("ORIGINAL-1")
        .respond_with(common::created("SS-1", "-1000", "-1270"))
        .expect(1)
        .mount(mock)
        .await;
    let response = server
        .invoke(
            &Call::object("Szamlazz.Order", "storno-normal", "storno_invoice"),
            Some(&json!({"invoice_number":"ORIGINAL-1"})),
            None,
        )
        .await;
    assert_eq!(response.body["outcome"], "reversed", "{response:?}");
    assert_eq!(response.body["storno_number"], "SS-1");
    assert_eq!(marker(server, "storno-normal").await["state"], "absent");
}

pub async fn unmanaged(server: &Restate, mock: &wiremock::MockServer) {
    common::number_query("UNMANAGED-1")
        .respond_with(common::Doc::unmanaged("UNMANAGED-1", "SZ").response())
        .with_priority(1)
        .mount(mock)
        .await;
    common::storno_of_number_repeating_telj("UNMANAGED-1")
        .respond_with(common::created("SS-UNMANAGED", "-1000", "-1270"))
        .expect(1)
        .mount(mock)
        .await;
    let response = server
        .invoke(
            &Call::service("Szamlazz.Agent", "storno"),
            Some(&json!({"invoice_number":"UNMANAGED-1"})),
            Some("unmanaged-storno"),
        )
        .await;
    assert_eq!(response.body["outcome"], "reversed", "{response:?}");
    let response = server
        .invoke(
            &Call::service("Szamlazz.Agent", "storno"),
            Some(&json!({"invoice_number":"ORIGINAL-1"})),
            None,
        )
        .await;
    assert_eq!(response.body["outcome"], "managed_by_order", "{response:?}");
}

pub async fn recovery(server: &Restate, mock: &wiremock::MockServer) {
    let visible = Arc::new(AtomicBool::new(false));
    let wrong = Arc::new(AtomicBool::new(false));
    let shown = visible.clone();
    let changed = wrong.clone();
    common::number_query("ORIG-RECOVERY")
        .respond_with(move |_: &wiremock::Request| {
            let mut doc = common::Doc::of("ORIG-RECOVERY", "SZ", "storno-recovery");
            doc.reversed = shown.load(Ordering::SeqCst);
            if changed.load(Ordering::SeqCst) {
                doc.document_id += 1;
            }
            doc.response()
        })
        .with_priority(1)
        .mount(mock)
        .await;
    let shown = visible.clone();
    common::external_id_query("rr:storno-recovery:storno:ORIG-RECOVERY")
        .respond_with(move |_: &wiremock::Request| {
            if shown.load(Ordering::SeqCst) {
                let mut doc = common::Doc::of("SS-RECOVERY", "SS", "storno-recovery");
                doc.referenced_invoice = Some("ORIG-RECOVERY");
                doc.response()
            } else {
                common::not_found()
            }
        })
        .with_priority(1)
        .mount(mock)
        .await;
    common::storno_of_number("ORIG-RECOVERY")
        .respond_with(wiremock::ResponseTemplate::new(500))
        .expect(1)
        .mount(mock)
        .await;
    let call = Call::object("Szamlazz.Order", "storno-recovery", "storno_invoice");
    let body = json!({"invoice_number":"ORIG-RECOVERY"});
    let started = server
        .invoke(&call.send(), Some(&body), Some("storno-recovery"))
        .await;
    pause(server, started.invocation_id()).await;
    server.admin().kill(started.invocation_id()).await;
    server
        .admin()
        .await_status(started.invocation_id(), &["completed"])
        .await;
    assert_eq!(
        server
            .invoke(&call, Some(&body), Some("new-storno-recovery"))
            .await
            .status,
        500
    );
    let observed = marker(server, "storno-recovery").await;
    let recover = Call::object("Szamlazz.Order", "storno-recovery", "recover");
    let mut evidence = json!({"operator":"operator","marker":observed["marker"],"evidence":{"type":"document","number":"SS-RECOVERY"}});
    visible.store(true, Ordering::SeqCst);
    let mut storno = common::Doc::of("SS-RECOVERY", "SS", "storno-recovery");
    storno.referenced_invoice = Some("ORIG-RECOVERY");
    common::number_query("SS-RECOVERY")
        .respond_with(storno.response())
        .with_priority(1)
        .mount(mock)
        .await;
    wrong.store(true, Ordering::SeqCst);
    assert_eq!(
        server.invoke(&recover, Some(&evidence), None).await.status,
        500,
        "other provider id is not the pinned original"
    );
    wrong.store(false, Ordering::SeqCst);
    evidence["marker"]["execution_contract"]["request_response_storno_v1"]["e_invoice"] =
        json!(true);
    assert_eq!(
        server.invoke(&recover, Some(&evidence), None).await.status,
        400
    );
    evidence["marker"] = observed["marker"].clone();
    let response = server
        .invoke(&recover, Some(&evidence), Some("recover-storno-evidence"))
        .await;
    assert_eq!(response.status, 200, "{response:?}");
    assert_eq!(marker(server, "storno-recovery").await["state"], "absent");
}

pub async fn cancellation(server: &Restate, mock: &wiremock::MockServer) {
    common::number_query("ORIG-CANCEL-STORNO")
        .respond_with(common::Doc::of("ORIG-CANCEL-STORNO", "ES", "storno-cancel").response())
        .with_priority(1)
        .mount(mock)
        .await;
    common::storno_of_number("ORIG-CANCEL-STORNO")
        .respond_with(wiremock::ResponseTemplate::new(500))
        .expect(1)
        .mount(mock)
        .await;
    let call = Call::object("Szamlazz.Order", "storno-cancel", "storno_invoice");
    let body = json!({"invoice_number":"ORIG-CANCEL-STORNO"});
    let started = server
        .invoke(&call.send(), Some(&body), Some("cancel-storno"))
        .await;
    pause(server, started.invocation_id()).await;
    server.admin().cancel(started.invocation_id()).await;
    server
        .admin()
        .await_status(started.invocation_id(), &["completed"])
        .await;
    let result = server
        .invoke(&call, Some(&body), Some("cancel-storno"))
        .await;
    let fault = result.fault::<restate_szamlazz::contract::Fault>();
    assert_eq!(
        fault.code,
        restate_szamlazz::contract::TerminalCode::OutcomeUnknown
    );
    assert_eq!(fault.kind, None);
    assert_eq!(marker(server, "storno-cancel").await["state"], "unresolved");
}

pub async fn interrupted(server: &Restate, mock: &wiremock::MockServer, lab: &Arc<Lab>) {
    for scenario in [
        "storno-visible",
        "storno-resend",
        "storno-date",
        "storno-appearance",
        "storno-id",
        "storno-order",
        "storno-refusal",
        "storno-recorded",
        "storno-warning",
        "storno-echo",
        "storno-unnumbered",
        "storno-private-code",
    ] {
        let number = format!("ORIG-{scenario}");
        let reversal = format!("SS-{scenario}");
        let sends = Arc::new(AtomicUsize::new(0));
        let changed = Arc::new(AtomicBool::new(false));
        let visible = Arc::new(AtomicBool::new(false));
        let shown = visible.clone();
        let change = changed.clone();
        let original = number.clone();
        common::number_query(&number)
            .respond_with(move |_: &wiremock::Request| {
                let mut doc = common::Doc::of(&original, "SZ", scenario);
                doc.reversed = shown.load(Ordering::SeqCst);
                if change.load(Ordering::SeqCst) {
                    match scenario {
                        "storno-date" => doc.fulfillment_date = Some(jiff::civil::date(2025, 1, 1)),
                        "storno-appearance" => doc.eszamla = Some(3),
                        "storno-id" => doc.document_id += 1,
                        "storno-order" => doc.order = Some("OTHER"),
                        _ => {}
                    }
                }
                doc.response()
            })
            .with_priority(1)
            .mount(mock)
            .await;
        let shown = visible.clone();
        let original = number.clone();
        let ss = reversal.clone();
        let change = changed.clone();
        common::external_id_query(&format!("rr:{scenario}:storno:{number}"))
            .respond_with(move |_: &wiremock::Request| {
                if scenario == "storno-private-code" && change.load(Ordering::SeqCst) {
                    return common::api_error("42", "PRIVATE-STORNO-VENDOR-TEXT");
                }
                if shown.load(Ordering::SeqCst) {
                    let mut doc = common::Doc::of(&ss, "SS", scenario);
                    doc.referenced_invoice = Some(&original);
                    doc.response()
                } else {
                    common::not_found()
                }
            })
            .with_priority(1)
            .mount(mock)
            .await;
        let count = sends.clone();
        let ss = reversal.clone();
        let original = number.clone();
        common::storno_of_number_repeating_telj(&number)
            .respond_with(move |req: &wiremock::Request| {
                let body = String::from_utf8_lossy(&req.body);
                assert!(body.contains("notify@example.test"));
                assert!(body.contains("<eszamla>false</eszamla>"));
                let n = count.fetch_add(1, Ordering::SeqCst);
                match scenario {
                    "storno-recorded" => wiremock::ResponseTemplate::new(500),
                    "storno-warning" => {
                        common::created_but_notification_failed(&ss, "1000", "1270")
                    }
                    "storno-echo" => common::created(&original, "-1000", "-1270"),
                    "storno-unnumbered" => common::created_without_a_number(),
                    "storno-refusal" if n > 0 => common::api_error("13", "scripted refusal"),
                    _ => common::created(&ss, "-1000", "-1270"),
                }
            })
            .mount(mock)
            .await;
        let recorded = matches!(
            scenario,
            "storno-recorded" | "storno-warning" | "storno-echo" | "storno-unnumbered"
        );
        if !recorded {
            lab.change(scenario, |s| s.hold = Some(WriteCheckpoint::Sent));
        }
        let call = Call::object("Szamlazz.Order", scenario, "storno_invoice");
        let body = json!({"invoice_number":number,"buyer_email":"notify@example.test","comment":"Pinned comment"});
        let started = server
            .invoke(&call.send(), Some(&body), Some(scenario))
            .await;
        if !recorded {
            tokio::time::timeout(Duration::from_secs(30), lab.reached.notified())
                .await
                .expect("storno held");
            changed.store(true, Ordering::SeqCst);
            visible.store(scenario == "storno-visible", Ordering::SeqCst);
            lab.cut.notify_one();
        }
        let expected = if matches!(scenario, "storno-resend" | "storno-refusal") {
            2
        } else {
            1
        };
        if !matches!(scenario, "storno-visible" | "storno-resend") {
            pause(server, started.invocation_id()).await;
            assert_eq!(sends.load(Ordering::SeqCst), expected, "{scenario}");
            assert!(
                !format!(
                    "{:?}",
                    server.admin().journal(started.invocation_id()).await
                )
                .contains("PRIVATE-STORNO-VENDOR-TEXT")
            );
            let observed = marker(server, scenario).await;
            assert_eq!(observed["state"], "unresolved");
            server.admin().resume(started.invocation_id()).await;
            pause(server, started.invocation_id()).await;
            assert_eq!(sends.load(Ordering::SeqCst), expected);
            changed.store(false, Ordering::SeqCst);
            visible.store(true, Ordering::SeqCst);
            server.admin().resume(started.invocation_id()).await;
        }
        server
            .admin()
            .await_status(started.invocation_id(), &["completed"])
            .await;
        let response = server.invoke(&call, Some(&body), Some(scenario)).await;
        assert_eq!(
            response.body["outcome"], "reversed",
            "{scenario}: {response:?}"
        );
        assert_eq!(response.body["storno_number"], reversal);
        if scenario == "storno-warning" {
            assert!(
                response.body["warnings"]
                    .as_array()
                    .expect("warnings")
                    .contains(&json!("notification_delivery_failed"))
            );
        }
        assert_eq!(sends.load(Ordering::SeqCst), expected);
        assert_eq!(marker(server, scenario).await["state"], "absent");
    }
}
