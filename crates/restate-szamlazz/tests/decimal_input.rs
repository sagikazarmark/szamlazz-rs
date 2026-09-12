//! Financial input must be exact before currency rounding or any durable work.
use restate_szamlazz::contract::{CreditEntryInput, ExchangeRateInput, LineItemInput};
use restate_szamlazz::szamlazz_agent::Currency;

#[test]
fn handler_json_boundary_keeps_exact_tokens_and_refuses_number_objects() {
    use restate_sdk::serde::{Deserialize as _, Serialize as _};
    use restate_szamlazz::service::Body;
    fn decode(text: &str) -> Body<LineItemInput> {
        Body::deserialize(&mut bytes::Bytes::copy_from_slice(text.as_bytes()))
            .expect("kept verdict")
    }
    for token in [
        "9007199254740993.5",
        "1e-2",
        "0.4999999999999999999999999999",
        "79228162514264337593543950335",
    ] {
        let text = format!(
            r#"{{"name":"$serde_json::private::Number","quantity":{token},"unit":"db","unit_price":"1","vat_rate":"AAM"}}"#
        );
        let body = decode(&text);
        let input: LineItemInput =
            serde_json::from_slice(&body.serialize().expect("valid body")).expect("decoded");
        assert_eq!(
            input.quantity,
            restate_szamlazz::szamlazz_agent::parse_decimal(token).expect("exact")
        );
    }
    for token in [
        r#"{"$serde_json::private::Number":"1"}"#,
        r#"{"\u0024serde_json::private::Number":"1"}"#,
        "[1]",
        "{}",
    ] {
        let text = format!(
            r#"{{"name":"x","quantity":{token},"unit":"db","unit_price":"1","vat_rate":"AAM"}}"#
        );
        assert!(decode(&text).serialize().is_err(), "{text}");
    }
    for text in [
        r#"{"name":"x","quantity":1,"quantity":2,"unit":"db","unit_price":1,"vat_rate":"AAM"}"#
            .to_owned(),
        format!("{}0{}", "[".repeat(130), "]".repeat(130)),
    ] {
        assert!(decode(&text).serialize().is_err());
    }
}

fn check<T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(
    value: &T,
    paths: &[&str],
    token: &str,
) {
    use restate_sdk::serde::{Deserialize as _, Json, Serialize as _};
    // Value's deserializer accepts strings even when Decimal's text
    // deserializer has been unified to expect f64. Exercise the bytes that
    // callers and the SDK actually replay, before inspecting JSON shape.
    let text = serde_json::to_string(value).expect("JSON text");
    assert_eq!(
        &serde_json::from_str::<T>(&text).expect("text replay"),
        value
    );
    assert_eq!(
        &serde_json::from_slice::<T>(text.as_bytes()).expect("byte replay"),
        value
    );
    let mut journal = Json(value).serialize().expect("SDK journal bytes");
    assert_eq!(
        &Json::<T>::deserialize(&mut journal).expect("SDK replay").0,
        value
    );
    let encoded = serde_json::to_value(value).expect("serialize");
    for path in paths {
        assert_eq!(
            encoded.pointer(path),
            Some(&serde_json::Value::String(token.into())),
            "{path}: {encoded}"
        );
        // The same exact rule applies to response and journal fields, not
        // just request inputs. Test every leaf, including vector members.
        let mut numeric = encoded.clone();
        *numeric.pointer_mut(path).expect("money leaf") =
            serde_json::from_str(token).expect("number");
        assert_eq!(
            &serde_json::from_slice::<T>(&serde_json::to_vec(&numeric).expect("numeric bytes"))
                .expect("exact numeric input"),
            value
        );
        for invalid in ["1e-29", "79228162514264337593543950336"] {
            for replacement in [
                serde_json::Value::String(invalid.into()),
                serde_json::from_str(invalid).expect("number"),
            ] {
                let mut invalid = encoded.clone();
                *invalid.pointer_mut(path).expect("money leaf") = replacement;
                let mut bytes =
                    bytes::Bytes::from(serde_json::to_vec(&invalid).expect("invalid bytes"));
                assert!(
                    Json::<T>::deserialize(&mut bytes).is_err(),
                    "{path}: {invalid}"
                );
            }
        }
    }
    assert_eq!(
        &serde_json::from_value::<T>(encoded).expect("round trip"),
        value
    );
}

