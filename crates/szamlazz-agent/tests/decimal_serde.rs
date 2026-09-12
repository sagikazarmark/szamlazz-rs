//! Public monetary decoding preserves source values before any domain arithmetic.
use serde::Deserialize;
use szamlazz_agent::{ExchangeRate, ops::credit_entry::CreditEntry, parse_decimal};

fn credit(token: &str) -> String {
    format!(r#"{{"date":"2026-09-11","title":"átutalás","amount":{token}}}"#)
}

#[test]
fn ignored_wrapper_preserves_raw_json_only_over_a_direct_parser() {
    fn decode<'de, T: Deserialize<'de>, D: serde::Deserializer<'de>>(de: D) -> Result<T, D::Error> {
        serde_ignored::deserialize(de, |_| {})
    }
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Buffered {
        Credit(#[serde(deserialize_with = "decode")] CreditEntry),
    }
    for token in ["9007199254740993.5", "1e-28", r#""12.34""#] {
        let body = credit(token);
        let expected: CreditEntry = serde_json::from_str(&body).expect("direct JSON");
        assert_eq!(
            decode::<CreditEntry, _>(&mut serde_json::Deserializer::from_str(&body))
                .expect("wrapped text"),
            expected
        );
        assert_eq!(
            decode::<CreditEntry, _>(&mut serde_json::Deserializer::from_slice(body.as_bytes()))
                .expect("wrapped bytes"),
            expected
        );
        assert_eq!(
            decode::<CreditEntry, _>(&mut serde_json::Deserializer::from_reader(body.as_bytes()))
                .expect("wrapped reader"),
            expected
        );
    }
    for token in [
        "12.34",
        r#"{"$serde_json::private::Number":"12.34"}"#,
        r#"{"$serde_json::private::RawValue":"12.34"}"#,
    ] {
        let body = credit(token);
        assert!(serde_json::from_str::<Buffered>(&body).is_err(), "{body}");
        if token.starts_with('{') {
            assert!(
                decode::<CreditEntry, _>(&mut serde_json::Deserializer::from_str(&body)).is_err(),
                "{body}"
            );
        }
    }
    let Buffered::Credit(entry) =
        serde_json::from_str(&credit(r#""12.34""#)).expect("buffered string");
    assert_eq!(entry.amount, parse_decimal("12.34").expect("amount"));
    let mut ron =
        ron::Deserializer::from_str("GrandTotal(net:12.34,vat:0,gross:12.34)").expect("RON");
    let total: szamlazz_agent::GrandTotal = decode(&mut ron).expect("wrapped RON scalar path");
    assert_eq!(total.net, entry.amount);
}

#[test]
fn ron_direct_round_trips_preserve_struct_names_options_and_enum_content() {
    fn round_trip<
        T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
    >(
        value: &T,
    ) {
        for config in [
            ron::ser::PrettyConfig::default(),
            ron::ser::PrettyConfig::default().struct_names(true),
        ] {
            let text = ron::ser::to_string_pretty(value, config).expect("serialize RON");
            assert_eq!(&ron::from_str::<T>(&text).expect(&text), value);
        }
    }
    use szamlazz_agent::{
        GrandTotal,
        ops::{invoice::CreationOutcome, storno::StornoResponse},
    };
    round_trip(
        &serde_json::from_str::<GrandTotal>(
            r#"{"net":"9007199254740993.5","vat":"0","gross":"9007199254740993.5"}"#,
        )
        .expect("totals fixture"),
    );
    round_trip(&ExchangeRate::new(
        "MNB",
        parse_decimal("12.34").expect("exact amount"),
    ));
    round_trip(&serde_json::from_str::<ExchangeRate>(r#"{"bank":"MNB"}"#).expect("rate fixture"));
    for state in ["numbered", "unnumbered"] {
        for value in ["null", r#""9007199254740993.5""#] {
            round_trip(
                &serde_json::from_str::<StornoResponse>(
                    &storno_bodies(state, "net_total", value)[0],
                )
                .expect("storno fixture"),
            );
        }
    }
    round_trip(&serde_json::from_str::<CreationOutcome>(r#"{"issued":{"invoice_number":"SZ-1","notification_delivery_failed":false,"net_total":"12.34"}}"#).expect("creation fixture"));
}

#[test]
fn built_in_storno_response_is_independent_of_member_order() {
    use szamlazz_agent::ops::storno::StornoResponse;
    for state in ["numbered", "unnumbered"] {
        for field in ["net_total", "gross_total", "outstanding"] {
            for token in [
                "12.34",
                "9007199254740993.5",
                "1e-2",
                "0.1234567890123456789012345678",
                "79228162514264337593543950335",
            ] {
                for value in [token.to_owned(), format!("\"{token}\"")] {
                    for body in storno_bodies(state, field, &value) {
                        let expected = parse_decimal(token).expect("decimal").to_string();
                        let decoded: StornoResponse = serde_json::from_str(&body).expect(&body);
                        let output = serde_json::to_value(&decoded).expect("serialize");
                        assert_eq!(output["response"][field], expected);
                        assert_eq!(
                            serde_json::from_slice::<StornoResponse>(body.as_bytes())
                                .expect("bytes"),
                            decoded
                        );
                        assert_eq!(
                            serde_json::from_reader::<_, StornoResponse>(body.as_bytes())
                                .expect("reader"),
                            decoded
                        );
                        assert_eq!(
                            serde_json::from_value::<StornoResponse>(output).expect("roundtrip"),
                            decoded
                        );
                        // Value sorts response before state, even when the
                        // serialized source originally emitted state first.
                        let value =
                            serde_json::from_str::<serde_json::Value>(&body).expect("value");
                        assert_eq!(
                            serde_json::from_value::<StornoResponse>(value).expect("numeric Value"),
                            decoded
                        );
                    }
                }
            }
            for value in ["null", "12"] {
                let [tag_first, content_first] = storno_bodies(state, field, value);
                assert_eq!(
                    serde_json::from_str::<StornoResponse>(&tag_first).expect("tag first"),
                    serde_json::from_str::<StornoResponse>(&content_first).expect("content first")
                );
            }
            for token in [
                "1e-29",
                r#""0.49999999999999999999999999999""#,
                "79228162514264337593543950336",
                r#"{"$serde_json::private::Number":"12.34"}"#,
                r#"{"\u0024serde_json::private::Number":"12.34"}"#,
                r#"{"$serde_json::private::RawValue":"12.34"}"#,
                "[]",
                "true",
            ] {
                for body in storno_bodies(state, field, token) {
                    assert!(
                        serde_json::from_str::<StornoResponse>(&body).is_err(),
                        "{body}"
                    );
                }
            }
        }
    }
}

fn storno_bodies(state: &str, field: &str, value: &str) -> [String; 2] {
    let content = format!(
        r#"{{"invoice_number":"SZ-1","notification_delivery_failed":false,"{field}":{value}}}"#
    );
    [
        format!(r#"{{"state":"{state}","response":{content}}}"#),
        format!(r#"{{"response":{content},"state":"{state}"}}"#),
    ]
}

#[test]
fn built_in_creation_outcome_keeps_exact_numbers_without_buffering() {
    use szamlazz_agent::ops::invoice::CreationOutcome;
    for token in [
        "12.34",
        "9007199254740993.5",
        "1e-2",
        "0.1234567890123456789012345678",
    ] {
        for value in [token.to_owned(), format!("\"{token}\"")] {
            for content in [
                format!(
                    r#"{{"net_total":{value},"invoice_number":"SZ-1","notification_delivery_failed":false}}"#
                ),
                format!(
                    r#"{{"invoice_number":"SZ-1","notification_delivery_failed":false,"net_total":{value}}}"#
                ),
            ] {
                let body = format!(r#"{{"issued":{content}}}"#);
                let decoded: CreationOutcome = serde_json::from_str(&body).expect(&body);
                assert_eq!(
                    decoded.issued().expect("issued").net_total,
                    Some(parse_decimal(token).expect("decimal"))
                );
                assert_eq!(
                    serde_json::from_value::<CreationOutcome>(
                        serde_json::to_value(&decoded).expect("serialize")
                    )
                    .expect("roundtrip"),
                    decoded
                );
            }
        }
    }
    for token in [
        "1e-29",
        r#""0.49999999999999999999999999999""#,
        r#"{"$serde_json::private::Number":"12.34"}"#,
        r#"{"$serde_json::private::RawValue":"12.34"}"#,
    ] {
        let body = format!(
            r#"{{"issued":{{"invoice_number":"SZ-1","notification_delivery_failed":false,"net_total":{token}}}}}"#
        );
        assert!(
            serde_json::from_str::<CreationOutcome>(&body).is_err(),
            "{body}"
        );
    }
    let preview: CreationOutcome =
        serde_json::from_str(r#"{"preview":{"pdf":"JVBERi0xLjQ="}}"#).expect("preview");
    assert!(preview.issued().is_none());
    assert_eq!(
        serde_json::from_value::<CreationOutcome>(
            serde_json::to_value(&preview).expect("serialize")
        )
        .expect("roundtrip"),
        preview
    );
}

#[test]
fn storno_envelope_shape_and_caller_wrapper_behavior_are_preserved() {
    use szamlazz_agent::ops::storno::StornoResponse;
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Wrapper {
        Storno(StornoResponse),
    }
    for body in [
        r#"{"state":"unnumbered","state":"numbered","response":{}}"#,
        r#"{"response":{},"state":"unnumbered","response":{}}"#,
        r#"{"response":{},"state":"future"}"#,
        r#"{"state":"unnumbered"}"#,
        r#"{"response":{}}"#,
        r#"{"response":null,"state":"unnumbered"}"#,
        r#"{"response":{},"state":"numbered"}"#,
        r#"{"response":{"net_total":1,"net_total":2},"state":"unnumbered"}"#,
    ] {
        assert!(
            serde_json::from_str::<StornoResponse>(body).is_err(),
            "{body}"
        );
    }
    for body in storno_bodies("unnumbered", "net_total", r#""12.34""#) {
        let Wrapper::Storno(decoded) =
            serde_json::from_str(&body).expect("caller buffered strings");
        assert_eq!(
            serde_json::to_value(decoded).expect("serialize")["response"]["net_total"],
            "12.34"
        );
    }
    for token in [
        "12.34",
        r#"{"$serde_json::private::Number":"12.34"}"#,
        r#"{"$serde_json::private::RawValue":"12.34"}"#,
    ] {
        for body in storno_bodies("unnumbered", "net_total", token) {
            assert!(serde_json::from_str::<Wrapper>(&body).is_err(), "{body}");
        }
    }
    let decoded: StornoResponse =
        serde_json::from_str(r#"{"extra":true,"response":{},"state":"unnumbered"}"#)
            .expect("unknown fields remain ignored");
    assert!(matches!(decoded, StornoResponse::Unnumbered(_)));
}

#[test]
fn direct_json_preserves_exact_strings_numbers_and_exponents() {
    for token in [
        "12.34",
        "1e-2",
        "1E+2",
        "0.1234567890123456789012345678",
        "9007199254740993.5",
        "79228162514264337593543950335",
        "1.0000000000000000000000000000000",
        "0e-999999999999999999999",
        "-1e-28",
    ] {
        for value in [token.to_owned(), format!("\"{token}\"")] {
            let body = credit(&value);
            let expected = parse_decimal(token).expect("representable");
            let entry: CreditEntry = serde_json::from_str(&body).expect("string input");
            assert_eq!(entry.amount, expected, "{body}");
            let entry: CreditEntry = serde_json::from_slice(body.as_bytes()).expect("byte input");
            assert_eq!(entry.amount, expected);
            let entry: CreditEntry =
                serde_json::from_reader(body.as_bytes()).expect("reader input");
            assert_eq!(entry.amount, expected);
            let entry: CreditEntry = serde_json::from_value(
                serde_json::from_str::<serde_json::Value>(&body).expect("preserved number"),
            )
            .expect("prebuilt value");
            assert_eq!(entry.amount, expected);
            let buffered: serde_json::Value = serde_json::from_str(&body).expect("value");
            assert_eq!(
                CreditEntry::deserialize(&buffered)
                    .expect("borrowed Value")
                    .amount,
                expected
            );
            let rate: ExchangeRate =
                serde_json::from_str(&format!(r#"{{"bank":"MNB","rate":{value}}}"#))
                    .expect("optional input");
            assert_eq!(rate.rate, Some(expected));
            let roundtrip: CreditEntry =
                serde_json::from_value(serde_json::to_value(&entry).expect("serialize"))
                    .expect("roundtrip");
            assert_eq!(roundtrip, entry);
        }
    }
    for body in [r#"{"bank":"MNB"}"#, r#"{"bank":"MNB","rate":null}"#] {
        assert_eq!(
            serde_json::from_str::<ExchangeRate>(body)
                .expect("omitted rate")
                .rate,
            None
        );
    }
}

#[test]
fn unrepresentable_values_and_objects_never_become_money() {
    let mut tokens = Vec::new();
    for token in [
        "0.49999999999999999999999999999",
        "1e-29",
        "79228162514264337593543950336",
        "1e999999999999999999999",
    ] {
        tokens.extend([token.to_owned(), format!("\"{token}\"")]);
    }
    for token in [
        r#"{"$serde_json::private::Number":"1"}"#,
        r#"{"\u0024serde_json::private::Number":"1"}"#,
        r#"{"$serde_json::private::RawValue":"1"}"#,
        r#"{"$serde_json::private::Number":"1","extra":2}"#,
        r#"{"$serde_json::private::Number":"1","$serde_json::private::Number":"2"}"#,
        "{}",
        "[]",
        "true",
        r#""NaN""#,
        r#"" 1""#,
        r#""1_000""#,
    ] {
        tokens.push(token.to_owned());
    }
    for token in tokens {
        assert!(
            serde_json::from_str::<CreditEntry>(&credit(&token)).is_err(),
            "{token}"
        );
        assert!(
            serde_json::from_str::<ExchangeRate>(&format!(r#"{{"bank":"MNB","rate":{token}}}"#))
                .is_err(),
            "{token}"
        );
    }
    assert!(serde_json::from_str::<CreditEntry>(&credit("null")).is_err());
    // A manually built Value::Object still has its object identity. Only
    // parsing source into Value first can already have erased the distinction.
    for key in [
        "$serde_json::private::Number",
        "$serde_json::private::RawValue",
    ] {
        let mut body: serde_json::Value = serde_json::from_str(&credit("1")).expect("credit");
        body["amount"] = serde_json::Value::Object(
            [(key.to_owned(), serde_json::Value::String("1".to_owned()))]
                .into_iter()
                .collect(),
        );
        assert!(serde_json::from_value::<CreditEntry>(body).is_err());
    }
}

#[test]
fn buffered_wrappers_support_scalars_but_refuse_ambiguous_maps() {
    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    enum Untagged {
        Credit(CreditEntry),
    }
    #[derive(Debug, Deserialize)]
    #[serde(tag = "type")]
    enum Tagged {
        Credit(CreditEntry),
    }
    #[derive(Debug, Deserialize)]
    struct Flattened {
        #[serde(flatten)]
        credit: CreditEntry,
    }
    for token in ["12", r#""0.1234567890123456789012345678""#] {
        let body = credit(token);
        let expected: CreditEntry = serde_json::from_str(&body).expect("direct");
        let Untagged::Credit(actual) = serde_json::from_str(&body).expect("untagged");
        assert_eq!(actual, expected);
        let tagged = body.replacen('{', r#"{"type":"Credit","#, 1);
        let Tagged::Credit(actual) = serde_json::from_str(&tagged).expect("tagged");
        assert_eq!(actual, expected);
        let actual: Flattened = serde_json::from_str(&body).expect("flattened");
        assert_eq!(actual.credit, expected);
    }
    for token in [
        "12.34",
        r#"{"$serde_json::private::Number":"12.34"}"#,
        r#"{"$serde_json::private::RawValue":"12.34"}"#,
    ] {
        let body = credit(token);
        assert!(serde_json::from_str::<Untagged>(&body).is_err());
        assert!(
            serde_json::from_str::<Tagged>(&body.replacen('{', r#"{"type":"Credit","#, 1)).is_err()
        );
        assert!(serde_json::from_str::<Flattened>(&body).is_err());
    }
}

#[test]
fn other_self_describing_formats_keep_scalar_behavior() {
    use serde::de::value::{Error, MapDeserializer};
    use szamlazz_agent::types::GrandTotal;
    let totals = GrandTotal::deserialize(MapDeserializer::<_, Error>::new(
        [("net", "1.234e-2"), ("vat", "0"), ("gross", "1.234e-2")].into_iter(),
    ))
    .expect("string scalars");
    assert_eq!(totals.net, parse_decimal("0.01234").expect("decimal"));
    let totals = GrandTotal::deserialize(MapDeserializer::<_, Error>::new(
        [("net", 12.34_f64), ("vat", 0.0), ("gross", 12.34)].into_iter(),
    ))
    .expect("float scalars");
    assert_eq!(totals.net, parse_decimal("12.34").expect("decimal"));
    assert!(
        GrandTotal::deserialize(MapDeserializer::<_, Error>::new(
            [("net", "1e-29"), ("vat", "0"), ("gross", "0")].into_iter(),
        ))
        .is_err()
    );
}

#[test]
fn vat_percentage_serialization_is_feature_independent() {
    let value = szamlazz_agent::VatRate::Percent(
        parse_decimal("9007199254740993.5").expect("exact percentage"),
    );
    assert_eq!(
        serde_json::to_string(&value).expect("serialize percentage"),
        r#""9007199254740993.5""#
    );
    assert_eq!(
        serde_json::from_str::<szamlazz_agent::VatRate>(
            &serde_json::to_string(&value).expect("serialize percentage")
        )
        .expect("deserialize percentage"),
        value
    );
}

fn fields<T: serde::de::DeserializeOwned + serde::Serialize>(
    baseline: &str,
    required: &[&str],
    optional: &[&str],
) {
    let baseline: serde_json::Value = serde_json::from_str(baseline).expect("fixture");
    for field in required.iter().chain(optional) {
        for token in ["12.34", "9007199254740993.5", "1e-2"] {
            let mut input = baseline.clone();
            input[field] = serde_json::from_str(token).expect("number");
            let decoded: T = serde_json::from_str(&input.to_string())
                .unwrap_or_else(|error| panic!("{} {field}: {error}", std::any::type_name::<T>()));
            let output = serde_json::to_value(decoded).expect("serialize");
            assert_eq!(
                output[field],
                parse_decimal(token).expect("decimal").to_string()
            );
        }
        for token in [
            "1e-29",
            r#""1e-29""#,
            r#"{"$serde_json::private::Number":"1"}"#,
            r#"{"$serde_json::private::RawValue":"1"}"#,
        ] {
            // Splice raw text: Value would itself interpret a Number lookalike.
            let mut input = baseline.clone();
            input[field] = serde_json::json!("REPLACE");
            let input = input.to_string().replace("\"REPLACE\"", token);
            assert!(
                serde_json::from_str::<T>(&input).is_err(),
                "{} {field}: {input}",
                std::any::type_name::<T>()
            );
        }
        let mut input = baseline.clone();
        input[field] = serde_json::Value::Null;
        assert_eq!(
            serde_json::from_str::<T>(&input.to_string()).is_ok(),
            optional.contains(field),
            "{field} null"
        );
        input.as_object_mut().expect("object").remove(*field);
        assert_eq!(
            serde_json::from_str::<T>(&input.to_string()).is_ok(),
            optional.contains(field),
            "{field} omitted"
        );
    }
}

#[test]
fn all_public_monetary_fields_apply_the_same_contract() {
    use szamlazz_agent::{
        LineItem,
        ops::{
            credit_entry::InvoiceBalance,
            invoice::{CreatedInvoice, InvoiceHeader, Mpl},
            query_pdf::InvoicePdf,
            query_xml::{DocumentItem, FinancialItem, InvoiceInfo, RecordedCreditEntry},
            receipt::{Receipt, ReceiptItem, ReceiptPayment},
            storno::InvoiceAcknowledgement,
        },
        types::{GrandTotal, VatTotal},
    };
    let row = r#"{"name":"x","unit":"db","vat_rate":"AAM","vat_rate_code":"0","quantity":"1","unit_price":"1","net_value":"1","vat_value":"0","gross_value":"1"}"#;
    let amounts = &[
        "quantity",
        "unit_price",
        "net_value",
        "vat_value",
        "gross_value",
    ];
    fields::<LineItem>(row, amounts, &["margin_vat_base"]);
    fields::<DocumentItem>(row, amounts, &["margin_vat_base"]);
    fields::<ReceiptItem>(row, amounts, &[]);
    fields::<GrandTotal>(
        r#"{"net":"1","vat":"0","gross":"1"}"#,
        &["net", "vat", "gross"],
        &[],
    );
    fields::<VatTotal>(
        r#"{"net":"1","vat":"0","gross":"1","vat_rate_code":"0"}"#,
        &["net", "vat", "gross"],
        &[],
    );
    fields::<FinancialItem>(
        r#"{"name":"x","net":"1","vat":"0","gross":"1","vat_rate_code":"0","deductible_vat":0,"labels":[]}"#,
        &["net", "vat", "gross"],
        &[],
    );
    fields::<CreditEntry>(&credit("1"), &["amount"], &[]);
    fields::<RecordedCreditEntry>(&credit("1"), &["amount"], &["exchange_rate"]);
    fields::<ReceiptPayment>(r#"{"method":"cash","amount":"1"}"#, &["amount"], &[]);
    fields::<ExchangeRate>(r#"{"bank":"MNB"}"#, &[], &["rate"]);
    fields::<Mpl>(
        r#"{"customer_code":"x","barcode":"x","weight":"1"}"#,
        &[],
        &["declared_value"],
    );
    fields::<InvoiceHeader>(
        r#"{"fulfillment_date":"2026-09-11","due_date":"2026-09-11","payment_method":"cash","currency":"HUF","language":"hu"}"#,
        &[],
        &["payable_adjustment"],
    );
    fields::<InvoiceInfo>(
        r#"{"id":1,"invoice_number":"SZ-1","document_type":"SZ","appearance":1}"#,
        &[],
        &["exchange_rate"],
    );
    fields::<Receipt>(
        r#"{"id":1,"receipt_number":"NY-1","document_type":"NY","reversed":false,"issue_date":"2026-09-11","payment_method":"cash","currency":"HUF","items":[],"totals":{"by_vat_rate":[],"total":{"net":"1","vat":"0","gross":"1"}}}"#,
        &[],
        &["exchange_rate"],
    );
    let totals = &["net_total", "gross_total", "outstanding"];
    fields::<InvoiceBalance>("{}", &[], totals);
    fields::<InvoiceAcknowledgement>("{}", &[], totals);
    fields::<CreatedInvoice>(
        r#"{"invoice_number":"SZ-1","notification_delivery_failed":false}"#,
        &[],
        totals,
    );
    fields::<InvoicePdf>(r#"{"pdf":"JVBERi0xLjQ="}"#, &[], totals);
}
