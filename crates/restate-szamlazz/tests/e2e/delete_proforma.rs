//! `Szamlazz.Order.delete_proforma` under Restate: pinned-target refresh after
//! pause/resume, guard outcomes, write faults, and replay of stored completions.

use serde_json::json;

use crate::harness::Harness;
use crate::harness::szamlazz::{
    Doc, delete_of, external_id_query, not_found, number_query, proforma_deleted,
};

/// A recorded manual/legacy target is selected by number even with a separate
/// namespace-owned proforma. After retention ends the old intent stays exact.
pub(crate) async fn named_target_deletion_leaves_the_namespace_holder_untouched(h: &Harness) {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    let order = "E2E-D-NAMED";
    let deleted = Arc::new(AtomicBool::new(false));
    let state = deleted.clone();
    number_query("D-LEGACY")
        .respond_with(move |_: &wiremock::Request| {
            if state.load(Ordering::SeqCst) {
                not_found()
            } else {
                Doc::of("D-LEGACY", "D", order).response()
            }
        })
        .expect(3)
        .mount(&h.mock)
        .await;
    external_id_query(&format!("acct:{order}:proforma"))
        .respond_with(Doc::of("D-WORKER", "D", order).response())
        .expect(0)
        .mount(&h.mock)
        .await;
    delete_of("D-LEGACY")
        .respond_with(move |_: &wiremock::Request| {
            deleted.store(true, Ordering::SeqCst);
            proforma_deleted()
        })
        .expect(1)
        .mount(&h.mock)
        .await;
    delete_of("D-WORKER")
        .respond_with(proforma_deleted())
        .expect(0)
        .mount(&h.mock)
        .await;
    let body = json!({"expected_number":"D-LEGACY", "mode":"named_target"});
    let first = h
        .call(order, "delete_proforma", &body, "named-delete")
        .await;
    assert_eq!(first.body, json!({"deleted":true,"reason":null}));
    assert_eq!(
        h.admin().runs(first.invocation_id()).await,
        [
            "namespace",
            "account",
            "verify-proforma-D-LEGACY",
            "prepare-write",
            "arm-write",
            "delete-proforma-D-LEGACY"
        ]
    );
    let replay = h
        .call(order, "delete_proforma", &body, "named-delete")
        .await;
    assert_eq!(replay.invocation_id(), first.invocation_id());
    h.admin().purge(first.invocation_id()).await;
    let old = h
        .call(order, "delete_proforma", &body, "named-delete")
        .await;
    assert_eq!(old.body, json!({"deleted":true,"reason":"absent"}));
    h.assert_state_absent(None, order).await;
}

/// The main suite walks the complete named-target reconciliation journal path.
pub(crate) async fn named_target_deletion_reconciles_a_lost_reply_without_another_send(
    h: &Harness,
) {
    let order = "E2E-D-NAMED-LOST";
    let number = "D-NAMED-LOST";
    number_query(number)
        .respond_with(Doc::of(number, "D", order).response())
        .expect(2)
        .mount(&h.mock)
        .await;
    delete_of(number)
        .respond_with(wiremock::ResponseTemplate::new(500))
        .expect(1)
        .mount(&h.mock)
        .await;
    let body = json!({"mode":"named_target", "expected_number":number});
    let call = restate_e2e_harness::Call::object("Szamlazz.Order", order, "delete_proforma");
    let owner = h
        .invoke(&call.send(), Some(&body), Some("named-lost"))
        .await;
    h.admin()
        .await_status(owner.invocation_id(), &["paused"])
        .await;
    assert_eq!(
        h.admin().runs(owner.invocation_id()).await,
        [
            "namespace",
            "account",
            "verify-proforma-D-NAMED-LOST",
            "prepare-write",
            "arm-write",
            "delete-proforma-D-NAMED-LOST",
            "reconcile-write"
        ]
    );
    h.admin().cancel(owner.invocation_id()).await;
    let cancelled = h.invoke(&call, Some(&body), Some("named-lost")).await;
    assert_eq!(cancelled.fault().is_cancelled(), Some(true));
    let blocked = h
        .call(order, "delete_proforma", &body, "named-lost-next")
        .await;
    assert_eq!(blocked.status, 500);
    h.expect_unresolved(None, order).await;
}

