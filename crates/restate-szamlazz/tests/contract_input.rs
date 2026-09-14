//! Input decoding at both the public Serde boundary and SDK Body ingress.
use std::fmt::Debug;

use bytes::Bytes;
use restate_sdk::serde::{Deserialize as SdkDeserialize, Serialize as SdkSerialize};
use restate_szamlazz::contract::recovery::{
    AttestedCompletion, RecoveryEvidence, RecoveryRequest, UnresolvedObservation, UnresolvedWrite,
    WriteOperation,
};
use restate_szamlazz::contract::{
    Amounts, BuyerInput, CorrectRequest, CreateOptions, CreateRequest, CreditEntryInput,
    DeleteMode, DeleteProformaRequest, DocumentInput, DocumentOverrides, ExchangeRateInput,
    LineItemInput, PostalAddressInput, ProformaLink, QueryRequest, QueryTaxpayerRequest, Reissue,
    SetCreditEntriesRequest, StornoRequest,
};
use restate_szamlazz::service::Body;
use rust_decimal::{Decimal, dec};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

fn ingress<T: DeserializeOwned + Serialize>(wire: &str) -> Result<T, serde_json::Error> {
    let body =
        <Body<T> as SdkDeserialize>::deserialize(&mut Bytes::copy_from_slice(wire.as_bytes()))
            .expect("Body defers malformed-input errors");
    // Body keeps its decoding result private. SDK serialization returns that
    // same error for a malformed body, or the actual decoded request on success.
    serde_json::from_slice(&SdkSerialize::serialize(&body)?)
}

fn accepted<T: DeserializeOwned + Serialize + Debug + PartialEq>(wire: &str) -> T {
    let direct: T = serde_json::from_str(wire).expect(wire);
    assert_eq!(ingress::<T>(wire).expect(wire), direct);
    direct
}

fn refused<T: DeserializeOwned + Serialize + Debug>(wire: &str, diagnostic: &str) {
    for error in [
        serde_json::from_str::<T>(wire).expect_err(wire),
        ingress::<T>(wire).expect_err(wire),
    ] {
        assert!(error.to_string().contains(diagnostic), "{wire}: {error}");
    }
}

fn buyer() -> Value {
    json!({"name":"A", "zip":"1", "city":"B", "address":"C"})
}

fn item() -> Value {
    json!({"name":"x", "quantity":"1", "unit":"db", "unit_price":"100", "vat_rate":"27"})
}

fn document() -> Value {
    json!({
        "buyer": buyer(), "items": [item()], "fulfillment_date":"2026-09-14",
        "due_date":"2026-09-21", "payment_method":"transfer"
    })
}

fn marker() -> Value {
    json!({
        "version":1, "token":"inv-1", "owner_invocation":"inv-1",
        "created_at":"2026-09-14T12:00:00Z", "scope":null, "order":"ORD-1",
        "namespace":"test", "external_id":"test:ORD-1:invoice", "account_id":"a",
        "endpoint":"https://example.test/", "credential_ref":"a",
        "operation":{"type":"create", "kind":"invoice", "expected_number":null, "corrected_number":null}
    })
}

fn recovery() -> Value {
    json!({"operator":"operator-1", "marker":marker(), "evidence":{"type":"document", "number":"SZ-1"}})
}

/// Supply field order explicitly: a JSON object's iteration order is not the
/// positional order Serde derives for a struct. Full arrays exercise the old
/// loophole, rather than merely failing for a missing required field.
fn object_boundary<T>(minimal: Value, fields: &[&str])
where
    T: DeserializeOwned + Serialize + Debug + PartialEq,
{
    let decoded = accepted::<T>(&minimal.to_string());
    let full = serde_json::to_value(decoded).expect("serialize");
    let positional = Value::Array(fields.iter().map(|field| full[*field].clone()).collect());
    for wire in [
        positional.to_string(),
        "[]".into(),
        "null".into(),
        "false".into(),
        "17".into(),
        "\"object\"".into(),
    ] {
        refused::<T>(&wire, "object");
    }
    let mut unknown = minimal;
    unknown["unexpected"] = json!(true);
    refused::<T>(&unknown.to_string(), "unknown field `unexpected`");
    let field = fields[0];
    let duplicate = format!(
        "{{{field:?}:{},{rest}",
        full[field],
        rest = &full.to_string()[1..]
    );
    refused::<T>(&duplicate, &format!("duplicate field `{field}`"));
}

