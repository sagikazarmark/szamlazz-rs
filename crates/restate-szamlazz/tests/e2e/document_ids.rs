//! Provider IDs follow the document actually observed, through public handlers.
use super::*;

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; original and reversal ID association"]
#[allow(
    clippy::too_many_lines,
    reason = "both handlers across acknowledgement, lookup and verification evidence"
)]
async fn e2e_document_ids_storno_acknowledgement_and_lookup() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "storno-ids",
            ..SERVER
        })
        .await;
    let mock = MockServer::start().await;
    let config = WorkerConfig::new("acct".parse().expect("namespace"))
        .validate()
        .expect("config");
    let (order, agent) = services_with_config(&mock.uri(), config);
    restate
        .deploy(Endpoint::builder().bind(order).bind(agent).build())
        .await;
    for managed in [true, false] {
        let call = if managed {
            Call::object("Szamlazz.Order", "IDS", "storno_invoice")
        } else {
            Call::service("Szamlazz.Agent", "storno")
        };
        let id = if managed {
            "acct:IDS:storno:SZ-IDS"
        } else {
            "acct:by-number:SZ-IDS:storno"
        };
        for header in [None, Some("9223372036854775807")] {
            number_query("SZ-IDS")
                .respond_with(
                    Doc {
                        order: managed.then_some("IDS"),
                        ..Doc::new("SZ-IDS", "SZ")
                    }
                    .response(),
                )
                .mount(&mock)
                .await;
            external_id_query(id)
                .respond_with(not_found())
                .mount(&mock)
                .await;
            let mut acknowledgement = ResponseTemplate::new(200).set_body_raw(
                crate::common::numbered_reply_body("SS-IDS", Some("-1270")),
                "application/xml",
            );
            if let Some(header) = header {
                acknowledgement = acknowledgement.insert_header("szlahu_id", header);
            }
            crate::common::storno_of_number("SZ-IDS")
                .respond_with(acknowledgement)
                .expect(1)
                .mount(&mock)
                .await;
            let body = json!({"invoice_number":"SZ-IDS"});
            let key = format!("storno-ids-{managed}-{header:?}");
            let response = restate.invoke(&call, Some(&body), Some(&key)).await;
            assert_eq!(response.body["outcome"], "reversed", "{}", response.body);
            assert_eq!(response.body["invoice_document_id"], 924_307_338_i64);
            assert_eq!(
                response.body["storno_document_id"],
                header.map_or(json!(null), |_| json!(i64::MAX))
            );
            mock.verify().await;
            mock.reset().await;
            let replay = restate.invoke(&call, Some(&body), Some(&key)).await;
            assert_eq!(replay.body, response.body);
            assert!(mock.received_requests().await.expect("requests").is_empty());
        }
        for reversed in [false, true] {
            // A live first observation followed by a reversed original exercises
            // the protected lookup's fresh verification as well as the early path.
            let reads = Arc::new(AtomicUsize::new(0));
            number_query("SZ-IDS")
                .respond_with(move |_: &wiremock::Request| {
                    Doc {
                        order: managed.then_some("IDS"),
                        reversed: reversed || reads.fetch_add(1, Ordering::SeqCst) > 0,
                        ..Doc::new("SZ-IDS", "SZ")
                    }
                    .response()
                })
                .mount(&mock)
                .await;
            let reversal = Doc {
                document_id: 8_123_456_789,
                order: managed.then_some("IDS"),
                referenced_invoice: Some("SZ-IDS"),
                ..Doc::new("SS-IDS", "SS")
            }
            .response();
            external_id_query(id)
                .respond_with(reversal.clone())
                .mount(&mock)
                .await;
            order_query("IDS").respond_with(reversal).mount(&mock).await;
            let response = restate
                .invoke(&call, Some(&json!({"invoice_number":"SZ-IDS"})), None)
                .await;
            assert_eq!(response.body["outcome"], "reversed", "{}", response.body);
            assert_eq!(response.body["invoice_document_id"], 924_307_338_i64);
            assert_eq!(response.body["storno_number"], "SS-IDS");
            assert_eq!(response.body["storno_document_id"], 8_123_456_789_i64);
            assert!(
                mock.received_requests()
                    .await
                    .expect("requests")
                    .iter()
                    .all(|r| !String::from_utf8_lossy(&r.body).contains("action-szamla_agent_st"))
            );
            mock.reset().await;
        }
        // An ambiguous acknowledgement requires a query that supplies the ID,
        // even when its szlahu_id header was absent.
        let reads = Arc::new(AtomicUsize::new(0));
        number_query("SZ-IDS")
            .respond_with(move |_: &wiremock::Request| {
                Doc {
                    order: managed.then_some("IDS"),
                    reversed: reads.fetch_add(1, Ordering::SeqCst) > 0,
                    ..Doc::new("SZ-IDS", "SZ")
                }
                .response()
            })
            .mount(&mock)
            .await;
        external_id_query(id)
            .respond_with(not_found())
            .mount(&mock)
            .await;
        number_query("SS-IDS")
            .respond_with(
                Doc {
                    document_id: 8_123_456_789,
                    order: managed.then_some("IDS"),
                    referenced_invoice: Some("SZ-IDS"),
                    ..Doc::new("SS-IDS", "SS")
                }
                .response(),
            )
            .mount(&mock)
            .await;
        crate::common::storno_of_number("SZ-IDS")
            .respond_with(crate::common::created_without_totals("SS-IDS"))
            .expect(1)
            .mount(&mock)
            .await;
        let response = restate
            .invoke(&call, Some(&json!({"invoice_number":"SZ-IDS"})), None)
            .await;
        assert_eq!(response.body["outcome"], "reversed", "{}", response.body);
        assert_eq!(response.body["invoice_document_id"], 924_307_338_i64);
        assert_eq!(response.body["storno_document_id"], 8_123_456_789_i64);
        mock.verify().await;
        mock.reset().await;
        number_query("SZ-IDS")
            .respond_with(
                Doc {
                    order: managed.then_some("IDS"),
                    reversed: true,
                    ..Doc::new("SZ-IDS", "SZ")
                }
                .response(),
            )
            .mount(&mock)
            .await;
        external_id_query(id)
            .respond_with(not_found())
            .mount(&mock)
            .await;
        order_query("IDS")
            .respond_with(not_found())
            .mount(&mock)
            .await;
        let response = restate
            .invoke(&call, Some(&json!({"invoice_number":"SZ-IDS"})), None)
            .await;
        assert_eq!(response.body["invoice_document_id"], 924_307_338_i64);
        assert_eq!(response.body["storno_number"], json!(null));
        assert_eq!(response.body["storno_document_id"], json!(null));
        mock.reset().await;
    }
    restate.finish().await;
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; provider metadata and retained responses"]
#[allow(
    clippy::too_many_lines,
    reason = "one document journey through issuance, replay and observation"
)]
async fn e2e_document_ids_create_query_get_and_replay() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "document-ids",
            ..SERVER
        })
        .await;
    let mock = MockServer::start().await;
    let config = WorkerConfig::new("acct".parse().expect("namespace"))
        .validate()
        .expect("config");
    let (order, agent) = services_with_config(&mock.uri(), config);
    restate
        .deploy(Endpoint::builder().bind(order).bind(agent).build())
        .await;
    for header in [None, Some("9223372036854775807")] {
        let body = create_body(dec!(1000));
        external_id_query("acct:IDS:invoice")
            .respond_with(not_found())
            .mount(&mock)
            .await;
        for kind in ["prepayment", "final", "proforma"] {
            external_id_query(&format!("acct:IDS:{kind}"))
                .respond_with(not_found())
                .mount(&mock)
                .await;
        }
        order_query("IDS")
            .respond_with(not_found())
            .mount(&mock)
            .await;
        let mut acknowledgement = crate::common::created_without_totals("SZ-IDS");
        if let Some(header) = header {
            acknowledgement = acknowledgement.insert_header("szlahu_id", header);
        }
        create_for("IDS")
            .respond_with(acknowledgement)
            .expect(1)
            .mount(&mock)
            .await;
        let key = if header.is_some() {
            "ids-present"
        } else {
            "ids-absent"
        };
        let call = Call::object("Szamlazz.Order", "IDS", "create_invoice");
        let issued = restate.invoke(&call, Some(&body), Some(key)).await;
        assert_eq!(issued.body["outcome"], "issued", "{}", issued.body);
        assert_eq!(
            issued.body["document_id"],
            header.map_or(json!(null), |_| json!(i64::MAX))
        );
        mock.verify().await;
        mock.reset().await;
        let observed = Doc {
            referenced_proforma: Some("D-CONSUMED"),
            ..Doc::of("SZ-IDS", "SZ", "IDS")
        };
        external_id_query("acct:IDS:invoice")
            .respond_with(observed.response())
            .mount(&mock)
            .await;
        number_query("SZ-IDS")
            .respond_with(observed.response())
            .mount(&mock)
            .await;
        for kind in ["prepayment", "final", "proforma"] {
            external_id_query(&format!("acct:IDS:{kind}"))
                .respond_with(not_found())
                .mount(&mock)
                .await;
        }
        let replay = restate.invoke(&call, Some(&body), Some(key)).await;
        assert_eq!(
            replay.body, issued.body,
            "retained response does not enrich metadata"
        );
        assert!(mock.received_requests().await.expect("requests").is_empty());
        let existing = restate.invoke(&call, Some(&body), None).await;
        assert_eq!(existing.body["outcome"], "already_issued");
        assert_eq!(existing.body["document_id"], 924_307_338_i64);
        let queried = restate
            .invoke(
                &Call::service("Szamlazz.Agent", "query"),
                Some(&json!({"selector":{"invoice_number":"SZ-IDS"}})),
                None,
            )
            .await;
        assert_eq!(
            queried.body["document_id"], 924_307_338_i64,
            "{}",
            queried.body
        );
        let status = restate
            .invoke(&Call::object("Szamlazz.Order", "IDS", "get"), None, None)
            .await;
        assert_eq!(status.body["invoice"]["document_id"], 924_307_338_i64);
        assert_eq!(status.body["proforma"]["number"], "D-CONSUMED");
        assert_eq!(status.body["proforma"]["document_id"], json!(null));
        mock.reset().await;
        external_id_query("acct:IDS:invoice")
            .respond_with(
                Doc {
                    reversed: true,
                    ..observed
                }
                .response(),
            )
            .mount(&mock)
            .await;
        order_query("IDS")
            .respond_with(not_found())
            .mount(&mock)
            .await;
        let reversed = restate.invoke(&call, Some(&body), None).await;
        assert_eq!(reversed.body["outcome"], "reversed");
        assert_eq!(reversed.body["document_id"], 924_307_338_i64);
        mock.reset().await;
    }
    restate.finish().await;
}
