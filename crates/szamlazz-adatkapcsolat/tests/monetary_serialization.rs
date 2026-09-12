//! Archive/public JSON money must not depend on downstream Decimal features.
use serde::{Serialize, de::DeserializeOwned};
use std::fmt::Write as _;
use szamlazz_adatkapcsolat::*;

fn fields<T: Serialize + DeserializeOwned>(identity: &str, fields: &[(&str, &str)]) {
    for token in [
        "9007199254740993.5",
        "79228162514264337593543950335",
        "12.3400",
    ] {
        let body = fields
            .iter()
            .fold(identity.to_owned(), |mut xml, (wire, _)| {
                write!(xml, "<{wire}>{token}</{wire}>").expect("write XML string");
                xml
            });
        let value: T = quick_xml::de::from_str(&format!("<row>{body}</row>")).expect("XML");
        let encoded = serde_json::to_value(value).expect("JSON");
        for (_, public) in fields {
            assert_eq!(
                encoded[*public],
                token,
                "{} {public}",
                std::any::type_name::<T>()
            );
        }
    }
}

#[test]
fn every_public_monetary_field_is_an_exact_string() {
    fields::<InvoiceInfo>(
        "<id>1</id><szamlaszam>SZ-1</szamlaszam>",
        &[("devizaarf", "exchange_rate")],
    );
    fields::<InvoiceItem>(
        "",
        &[
            ("mennyiseg", "quantity"),
            ("nettoegysegar", "unit_price"),
            ("afakulcs", "vat_rate"),
            ("netto", "net_value"),
            ("arresafaalap", "margin_vat_base"),
            ("afa", "vat_value"),
            ("brutto", "gross_value"),
        ],
    );
    let totals = &[
        ("afakulcs", "vat_rate"),
        ("netto", "net"),
        ("afa", "vat"),
        ("brutto", "gross"),
    ];
    fields::<VatTotal>("", totals);
    fields::<FinancialItem>("", totals);
    fields::<RecordedCreditEntry>("", &[("osszeg", "amount"), ("devizaarf", "exchange_rate")]);
    fields::<BankTransaction>("<id>1</id>", &[("osszeg", "amount")]);
    fields::<ReceiptInfo>("<id>1</id>", &[("devizaarf", "exchange_rate")]);
    fields::<ReceiptItem>(
        "",
        &[
            ("nettoEgysegar", "unit_price"),
            ("mennyiseg", "quantity"),
            ("netto", "net_value"),
            ("afakulcs", "vat_rate"),
            ("afa", "vat_value"),
            ("brutto", "gross_value"),
        ],
    );
    fields::<ReceiptPayment>("", &[("osszeg", "amount")]);
    let total: VatTotal = quick_xml::de::from_str("<row/>").expect("empty total XML");
    let encoded = serde_json::to_value(total).expect("total JSON");
    assert!(encoded["net"].is_null());
}