/// Exact-number selection requires the reported order/type and preserves read faults.
#[allow(clippy::too_many_lines, reason = "one named-target selection matrix")]
pub(crate) async fn named_target_deletion_requires_reported_order_type_and_unpaid_content(
    h: &Harness,
) {
    use crate::common::{CreditRecord, api_error};
    use wiremock::ResponseTemplate;

    let order = "E2E-D-NAMED-GUARDS";
    external_id_query(&format!("acct:{order}:proforma"))
        .respond_with(not_found())
        .expect(0)
        .mount(&h.mock)
        .await;
    let paid = [CreditRecord::transfer("1")];
    for (label, response, force, status, reason, sends) in [
        (
            "UNASSOCIATED",
            Doc {
                order: None,
                ..Doc::of("D-NAMED-UNASSOCIATED", "D", order)
            }
            .response(),
            true,
            200,
            "target_changed",
            0,
        ),
        (
            "WRONG-ORDER",
            Doc::of("D-NAMED-WRONG-ORDER", "D", "OTHER").response(),
            true,
            200,
            "target_changed",
            0,
        ),
        (
            "WRONG-TYPE",
            Doc::of("D-NAMED-WRONG-TYPE", "SZ", order).response(),
            true,
            200,
            "target_changed",
            0,
        ),
        (
            "OPEN-TYPE",
            Doc::of("D-NAMED-OPEN-TYPE", "FUTURE", order).response(),
            true,
            200,
            "target_changed",
            0,
        ),
        (
            "WRONG-NUMBER",
            Doc::of("D-NEWER", "D", order).response(),
            true,
            503,
            "unavailable",
            0,
        ),
        ("MISSING", not_found(), false, 200, "absent", 0),
        (
            "READ",
            ResponseTemplate::new(500),
            false,
            503,
            "unavailable",
            0,
        ),
        (
            "API",
            api_error("57", "read refused"),
            false,
            503,
            "unavailable",
            0,
        ),
        (
            "CREDENTIAL",
            api_error("3", "login"),
            false,
            503,
            "credentials_rejected",
            0,
        ),
        (
            "PAID",
            Doc {
                credit_entries: &paid,
                ..Doc::of("D-NAMED-PAID", "D", order)
            }
            .response(),
            false,
            200,
            "proforma_paid",
            0,
        ),
        (
            "FORCE",
            Doc {
                credit_entries: &paid,
                ..Doc::of("D-NAMED-FORCE", "D", order)
            }
            .response(),
            true,
            200,
            "",
            1,
        ),
        (
            "NO-ID",
            Doc::of("D-NAMED-NO-ID", "D", order).response(),
            false,
            200,
            "",
            1,
        ),
    ] {
        // Each case has an independent target, including any late read execution.
        let number = format!("D-NAMED-{label}");
        number_query(&number)
            .respond_with(response)
            .expect(1..)
            .mount(&h.mock)
            .await;
        delete_of(&number)
            .respond_with(proforma_deleted())
            .expect(sends)
            .mount(&h.mock)
            .await;
        let reply = h
            .call(
                order,
                "delete_proforma",
                &json!({"expected_number":number, "mode":"named_target", "force":force}),
                &format!("named-{label}"),
            )
            .await;
        assert_eq!(reply.status, status, "{label}: {}", reply.body);
        if status == 200 {
            assert_eq!(
                reply.body["deleted"],
                reason.is_empty() || reason == "absent",
                "{label}"
            );
            assert_eq!(
                reply.body["reason"],
                if reason.is_empty() {
                    json!(null)
                } else {
                    json!(reason)
                },
                "{label}"
            );
        } else {
            assert_eq!(reply.fault().code.as_str(), reason, "{label}");
        }
        h.assert_state_absent(None, order).await;
    }
}