#[test]
fn public_money_replays_from_json_text_bytes_and_sdk() {
    use restate_szamlazz::{contract::*, identity::IssuedKind};
    use serde_json::json;
    let token = "9007199254740993.5";
    let amount = szamlazz_agent::parse_decimal(token).expect("decimal");
    check(
        &LineItemInput::new("x", amount, "db", amount, "AAM"),
        &["/quantity", "/unit_price"],
        token,
    );
    check(
        &serde_json::from_value::<ExchangeRateInput>(json!({"bank":"MNB", "rate":token}))
            .expect("exchange rate"),
        &["/rate"],
        token,
    );
    check(
        &CreditEntryInput::new(jiff::civil::date(2026, 9, 11), PaymentMethod::Cash, amount),
        &["/amount"],
        token,
    );
    check(&CreditEntryRecord::new(amount), &["/amount"], token);
    let mut query = QueryResponse::new("SZ-1", "SZ");
    query.net_total = Some(amount);
    query.vat_total = Some(amount);
    query.gross_total = Some(amount);
    query.outstanding = Some(amount);
    query.credit_entries = vec![CreditEntryRecord::new(amount)];
    check(
        &query,
        &[
            "/net_total",
            "/vat_total",
            "/gross_total",
            "/outstanding",
            "/credit_entries/0/amount",
        ],
        token,
    );
    let create = CreateResponse::new(CreateOutcome::Issued, IssuedKind::Invoice, "ns:o:invoice")
        .with_net_total(amount)
        .with_gross_total(amount)
        .with_outstanding(amount);
    check(
        &create,
        &["/net_total", "/gross_total", "/outstanding"],
        token,
    );
    let mut status = DocumentStatus::new("SZ-1", DocumentState::Live);
    status.gross = Some(amount);
    status.net = Some(amount);
    status.credit_entries = vec![amount];
    check(&status, &["/gross", "/net", "/credit_entries/0"], token);
    let mut registered = SetCreditEntriesResponse::new("SZ-1");
    registered.outstanding = Some(amount);
    registered.gross_total = Some(amount);
    check(&registered, &["/outstanding", "/gross_total"], token);
}

#[test]
fn journal_money_replays_from_json_text_bytes_and_sdk() {
    use restate_szamlazz::gateway;
    use serde_json::json;
    use szamlazz_agent::wire::AgentRequest;
    let token = "9007199254740993.5";
    let amount = szamlazz_agent::parse_decimal(token).expect("decimal");
    check(
        &gateway::SetCreditEntriesOutcome::Done {
            outstanding: Some(amount),
            gross: Some(amount),
        },
        &["/Done/outstanding", "/Done/gross"],
        token,
    );

    // Project the same XML facts as a real operation, then exercise the exact
    // journal encoding of both projections (including the nested credit entry).
    let xml = common::Doc {
        net: token,
        vat: token,
        gross: token,
        credit_entries: &[common::CreditRecord::transfer(token)],
        ..common::Doc::new("SZ-1", "SZ")
    }
    .xml();
    let found = szamlazz_agent::ops::query_xml::QueryInvoiceXml::new(
        szamlazz_agent::InvoiceSelector::InvoiceNumber("SZ-1".into()),
    )
    .parse(&szamlazz_agent::wire::RawResponse::new::<&str, &str>(
        [],
        xml.into_bytes(),
    ))
    .expect("queried document XML");
    let found = gateway::FoundDocument::from(found);
    check(&found.credit_entries[0], &["/amount"], token);
    check(
        &found,
        &[
            "/net_total",
            "/vat_total",
            "/gross_total",
            "/credit_entries/0/amount",
        ],
        token,
    );
    check(
        &gateway::QueryOutcome::Found(Box::new(found)),
        &[
            "/Found/net_total",
            "/Found/vat_total",
            "/Found/gross_total",
            "/Found/credit_entries/0/amount",
        ],
        token,
    );
    let created: szamlazz_agent::ops::invoice::CreatedInvoice = serde_json::from_value(json!({"invoice_number":"SZ-1", "notification_delivery_failed":false, "net_total":token, "gross_total":token, "outstanding":token})).expect("created document");
    check(
        &gateway::IssuedDocument::from(created),
        &["/net_total", "/gross_total", "/outstanding"],
        token,
    );
}

mod common;

#[cfg(feature = "schemars")]
#[test]
fn decimal_discovery_accepts_the_runtime_string_grammar() {
    let line = serde_json::to_value(schemars::schema_for!(LineItemInput)).expect("schema");
    let credit = serde_json::to_value(schemars::schema_for!(CreditEntryInput)).expect("schema");
    let exchange = serde_json::to_value(schemars::schema_for!(ExchangeRateInput)).expect("schema");
    for schema in [
        &line["properties"]["quantity"],
        &line["properties"]["unit_price"],
        &credit["properties"]["amount"],
        &exchange["properties"]["rate"],
    ] {
        let pattern = regex::Regex::new(schema["pattern"].as_str().expect("input decimal pattern"))
            .expect("valid regex");
        let forbidden = regex::Regex::new(schema["not"]["pattern"].as_str().expect("line endings"))
            .expect("valid regex");
        for text in [
            "1e-2", "1E+2", "+1", ".5", "1.", "-.5e+1", "001.20", "0", "-0",
        ] {
            assert!(pattern.is_match(text), "schema must accept {text}");
            assert!(!forbidden.is_match(text));
            let body = serde_json::json!({"bank":"MNB", "rate":text});
            serde_json::from_value::<ExchangeRateInput>(body)
                .expect("runtime accepts schema example");
        }
        for text in [
            "", "+", ".", "1e", "1e--2", " 1", "1 ", "1\n", "NaN", "1,2", "１２",
        ] {
            assert!(
                !pattern.is_match(text) || forbidden.is_match(text),
                "schema must refuse {text:?}"
            );
            let body = serde_json::json!({"bank":"MNB", "rate":text});
            assert!(serde_json::from_value::<ExchangeRateInput>(body).is_err());
        }
    }
    assert_eq!(
        exchange["properties"]["rate"]["type"],
        serde_json::json!(["string", "number", "null"])
    );
}

