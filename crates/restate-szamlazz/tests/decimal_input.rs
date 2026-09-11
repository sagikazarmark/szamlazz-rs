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