/// Guard outcomes and send uncertainty survive the real handler/envelope path.
/// Reusing the ingress key replays the completion without re-reading or sending.
pub(crate) async fn deletion_answers_preserve_guard_failures_and_send_uncertainty(h: &Harness) {
    use crate::common::{api_error, szlahu_down};
    use restate_szamlazz::contract::TerminalCode;
    use wiremock::ResponseTemplate;

    // `fresh: None` means the number query agrees and the write is reached.
    // Labels only name fixtures; guard/send behavior and expected faults are explicit.
    for (suffix, fresh, send, status, reason, terminal, code) in [
        ("CHANGED", Some(Doc::new("D-OTHER", "SZ").response()), proforma_deleted(), 503, "", Some(TerminalCode::Unavailable), None),
        ("GONE", Some(not_found()), proforma_deleted(), 200, "", None, None),
        ("READ", Some(ResponseTemplate::new(500)), proforma_deleted(), 503, "", Some(TerminalCode::Unavailable), None),
        ("READCODE", Some(api_error("57", "query cause")), proforma_deleted(), 503, "", Some(TerminalCode::Unavailable), Some("57")),
        ("FUTURE", None, api_error("99999", "send cause"), 500, "", Some(TerminalCode::OutcomeUnknown), Some("99999")),
        ("ABSENT", None, ResponseTemplate::new(200).set_body_string(
            "<xmlszamladbkdelvalasz xmlns=\"http://www.szamlazz.hu/xmlszamladbkdelvalasz\"><sikeres>false</sikeres><hibauzenet>send cause</hibauzenet></xmlszamladbkdelvalasz>"), 500, "", Some(TerminalCode::OutcomeUnknown), Some("absent")),
        ("DOWN", None, szlahu_down(), 500, "", Some(TerminalCode::OutcomeUnknown), None),
        ("REFUSED", None, api_error("57", "XML reading error"), 200, "57", None, None),
        ("335", None, api_error("335", "gone"), 200, "", None, None),
        ("CREDENTIAL", None, api_error("3", "login"), 503, "", Some(TerminalCode::CredentialsRejected), Some("3")),
    ] {
        let order = format!("E2E-D-{suffix}");
        let number = format!("D-{suffix}");
        let doc = Doc::of(&number, "D", &order);
        let reaches_send = fresh.is_none();
        external_id_query(&format!("acct:{order}:proforma")).respond_with(doc.response()).expect(1..).mount(&h.mock).await;
        number_query(&number).respond_with(fresh.unwrap_or_else(|| doc.response())).expect(1).mount(&h.mock).await;
        delete_of(&number).respond_with(send).expect(u64::from(reaches_send)).mount(&h.mock).await;
        let key = format!("delete-{suffix}");
        let body = json!({"expected_number": number, "force": true});
        if status == 500 {
            let call = restate_e2e_harness::Call::object("Szamlazz.Order", &order, "delete_proforma");
            let submitted = h.invoke(&call.send(), Some(&body), Some(&key)).await;
            h.admin().await_status(submitted.invocation_id(), &["paused"]).await;
            assert_eq!(h.delete_bodies_of(&number).await.len(), 1);
            h.admin().cancel(submitted.invocation_id()).await;
            let cancelled = h.invoke(&call, Some(&body), Some(&key)).await;
            assert_eq!(cancelled.fault().is_cancelled(), Some(true));
            let blocked = h.call(&order, "delete_proforma", &body, &format!("{key}-next")).await;
            assert_eq!(blocked.status, 500);
            h.expect_unresolved(None, &order).await;
            continue;
        }
        let reply = h.call(&order, "delete_proforma", &body, &key).await;
        assert_eq!(reply.status, status, "{suffix}: {}", reply.body);
        if status == 200 {
            assert_eq!(reply.body["deleted"], reason.is_empty(), "{}", reply.body);
            if !reason.is_empty() { assert_eq!(reply.body["reason"], reason); }
        } else {
            let fault = reply.fault();
            assert_eq!(fault.szamlazz_code.as_deref(), code, "{fault:?}");
            assert_eq!(Some(&fault.code), terminal.as_ref());
            if status == 500 {
                assert!(fault.message.contains("may have landed"), "{fault:?}");
                assert!(fault.message.contains("read get"), "{fault:?}");
                assert!(!fault.message.contains("nothing was sent"), "{fault:?}");
                if code.is_some() { assert!(fault.message.contains("send cause"), "{fault:?}"); }
            }
        }
        let stored = h.call(&order, "delete_proforma", &body, &key).await;
        assert_eq!(stored.invocation_id(), reply.invocation_id());
        assert_eq!(stored.body, reply.body);
        assert_eq!(h.delete_bodies_of(&number).await.len(), usize::from(reaches_send));
        h.assert_state_absent(None, &order).await;
    }
}

