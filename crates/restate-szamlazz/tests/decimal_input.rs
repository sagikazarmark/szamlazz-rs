//! Financial input must be exact before currency rounding or any durable work.
use restate_szamlazz::contract::{CreditEntryInput, ExchangeRateInput, LineItemInput};
use restate_szamlazz::szamlazz_agent::Currency;

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
