//! Exercise every outbound date position through the checked public boundary.
use jiff::civil::date;
use rust_decimal::dec;
use szamlazz_agent::ops::credit_entry::{CreditEntry, RegisterCreditEntry};
use szamlazz_agent::ops::invoice::{Buyer, BuyerLedger, CreateInvoice, InvoiceHeader, InvoiceKind};
use szamlazz_agent::ops::storno::StornoInvoice;
use szamlazz_agent::wire::AgentRequest;
use szamlazz_agent::{
    Credentials, Currency, Language, LineItem, LineItemLedger, PaymentMethod, RequestError, VatRate,
};

fn invoice() -> CreateInvoice {
    CreateInvoice::new(
        InvoiceKind::invoice(),
        InvoiceHeader::new(
            date(2026, 2, 28),
            date(2026, 2, 28),
            PaymentMethod::Transfer,
            Currency::HUF,
            Language::Hungarian,
        ),
        Buyer {
            ledger: Some(BuyerLedger::default()),
            ..Buyer::new("Buyer", "1111", "Budapest", "Street 1")
        },
        vec![LineItem {
            ledger: Some(LineItemLedger::default()),
            ..LineItem::new(
                "Item",
                dec!(1),
                "db",
                dec!(100),
                VatRate::percent(27),
                dec!(100),
                dec!(27),
                dec!(127),
            )
        }],
    )
}

fn check_position<R>(request: &R, pointer: &str, field: &'static str, wire_element: &str)
where
    R: AgentRequest + serde::Serialize + serde::de::DeserializeOwned,
{
    let credentials = Credentials::agent_key("dummy-key");
    for year in [-9999, -1, 0, 1, 2024, 9999] {
        let day = if year == 2024 { 29 } else { 28 };
        let value = date(year, 2, day);
        let mut input = serde_json::to_value(request).expect("request JSON");
        *input.pointer_mut(pointer).expect("existing date field") =
            serde_json::to_value(value).expect("date JSON");
        let request: R = serde_json::from_value(input).expect("Jiff permits the year");
        if year <= 0 {
            let expected = RequestError::InvalidDateYear { field, year };
            assert_eq!(request.validate(), Err(expected.clone()), "{pointer}");
            assert_eq!(
                request.to_wire(&credentials).expect_err("nonpositive date"),
                expected,
                "{pointer}"
            );
        } else {
            let wire = request.to_wire(&credentials).expect("positive date");
            let body = String::from_utf8(wire.body).expect("UTF-8");
            // The control date differs from every other date in the fixture;
            // an unchanged sibling cannot satisfy this preservation assertion.
            assert_eq!(
                body.matches(&format!("<{wire_element}>{value}</{wire_element}>"))
                    .count(),
                1,
                "{pointer}"
            );
        }
    }
}

#[test]
fn invoice_header_and_nested_dates_require_positive_years() {
    let request = invoice();
    request
        .to_wire(&Credentials::agent_key("key"))
        .expect("absent optional dates");
    for (pointer, field, element) in [
        ("/header/issue_date", "header.issue_date", "keltDatum"),
        (
            "/header/fulfillment_date",
            "header.fulfillment_date",
            "teljesitesDatum",
        ),
        (
            "/header/due_date",
            "header.due_date",
            "fizetesiHataridoDatum",
        ),
        (
            "/buyer/ledger/accounting_date",
            "buyer.ledger.accounting_date",
            "konyvelesDatum",
        ),
        (
            "/buyer/ledger/settlement_from",
            "buyer.ledger.settlement_from",
            "elszDatumTol",
        ),
        (
            "/buyer/ledger/settlement_to",
            "buyer.ledger.settlement_to",
            "elszDatumIg",
        ),
        (
            "/items/0/ledger/settlement_from",
            "items.ledger.settlement_from",
            "elszDatumTol",
        ),
        (
            "/items/0/ledger/settlement_to",
            "items.ledger.settlement_to",
            "elszDatumIg",
        ),
    ] {
        check_position(&request, pointer, field, element);
    }
    // Validation must reach later rows, not only the first item.
    let mut request = request;
    request.items.push(request.items[0].clone());
    check_position(
        &request,
        "/items/1/ledger/settlement_to",
        "items.ledger.settlement_to",
        "elszDatumIg",
    );
}

#[test]
fn storno_dates_are_optional_but_require_positive_years_when_present() {
    let request = StornoInvoice::new("I-1");
    request
        .to_wire(&Credentials::agent_key("key"))
        .expect("absent dates");
    check_position(&request, "/issue_date", "issue_date", "keltDatum");
    check_position(
        &request,
        "/fulfillment_date",
        "fulfillment_date",
        "teljesitesDatum",
    );
}

#[test]
fn every_credit_entry_date_requires_a_positive_year() {
    let mut request = RegisterCreditEntry::new("I-1");
    for _ in 0..2 {
        request
            .entries
            .push(CreditEntry::new(
                date(2026, 2, 28),
                PaymentMethod::Transfer,
                dec!(1),
            ))
            .expect("two entries");
    }
    for additive in [false, true] {
        request.additive = additive;
        for index in 0..2 {
            check_position(
                &request,
                &format!("/entries/{index}/date"),
                "entries.date",
                "datum",
            );
        }
    }
}