#[test]
#[allow(clippy::too_many_lines)] // One inventory of every named input boundary.
fn every_named_input_record_requires_an_object_and_keeps_its_field_contract() {
    object_boundary::<QueryRequest>(json!({"selector":{"invoice_number":"SZ-1"}}), &["selector"]);
    object_boundary::<QueryTaxpayerRequest>(json!({"tax_number":"12345678"}), &["tax_number"]);
    object_boundary::<SetCreditEntriesRequest>(
        json!({"invoice_number":"SZ-1", "entries":[]}),
        &["invoice_number", "entries", "additive"],
    );
    object_boundary::<CreditEntryInput>(
        json!({"date":"2026-09-14", "title":"cash", "amount":"1"}),
        &["date", "title", "amount", "comment"],
    );
    object_boundary::<CreateRequest>(json!({"document":document()}), &["document", "options"]);
    object_boundary::<CreateOptions>(json!({}), &["reissue", "proforma"]);
    object_boundary::<Reissue>(json!({"expected_number":"SZ-1"}), &["expected_number"]);
    object_boundary::<CorrectRequest>(
        json!({"invoice_number":"SZ-1", "correction_id":"c-1", "document":document()}),
        &["invoice_number", "correction_id", "document"],
    );
    object_boundary::<DocumentInput>(
        document(),
        &[
            "buyer",
            "items",
            "fulfillment_date",
            "due_date",
            "payment_method",
            "paid",
            "comment",
            "issue_date",
            "overrides",
            "expected_totals",
        ],
    );
    object_boundary::<DocumentOverrides>(
        json!({}),
        &[
            "language",
            "currency",
            "exchange_rate",
            "template",
            "send_email",
            "e_invoice",
            "number_prefix",
        ],
    );
    object_boundary::<ExchangeRateInput>(json!({"bank":"MNB"}), &["bank", "rate"]);
    object_boundary::<BuyerInput>(
        buyer(),
        &[
            "name",
            "zip",
            "city",
            "address",
            "country",
            "email",
            "tax_number",
            "eu_tax_number",
            "group_id",
            "taxpayer_status",
            "phone",
            "comment",
            "postal_address",
            "id",
        ],
    );
    object_boundary::<PostalAddressInput>(
        json!({}),
        &["name", "country", "zip", "city", "address"],
    );
    object_boundary::<LineItemInput>(
        item(),
        &[
            "name",
            "quantity",
            "unit",
            "unit_price",
            "vat_rate",
            "amounts",
            "id",
            "comment",
        ],
    );
    object_boundary::<Amounts>(
        json!({"net":"100", "vat":"27", "gross":"127"}),
        &["net", "vat", "gross"],
    );
    object_boundary::<StornoRequest>(
        json!({"invoice_number":"SZ-1"}),
        &["invoice_number", "comment", "buyer_email"],
    );
    object_boundary::<DeleteProformaRequest>(
        json!({"expected_number":"D-1"}),
        &["expected_number", "mode", "force"],
    );
    object_boundary::<UnresolvedWrite>(
        marker(),
        &[
            "version",
            "token",
            "owner_invocation",
            "created_at",
            "scope",
            "order",
            "namespace",
            "external_id",
            "account_id",
            "endpoint",
            "credential_ref",
            "operation",
        ],
    );
    object_boundary::<RecoveryRequest>(recovery(), &["operator", "marker", "evidence"]);
}