#[test]
fn public_inputs_compose_inside_buffered_serde_wrappers() {
    #[derive(Debug, serde::Deserialize)]
    #[serde(untagged)]
    enum Input {
        Line(LineItemInput),
        Text(String),
    }
    #[derive(Debug, serde::Deserialize)]
    #[serde(tag = "type")]
    enum Tagged {
        Line(LineItemInput),
    }
    for token in [
        "\"0.4999999999999999999999999999\"",
        "0.4999999999999999999999999999",
    ] {
        let line = format!(
            r#"{{"name":"x","quantity":1,"unit":"db","unit_price":{token},"vat_rate":"AAM"}}"#
        );
        let input: Input = serde_json::from_str(&line).expect("untagged wrapper");
        let Input::Line(input) = input else {
            panic!("wrong variant: {input:?}")
        };
        assert_eq!(
            input.unit_price.to_string(),
            "0.4999999999999999999999999999"
        );
        let tagged = line.replacen('{', r#"{"type":"Line","#, 1);
        let Tagged::Line(input) = serde_json::from_str(&tagged).expect("tagged wrapper");
        assert_eq!(
            input.unit_price.to_string(),
            "0.4999999999999999999999999999"
        );
    }
    let Input::Text(text) = serde_json::from_str(r#""text""#).expect("text") else {
        panic!("text")
    };
    assert_eq!(text, "text");
}

#[test]
fn unrepresentable_inputs_are_refused_on_every_input_channel() {
    for token in [
        "0.49999999999999999999999999999",
        "1e-29",
        "79228162514264337593543950336",
        "1e999999999999999999999999",
    ] {
        for value in [token.to_owned(), format!("\"{token}\"")] {
            for field in ["quantity", "unit_price"] {
                let other = if field == "quantity" {
                    "unit_price"
                } else {
                    "quantity"
                };
                let body = format!(
                    r#"{{"name":"x","unit":"db","vat_rate":"AAM","{field}":{value},"{other}":"1"}}"#
                );
                assert!(
                    serde_json::from_str::<LineItemInput>(&body).is_err(),
                    "{body}"
                );
            }
            let body = format!(r#"{{"date":"2026-09-11","title":"transfer","amount":{value}}}"#);
            assert!(
                serde_json::from_str::<CreditEntryInput>(&body).is_err(),
                "{body}"
            );
            let body = format!(r#"{{"bank":"MNB","rate":{value}}}"#);
            assert!(
                serde_json::from_str::<ExchangeRateInput>(&body).is_err(),
                "{body}"
            );
        }
    }
}

#[test]
fn representable_tokens_reach_calculation_without_binary_float_rounding() {
    for (token, expected) in [
        ("0.4999999999999999999999999999", "0"),
        ("0.5000000000000000000000000000", "1"),
        ("4.999999999999999999999999999e-1", "0"),
        ("0.500000000000000000000000000000000", "1"),
    ] {
        for value in [token.to_owned(), format!("\"{token}\"")] {
            let body = format!(
                r#"{{"name":"x","quantity":1,"unit":"db","unit_price":{value},"vat_rate":"AAM"}}"#
            );
            let input: LineItemInput = serde_json::from_str(&body).expect("exact input");
            let item = input
                .to_line_item(&Currency::from("HUF"))
                .expect("calculation");
            assert_eq!(item.net_value.to_string(), expected, "{body}");
            let encoded = serde_json::to_value(&input).expect("encode");
            assert_eq!(
                serde_json::from_value::<LineItemInput>(encoded).expect("round trip"),
                input
            );
        }
    }
    for body in [r#"{"bank":"MNB"}"#, r#"{"bank":"MNB","rate":null}"#] {
        assert_eq!(
            serde_json::from_str::<ExchangeRateInput>(body)
                .expect("optional")
                .rate,
            None
        );
    }
}

#[test]
fn numeric_values_remain_exact_through_serde_json_value() {
    for token in [
        "0.5",
        "0.1",
        "18446744073709551616",
        "-18446744073709551616",
        "79228162514264337593543950335",
        "0.4999999999999999999999999999",
    ] {
        let body = format!(r#"{{"bank":"MNB","rate":{token}}}"#);
        let direct: ExchangeRateInput = serde_json::from_str(&body).expect("direct");
        let value: serde_json::Value = serde_json::from_str(&body).expect("value");
        assert_eq!(
            serde_json::from_value::<ExchangeRateInput>(value).expect("from value"),
            direct
        );
    }
    for token in [
        "0.49999999999999999999999999999",
        "1e-29",
        "79228162514264337593543950336",
    ] {
        let body = format!(r#"{{"bank":"MNB","rate":{token}}}"#);
        let value: serde_json::Value = serde_json::from_str(&body).expect("value");
        assert!(
            serde_json::from_value::<ExchangeRateInput>(value).is_err(),
            "{token}"
        );
    }
}
