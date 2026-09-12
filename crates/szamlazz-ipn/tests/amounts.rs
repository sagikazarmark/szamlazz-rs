//! Public monetary boundaries, including stack-overflow regressions isolated
//! in a subprocess: a regression must fail this test, not abort the runner.

use rust_decimal::{Decimal, dec};
use szamlazz_ipn::PaymentNotification;

#[test]
fn form_amount_grammar_and_exactness() {
    for (text, expected) in [
        (" +0012.3400 ", Some(dec!(12.3400))),
        ("-.5", Some(dec!(-0.5))),
        ("1.", Some(dec!(1))),
        ("1__2", Some(dec!(12))), // Existing from_str_exact tolerance.
        ("1_2,50", Some(dec!(12.50))),
        ("1e2", None),
        ("1E-2", None),
        ("1,2.3", None),
        ("1,2,3", None),
        (".", None),
        ("+", None),
        ("_1", None),
        ("1 2", None),
        ("NaN", None),
        ("∞", None),
        ("79228162514264337593543950335", Some(Decimal::MAX)),
        ("79228162514264337593543950336", None),
        ("0.12345678901234567890123456789", None),
        ("0.00000000000000000000000000000", None),
    ] {
        let body = form_urlencoded::Serializer::new(String::new())
            .append_pair("szlahu_szamlaszam", "E-1")
            .append_pair("szlahu_bruttovegosszeg", text)
            .append_pair("szlahu_kifizetettbrutto", text)
            .finish();
        let value = PaymentNotification::from_form_bytes(body.as_bytes()).expect("content");
        assert_eq!(value.gross_total, expected, "{text}");
        assert_eq!(value.paid_gross, expected, "{text}");
        assert_eq!(value.raw_gross_total.as_deref(), Some(text));
        assert_eq!(value.raw_paid_gross.as_deref(), Some(text));
        assert_eq!(value.is_fully_paid(), expected.map(|_| true));
    }
}

#[test]
fn long_amounts_on_small_stack() {
    const CHILD: &str = "SZAMLAZZ_IPN_LONG_AMOUNTS_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().expect("test executable"))
            .args(["--exact", "long_amounts_on_small_stack", "--nocapture"])
            .env(CHILD, "1")
            .output()
            .expect("run isolated regression");
        assert!(
            output.status.success(),
            "child failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    std::thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(|| {
            let zeroes = "0".repeat(100_000);
            for (text, expected) in [
                (zeroes.clone(), Some(Decimal::ZERO)),
                (format!("-{zeroes}12,34"), Some(dec!(-12.34))),
                (format!("{zeroes}79228162514264337593543950335"), Some(Decimal::MAX)),
                (format!("{zeroes}79228162514264337593543950336"), None),
                (format!("{zeroes}e0"), None),
                (format!("{zeroes}junk"), None),
                (format!("0.{zeroes}"), None),
                (format!("0.{zeroes}1"), None),
                (format!("0{}", "_".repeat(100_000)), Some(Decimal::ZERO)),
            ] {
                let body = format!("szlahu_szamlaszam=E&szlahu_bruttovegosszeg={text}&szlahu_kifizetettbrutto={text}");
                let value = PaymentNotification::from_form_bytes(body.as_bytes()).expect("content");
                assert_eq!(value.gross_total, expected);
                assert_eq!(value.paid_gross, expected);
                assert_eq!(value.raw_gross_total.as_deref(), Some(text.as_str()));
                assert_eq!(value.raw_paid_gross.as_deref(), Some(text.as_str()));
                assert_eq!(value.is_fully_paid(), expected.map(|_| true));
            }
            #[cfg(feature = "serde")]
            long_json_amounts(&zeroes);
        })
        .expect("small-stack thread")
        .join()
        .expect("amount parsing");
}

#[cfg(feature = "serde")]
fn long_json_amounts(zeroes: &str) {
    for (text, expected) in [
        (format!("0.{zeroes}"), Some(Decimal::ZERO)),
        (format!("12.34{zeroes}"), Some(dec!(12.34))),
        (format!("0.{zeroes}e-2"), Some(Decimal::ZERO)),
        (
            format!("1.00{zeroes}e-28"),
            Some(dec!(0.0000000000000000000000000001)),
        ),
        (format!("0.{zeroes}1"), None),
        (format!("0.{zeroes}1e28"), None),
        (format!("1{zeroes}"), None),
    ] {
        for token in [text.clone(), serde_json::to_string(&text).expect("string")] {
            let json =
                format!(r#"{{"document_number":"E","gross_total":{token},"paid_gross":{token}}}"#);
            let value = serde_json::from_str::<PaymentNotification>(&json);
            if let Some(expected) = expected {
                let value = value.expect("exact JSON");
                assert_eq!(value.gross_total, Some(expected));
                assert_eq!(value.paid_gross, Some(expected));
            } else {
                assert!(value.is_err());
            }
        }
    }
    // JSON strings retain JSON-number grammar, including its leading-zero rule.
    let json = format!(r#"{{"document_number":"E","gross_total":"{zeroes}"}}"#);
    assert!(serde_json::from_str::<PaymentNotification>(&json).is_err());
}