#[test]
fn nested_record_boundaries_refuse_positional_arrays() {
    let mut request =
        json!({"document":document(), "options":{"reissue":{"expected_number":"SZ-1"}}});
    request["document"]["buyer"]["postal_address"] = json!({});
    request["document"]["overrides"] = json!({"exchange_rate":{"bank":"MNB"}});
    request["document"]["items"][0]["amounts"] = json!({"net":"100", "vat":"27", "gross":"127"});
    request["document"]["expected_totals"] = request["document"]["items"][0]["amounts"].clone();
    accepted::<CreateRequest>(&request.to_string());
    for (path, array) in [
        (
            "/document",
            json!([
                buyer(),
                [],
                "2026-09-14",
                "2026-09-21",
                "cash",
                false,
                null,
                null,
                {},
                null
            ]),
        ),
        ("/options", json!([null, "auto"])),
        ("/options/reissue", json!(["SZ-1"])),
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
    ] {
        let mut invalid = request.clone();
        *invalid.pointer_mut(path).expect(path) = array;
        refused::<CreateRequest>(&invalid.to_string(), "object");
    }
    refused::<SetCreditEntriesRequest>(
        r#"{"invoice_number":"SZ-1","entries":[["2026-09-14","cash","1",null]]}"#,
        "object",
    );
    let mut invalid = recovery();
    invalid["marker"] = json!([1, "inv-1", "inv-1", "now", null, "ORD-1", "test", "test:ORD-1:invoice", "a", "https://example.test/", "a", {"type":"storno", "number":"SZ-1"}]);
    refused::<RecoveryRequest>(&invalid.to_string(), "object");
}