/// Pause an unfinished deletion after its ownership result is journaled.
/// On replay credit entries have arrived and a replacement holds the external
/// id. Consumed permission prevents another guard or send. Reconciliation retains
/// uncertainty without querying: no document observation can establish deletion.
pub(crate) async fn interrupted_deletion_reconciles_without_requerying_or_resending(h: &Harness) {
    use crate::common::CreditRecord;
    use restate_e2e_harness::run_result;
    use std::{sync::Arc, time::Duration};

    let reached = Arc::new(tokio::sync::Notify::new());
    external_id_query("acct:E2E-D-REPLAY:proforma")
        .respond_with(Doc::of("D-PINNED", "D", "E2E-D-REPLAY").response())
        .up_to_n_times(1)
        .expect(1)
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-D-REPLAY:proforma")
        .respond_with(Doc::of("D-REPLACEMENT", "D", "E2E-D-REPLAY").response())
        // Deletion reconciliation cannot establish completion from a holder;
        // it must not query the replacement or renew the consumed send permit.
        .expect(0)
        .mount(&h.mock)
        .await;
    let signal = Arc::clone(&reached);
    number_query("D-PINNED")
        .respond_with(move |_: &wiremock::Request| {
            signal.notify_one();
            Doc::of("D-PINNED", "D", "E2E-D-REPLAY")
                .response()
                .set_delay(Duration::from_secs(60))
        })
        .up_to_n_times(1)
        .expect(1)
        .mount(&h.mock)
        .await;
    delete_of("D-PINNED")
        .respond_with(proforma_deleted())
        .expect(0)
        .mount(&h.mock)
        .await;
    delete_of("D-REPLACEMENT")
        .respond_with(proforma_deleted())
        .expect(0)
        .mount(&h.mock)
        .await;

    let body = json!({"expected_number": "D-PINNED"});
    let call = h.call("E2E-D-REPLAY", "delete_proforma", &body, "delete-replay");
    let interrupt = async {
        reached.notified().await;
        let id = h.in_flight_on("E2E-D-REPLAY").await;
        h.admin().pause(&id).await;
        let journal = h.admin().journal(&id).await;
        let lookup = run_result(&journal, "lookup-proforma").expect("completed ownership lookup");
        assert!(lookup.raw_contains("D-PINNED"));
        assert!(run_result(&journal, "delete-proforma-D-PINNED").is_none());
        number_query("D-PINNED")
            .respond_with(
                Doc {
                    credit_entries: &[CreditRecord::transfer("1")],
                    ..Doc::of("D-PINNED", "D", "E2E-D-REPLAY")
                }
                .response(),
            )
            .expect(0)
            .mount(&h.mock)
            .await;
        h.admin().resume(&id).await;
        h.admin().await_status(&id, &["paused"]).await;
        assert!(
            h.admin()
                .runs(&id)
                .await
                .iter()
                .any(|run| run == "reconcile-write"),
            "replay must reach reconciliation, not skip the unresolved write"
        );
        h.admin().cancel(&id).await;
    };
    let (reply, ()) = tokio::join!(call, interrupt);
    assert_eq!(reply.status, 500, "{}", reply.body);
    assert_eq!(reply.fault().is_cancelled(), Some(true));
    assert!(h.delete_bodies_of("D-PINNED").await.is_empty());
    assert!(h.delete_bodies_of("D-REPLACEMENT").await.is_empty());
    let observed = h
        .invoke(
            &restate_e2e_harness::Call::object(
                "Szamlazz.Order",
                "E2E-D-REPLAY",
                "observe_unresolved",
            ),
            None,
            None,
        )
        .await;
    assert_eq!(observed.body["state"], "unresolved", "{}", observed.body);
    assert_eq!(
        observed.body["marker"]["operation"],
        json!({"type":"delete", "number":"D-PINNED"})
    );
    h.expect_unresolved(None, "E2E-D-REPLAY").await;
}

/// The order's live proforma is found under its external id and deleted after
/// one send; once gone, a second call finds nothing and answers
/// `deleted{reason: absent}` from the lookup alone.
pub(crate) async fn proforma_is_deleted_by_the_orders_handler(h: &Harness) {
    // Live for the first call's lookup, gone after the delete.
    external_id_query("acct:E2E-D1:proforma")
        .respond_with(Doc::of("D-D1", "D", "E2E-D1").response())
        .up_to_n_times(1)
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-D1:proforma")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    delete_of("D-D1")
        .respond_with(proforma_deleted())
        .expect(1)
        .mount(&h.mock)
        .await;
    number_query("D-D1")
        .respond_with(Doc::of("D-D1", "D", "E2E-D1").response())
        .mount(&h.mock)
        .await;

    let reply = h
        .call(
            "E2E-D1",
            "delete_proforma",
            &json!({"expected_number": "D-D1"}),
            "e2e-d1-k1",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["deleted"], true, "{}", reply.body);
    assert!(reply.body["reason"].is_null(), "{}", reply.body);
    h.assert_state_absent(None, "E2E-D1").await;
    assert_eq!(
        h.admin().runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "lookup-proforma",
            "prepare-write",
            "arm-write",
            "delete-proforma-D-D1"
        ]
    );

    let again = h
        .call(
            "E2E-D1",
            "delete_proforma",
            &json!({"expected_number": "D-D1"}),
            "e2e-d1-k2",
        )
        .await;
    assert_eq!(again.status, 200, "{}", again.body);
    assert_eq!(again.body["deleted"], true, "{}", again.body);
    assert_eq!(again.body["reason"], "absent", "{}", again.body);
    assert_eq!(
        h.admin().runs(again.invocation_id()).await,
        ["namespace", "account", "lookup-proforma"],
        "the lookup answered; no delete step"
    );
    assert_eq!(h.delete_bodies_of("D-D1").await.len(), 1, "one send in all");
}
