//! Known create outcomes require payload before a caller can interpret them.

use restate_szamlazz::contract::{ConflictReason, CreateResponse, CreateResponseView};
use serde_json::{Value, json};

fn response(outcome: &str, fields: Value) -> CreateResponse {
    let Value::Object(fields) = fields else {
        panic!("response fields must be an object");
    };
    let mut wire =
        json!({"outcome": outcome, "kind": "invoice", "external_id": "acct:ORD:invoice"});
    wire.as_object_mut().expect("object").extend(fields);
    serde_json::from_value(wire).expect("open wire record")
}

#[test]
fn document_outcomes_require_a_nonblank_number_without_normalizing_it() {
    for outcome in ["issued", "already_issued", "reconciled", "reversed"] {
        for fields in [
            json!({}),
            json!({"invoice_number": null}),
            json!({"invoice_number": ""}),
            json!({"invoice_number": " \t"}),
        ] {
            assert_eq!(
                response(outcome, fields)
                    .validated()
                    .expect_err(outcome)
                    .field,
                "invoice_number"
            );
        }
        let response = response(outcome, json!({"invoice_number": " provider:É-1 "}));
        let number = match response.validated().expect(outcome) {
            CreateResponseView::Issued { invoice_number }
            | CreateResponseView::AlreadyIssued { invoice_number }
            | CreateResponseView::Reconciled { invoice_number }
            | CreateResponseView::Reversed { invoice_number, .. } => invoice_number,
            other => panic!("unexpected view {other:?}"),
        };
        assert_eq!(number, " provider:É-1 ");
        assert!(response.storno_number.is_none());
    }
}

#[test]
fn conflict_and_rejection_require_their_own_evidence() {
    for fields in [
        json!({}),
        json!({"conflict_reason": null}),
        json!({"conflict_reason": " "}),
    ] {
        assert_eq!(
            response("conflict", fields)
                .validated()
                .expect_err("reason")
                .field,
            "conflict_reason"
        );
    }
    for reason in [
        ConflictReason::Live,
        ConflictReason::Other("future_reason".into()),
    ] {
        let conflict = response("conflict", json!({"conflict_reason": reason}));
        assert_eq!(
            conflict
                .validated()
                .expect("other conflict requires no vendor answer"),
            CreateResponseView::Conflict { reason: &reason }
        );
    }
    for (outcome, reason) in [
        ("rejected", Value::Null),
        ("conflict", json!("duplicate_order_number")),
    ] {
        for (fields, missing) in [
            (json!({}), "code"),
            (json!({"code": " "}), "code"),
            (json!({"code": "71"}), "message"),
            (json!({"code": "71", "message": null}), "message"),
        ] {
            let mut fields = fields;
            fields["conflict_reason"] = reason.clone();
            assert_eq!(
                response(outcome, fields)
                    .validated()
                    .expect_err("answer")
                    .field,
                missing
            );
        }
        let complete = response(
            outcome,
            json!({"conflict_reason": reason, "code": "71", "message": ""}),
        );
        assert!(
            complete.validated().is_ok(),
            "an empty but reported message is preserved"
        );
        assert!(
            complete.existing_number.is_none(),
            "duplicate refusal can lack a found number"
        );
    }
}

#[test]
fn duplicate_order_number_view_exposes_the_checked_answer_and_optional_number() {
    for code in ["71", "152"] {
        for message in ["", " Duplicate order: árv íz "] {
            for existing_number in [None, Some(" provider:É-1 ")] {
                let response = response(
                    "conflict",
                    json!({
                        "conflict_reason": "duplicate_order_number",
                        "code": code,
                        "message": message,
                        "existing_number": existing_number,
                    }),
                );
                assert_eq!(
                    response.validated().expect("complete duplicate refusal"),
                    CreateResponseView::DuplicateOrderNumber {
                        code,
                        message,
                        existing_number
                    }
                );
                let wire = serde_json::to_value(&response).expect("encode");
                assert_eq!(wire["outcome"], "conflict");
                assert_eq!(wire["conflict_reason"], "duplicate_order_number");
            }
        }
    }
}

#[test]
fn future_outcomes_are_unclassified_and_retain_known_payload() {
    let response = response(
        " future/🧾 ",
        json!({"invoice_number": "SZ-1", "code": "new", "message": "detail"}),
    );
    assert_eq!(
        response.validated().expect("open outcome"),
        CreateResponseView::Other {
            outcome: " future/🧾 "
        }
    );
    let wire = serde_json::to_value(&response).expect("encode");
    let decoded: CreateResponse = serde_json::from_value(wire).expect("decode");
    assert_eq!(decoded, response);
}