#[test]
fn defaults_and_actual_vec_arrays_still_work() {
    let request =
        accepted::<CreateRequest>(&json!({"document":document(), "options":{}}).to_string());
    assert_eq!(request.options, CreateOptions::default());
    assert_eq!(request.options.proforma, ProformaLink::Auto);
    assert_eq!(request.document.overrides, DocumentOverrides::default());
    assert!(!request.document.paid);
    assert!(request.document.issue_date.is_none());
    assert!(request.document.buyer.email.is_none());
    assert!(request.document.items[0].amounts.is_none());
    assert_eq!(
        accepted::<PostalAddressInput>("{}"),
        PostalAddressInput::default()
    );
    assert!(
        accepted::<ExchangeRateInput>(r#"{"bank":"MNB"}"#)
            .rate
            .is_none()
    );
    let deletion = accepted::<DeleteProformaRequest>(r#"{"expected_number":"D-1"}"#);
    assert_eq!(deletion.mode, DeleteMode::NamespaceOwned);
    assert!(!deletion.force);
    let storno = accepted::<StornoRequest>(r#"{"invoice_number":"SZ-1"}"#);
    assert!(storno.comment.is_none() && storno.buyer_email.is_none());

    for items in [json!([]), json!([item(), item()])] {
        let mut input = document();
        input["items"] = items.clone();
        assert_eq!(
            accepted::<DocumentInput>(&input.to_string()).items.len(),
            items.as_array().expect("items array").len()
        );
    }
    for entries in [
        json!([]),
        json!([{"date":"2026-09-14", "title":"cash", "amount":"1"}]),
    ] {
        let input = json!({"invoice_number":"SZ-1", "entries":entries});
        let decoded = accepted::<SetCreditEntriesRequest>(&input.to_string());
        assert!(!decoded.additive);
        assert_eq!(
            decoded.entries.len(),
            entries.as_array().expect("entries array").len()
        );
    }
    for wrong in [json!({}), json!(null)] {
        let mut input = document();
        input["items"] = wrong.clone();
        refused::<DocumentInput>(&input.to_string(), "sequence");
        refused::<SetCreditEntriesRequest>(
            &json!({"invoice_number":"SZ-1", "entries":wrong}).to_string(),
            "sequence",
        );
    }
}

#[test]
fn precise_numeric_tokens_survive_every_monetary_input_path() {
    let exact = "7922816251426433759354395033.5";
    let tiny = "0.0000000000000000000000000001";
    let wire = r#"{"document":{"buyer":BUYER,"items":[{"name":"x","quantity":TINY,"unit":"db","unit_price":EXACT,"vat_rate":"27","amounts":{"net":EXACT,"vat":TINY,"gross":EXACT}}],"fulfillment_date":"2026-09-14","due_date":"2026-09-21","payment_method":"cash","overrides":{"exchange_rate":{"bank":"MNB","rate":EXACT}},"expected_totals":{"net":EXACT,"vat":TINY,"gross":EXACT}}}"#
        .replace("BUYER", &buyer().to_string())
        .replace("EXACT", exact)
        .replace("TINY", tiny);
    let request = accepted::<CreateRequest>(&wire);
    let expected: Decimal = exact.parse().expect("exact decimal");
    let line = &request.document.items[0];
    assert_eq!(
        line.quantity,
        tiny.parse::<Decimal>().expect("tiny decimal")
    );
    assert_eq!(line.unit_price, expected);
    assert_eq!(line.amounts.as_ref().expect("line amounts").net, expected);
    assert_eq!(
        request
            .document
            .expected_totals
            .as_ref()
            .expect("totals")
            .gross,
        expected
    );
    assert_eq!(
        request
            .document
            .overrides
            .exchange_rate
            .as_ref()
            .expect("exchange rate")
            .rate,
        Some(expected)
    );
    let credit = accepted::<SetCreditEntriesRequest>(&format!(
        r#"{{"invoice_number":"SZ-1","entries":[{{"date":"2026-09-14","title":"cash","amount":{exact}}}]}}"#
    ));
    assert_eq!(credit.entries[0].amount, expected);
    assert_eq!(
        accepted::<Amounts>(r#"{"net":1e-28,"vat":"0","gross":1e-28}"#).net,
        dec!(0.0000000000000000000000000001)
    );
    refused::<CreateRequest>(&wire.replace(tiny, "1e-29"), "exactly representable");
    refused::<CreateRequest>(
        &wire.replace(
            &format!("\"quantity\":{tiny}"),
            &format!("\"quantity\":{tiny},\"quantity\":1"),
        ),
        "duplicate field `quantity`",
    );
    refused::<CreateRequest>(
        &wire.replace(
            &format!("\"net\":{exact}"),
            &format!("\"net\":{exact},\"net\":0"),
        ),
        "duplicate field `net`",
    );
    refused::<SetCreditEntriesRequest>(
        r#"{"invoice_number":"SZ-1","entries":[{"date":"2026-09-14","title":"cash","amount":1,"amount":2}]}"#,
        "duplicate field `amount`",
    );
}

#[test]
fn known_tagged_recovery_payloads_require_object_shapes() {
    for operation in [
        json!({"type":"create", "kind":"invoice"}),
        json!({"type":"storno", "number":"SZ-1"}),
        json!({"type":"delete", "number":"D-1"}),
    ] {
        accepted::<WriteOperation>(&operation.to_string());
        let mut invalid = operation.clone();
        invalid["unexpected"] = json!(true);
        refused::<WriteOperation>(&invalid.to_string(), "unknown field");
        let mut request = recovery();
        request["marker"]["operation"] = operation;
        accepted::<RecoveryRequest>(&request.to_string());
    }
    for array in [
        json!(["create", "invoice", null, null]),
        json!(["storno", "SZ-1"]),
        json!(["delete", "D-1"]),
    ] {
        refused::<WriteOperation>(&array.to_string(), "object");
        let mut request = recovery();
        request["marker"]["operation"] = array;
        refused::<RecoveryRequest>(&request.to_string(), "object");
    }
    for completion in [
        json!({"type":"issued", "number":"SZ-1"}),
        json!({"type":"reversed", "number":"SS-1"}),
        json!({"type":"deleted", "number":"D-1"}),
    ] {
        accepted::<AttestedCompletion>(&completion.to_string());
        let array = json!([completion["type"], completion["number"]]);
        refused::<AttestedCompletion>(&array.to_string(), "object");
        let mut input = recovery();
        input["evidence"] = json!({"type":"completed", "audit_reference":"incident-1", "completion":completion, "completed_and_cannot_execute_later":true});
        accepted::<RecoveryRequest>(&input.to_string());
        input["evidence"]["completion"] = array;
        refused::<RecoveryRequest>(&input.to_string(), "object");
    }
    for evidence in [
        json!({"type":"document", "number":"SZ-1"}),
        json!({"type":"not_executed", "audit_reference":"incident-1", "did_not_execute_and_cannot_execute_later":true}),
        json!({"type":"completed", "audit_reference":"incident-1", "completion":{"type":"issued", "number":"SZ-1"}, "completed_and_cannot_execute_later":true}),
    ] {
        accepted::<RecoveryEvidence>(&evidence.to_string());
        let mut request = recovery();
        request["evidence"] = evidence;
        accepted::<RecoveryRequest>(&request.to_string());
    }
    for array in [
        json!(["document", "SZ-1"]),
        json!(["not_executed", "incident-1", true]),
        json!(["completed", "incident-1", {"type":"issued", "number":"SZ-1"}, true]),
    ] {
        refused::<RecoveryEvidence>(&array.to_string(), "object");
        let mut input = recovery();
        input["evidence"] = array;
        refused::<RecoveryRequest>(&input.to_string(), "object");
    }
    refused::<WriteOperation>(
        r#"{"type":"storno","number":"SZ-1","number":"SZ-2"}"#,
        "duplicate field `number`",
    );
    refused::<RecoveryEvidence>(
        r#"{"type":"document","number":"SZ-1","number":"SZ-2"}"#,
        "duplicate field `number`",
    );
    refused::<AttestedCompletion>(
        r#"{"type":"issued","number":"SZ-1","number":"SZ-2"}"#,
        "duplicate field `number`",
    );
    refused::<WriteOperation>(
        r#"{"type":"storno","type":"delete","number":"SZ-1"}"#,
        "duplicate field `type`",
    );
}

#[test]
fn persisted_markers_fail_closed_while_newer_observations_remain_opaque() {
    for (path, value) in [
        ("/version", json!(2)),
        ("/operation/type", json!("future_write")),
    ] {
        let mut newer = marker();
        *newer.pointer_mut(path).expect("marker field") = value;
        assert!(serde_json::from_value::<UnresolvedWrite>(newer.clone()).is_err());
        let mut input = recovery();
        input["marker"] = newer.clone();
        refused::<RecoveryRequest>(
            &input.to_string(),
            if path == "/version" {
                "unsupported"
            } else {
                "unknown variant"
            },
        );
        let observation = json!({"state":"unresolved", "marker":newer});
        let decoded: UnresolvedObservation =
            serde_json::from_value(observation.clone()).expect("newer observation");
        assert!(matches!(&decoded, UnresolvedObservation::Other { .. }));
        assert_eq!(
            serde_json::to_value(decoded).expect("serialize"),
            observation
        );
    }
    let mut newer = marker();
    newer["future_intent"] = json!({"must_preserve":true});
    refused::<UnresolvedWrite>(&newer.to_string(), "unknown field");
    let observation = json!({"state":"unresolved", "marker":newer});
    let decoded: UnresolvedObservation =
        serde_json::from_value(observation.clone()).expect("newer observation");
    assert!(matches!(&decoded, UnresolvedObservation::Other { .. }));
    assert_eq!(
        serde_json::to_value(decoded).expect("serialize"),
        observation
    );
}

#[test]
fn tagged_payloads_stay_closed_and_validate_required_fields() {
    for wire in [
        r#"{"type":"create"}"#,
        r#"{"type":"storno"}"#,
        r#"{"type":"delete"}"#,
    ] {
        refused::<WriteOperation>(wire, "missing field");
    }
    for tag in ["issued", "reversed", "deleted"] {
        refused::<AttestedCompletion>(&format!(r#"{{"type":"{tag}"}}"#), "missing field");
        refused::<AttestedCompletion>(
            &format!(r#"{{"type":"{tag}","number":["SZ-1"]}}"#),
            "invalid type",
        );
        refused::<AttestedCompletion>(
            &format!(r#"{{"type":"{tag}","number":"SZ-1","future":true}}"#),
            "unknown field",
        );
    }
    for tag in ["document", "not_executed", "completed"] {
        refused::<RecoveryEvidence>(&format!(r#"{{"type":"{tag}"}}"#), "missing field");
    }
    refused::<RecoveryEvidence>(
        r#"{"type":"not_executed","audit_reference":"i","did_not_execute_and_cannot_execute_later":false}"#,
        "must attest",
    );
    refused::<RecoveryEvidence>(
        r#"{"type":"completed","audit_reference":"i","completion":{"type":"issued","number":"SZ-1"},"completed_and_cannot_execute_later":false}"#,
        "must attest",
    );
    refused::<RecoveryEvidence>(
        r#"{"type":"document","number":"SZ-1","future":true}"#,
        "unknown field",
    );
    refused::<RecoveryEvidence>(
        r#"{"type":"not_executed","audit_reference":"i","did_not_execute_and_cannot_execute_later":true,"future":true}"#,
        "unknown field",
    );
    refused::<RecoveryEvidence>(
        r#"{"type":"completed","audit_reference":"i","completion":{"type":"issued","number":"SZ-1"},"completed_and_cannot_execute_later":true,"future":true}"#,
        "unknown field",
    );
}

#[cfg(feature = "schemars")]
#[test]
fn object_inputs_keep_their_public_discovery_schemas() {
    fn check<T: schemars::JsonSchema>(name: &str, properties: &[&str]) {
        let schema = serde_json::to_value(schemars::schema_for!(T)).expect("schema");
        assert_eq!(schema["title"], name);
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);
        for field in properties {
            assert!(
                schema["properties"][field].is_object(),
                "{name}.{field}: {schema}"
            );
        }
    }
    check::<QueryRequest>("QueryRequest", &["selector"]);
    check::<QueryTaxpayerRequest>("QueryTaxpayerRequest", &["tax_number"]);
    check::<SetCreditEntriesRequest>("SetCreditEntriesRequest", &["entries", "additive"]);
    check::<CreditEntryInput>("CreditEntryInput", &["date", "title", "amount"]);
    check::<CreateRequest>("CreateRequest", &["document", "options"]);
    check::<CreateOptions>("CreateOptions", &["reissue", "proforma"]);
    check::<Reissue>("Reissue", &["expected_number"]);
    check::<CorrectRequest>("CorrectRequest", &["correction_id", "document"]);
    check::<DocumentInput>("DocumentInput", &["buyer", "items", "expected_totals"]);
    check::<DocumentOverrides>("DocumentOverrides", &["exchange_rate", "currency"]);
    check::<ExchangeRateInput>("ExchangeRateInput", &["bank", "rate"]);
    check::<BuyerInput>("BuyerInput", &["name", "postal_address"]);
    check::<PostalAddressInput>("PostalAddressInput", &["name", "address"]);
    check::<LineItemInput>("LineItemInput", &["quantity", "unit_price", "amounts"]);
    check::<Amounts>("Amounts", &["net", "vat", "gross"]);
    check::<StornoRequest>("StornoRequest", &["invoice_number", "buyer_email"]);
    check::<DeleteProformaRequest>(
        "DeleteProformaRequest",
        &["expected_number", "mode", "force"],
    );
    check::<UnresolvedWrite>("UnresolvedWrite", &["version", "operation"]);
    check::<RecoveryRequest>("RecoveryRequest", &["operator", "marker", "evidence"]);
}
