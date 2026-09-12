//! Numeric lexical shape at the full document boundary.

use rust_decimal::{Decimal, dec};
use szamlazz_adatkapcsolat::{Document, ParseError};

fn bank(amount: &str) -> String {
    format!(r#"<banktranz xmlns="http://www.szamlazz.hu/banktranz"><id>1</id>{amount}</banktranz>"#)
}

#[test]
fn numeric_tokens_are_validated_before_conversion_or_rounding() {
    for token in [
        "1__2",
        "1_2",
        "_12",
        "12_",
        "1._2",
        "1e1_0",
        "1,2",
        "1 2",
        "NaN",
        "INF",
        "-INF",
        "∞",
        "１２",
        ".",
        "+",
        "1..2",
        "1e",
        "e2",
        "1e+",
        "1e--2",
        "1e2e3",
        "1e2.0",
        "0.123456789012345678901234567890junk",
        "0.123456789012345678901234567890_",
        "0.123456789012345678901234567890.1",
    ] {
        let xml = bank(&format!("<osszeg>{token}</osszeg>"));
        assert!(
            matches!(Document::parse(xml.as_bytes()), Err(ParseError::Xml(_))),
            "{token}"
        );
    }
}

#[test]
fn finite_spellings_scale_and_rounding_are_preserved() {
    for (token, expected) in [
        ("0", Decimal::ZERO),
        ("+0012.3400", dec!(12.3400)),
        ("-.5", dec!(-0.5)),
        ("1.", dec!(1)),
        ("000.", dec!(0)),
        ("000e2", dec!(0)),
        (".5E+1", dec!(5)),
        ("1.25e-2", dec!(0.0125)),
        ("  12.34\n", dec!(12.34)),
        ("79228162514264337593543950335", Decimal::MAX),
        (
            "0.12345678901234567890123456789",
            dec!(0.1234567890123456789012345679),
        ),
        ("2.5e-28", dec!(0.0000000000000000000000000003)),
    ] {
        let xml = bank(&format!("<osszeg>{token}</osszeg>"));
        let Document::BankTransaction(tx) = Document::parse(xml.as_bytes()).expect(token) else {
            panic!("bank transaction");
        };
        assert_eq!(tx.amount, Some(expected), "{token}");
        assert_eq!(tx.raw_xml(), Some(xml.as_str()));
        if token == "+0012.3400" {
            assert_eq!(tx.amount.expect("amount").scale(), 4);
        }
    }
    for amount in [
        "",
        "<osszeg/>",
        "<osszeg></osszeg>",
        "<osszeg> \n </osszeg>",
    ] {
        let Document::BankTransaction(tx) =
            Document::parse(bank(amount).as_bytes()).expect("content")
        else {
            panic!("bank transaction");
        };
        assert_eq!(tx.amount, None);
    }
}

#[test]
fn long_numeric_tokens_on_small_stack() {
    const CHILD: &str = "SZAMLAZZ_ADATKAPCSOLAT_NUMERIC_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().expect("test executable"))
            .args([
                "--exact",
                "long_numeric_tokens_on_small_stack",
                "--nocapture",
            ])
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
        .stack_size(256 * 1024)
        .spawn(|| {
            let zeroes = "0".repeat(100_000);
            for (token, expected) in [
                (zeroes.clone(), Decimal::ZERO),
                (format!("-{zeroes}12.3400"), dec!(-12.3400)),
                (format!("{zeroes}1e2"), dec!(100)),
                (format!("0.{zeroes}"), Decimal::ZERO),
            ] {
                let xml = bank(&format!("<osszeg>{token}</osszeg>"));
                let Document::BankTransaction(tx) =
                    Document::parse(xml.as_bytes()).expect("number")
                else {
                    panic!("bank transaction");
                };
                assert_eq!(tx.amount, Some(expected));
            }
            for token in [
                format!("{zeroes}junk"),
                format!("0.{zeroes}_"),
                "1".repeat(100_000),
            ] {
                assert!(
                    Document::parse(bank(&format!("<osszeg>{token}</osszeg>")).as_bytes()).is_err()
                );
            }
        })
        .expect("small-stack thread")
        .join()
        .expect("numeric parsing");
}
