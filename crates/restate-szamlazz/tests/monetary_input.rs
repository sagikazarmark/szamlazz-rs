//! Approved amounts survive the public worker conversion and monetary XML.
use restate_szamlazz::{
    account::Account,
    contract::{DocumentInput, IssuedKind},
    gateway::DocumentRefs,
    identity::{ExternalId, OrderKey},
};
use rust_decimal::dec;
use serde_json::json;
use szamlazz_agent::{Credentials, wire::AgentRequest};

fn document() -> DocumentInput {
    serde_json::from_value(json!({
        "buyer": {"name":"Buyer", "zip":"1", "city":"City", "address":"Street"},
        "items": [{"name":"Tickets", "quantity":"3", "unit":"db",
            "unit_price":"7.873333", "vat_rate":"27",
            "amounts":{"net":"23.62", "vat":"6.38", "gross":"30.00"}}],
        "fulfillment_date":"2026-09-12", "due_date":"2026-09-12",
        "payment_method":"card", "overrides":{"currency":"EUR"},
        "expected_totals":{"net":"23.62", "vat":"6.38", "gross":"30.00"}
    }))
    .expect("public request")
}

#[test]
fn approved_gross_reaches_xml_and_preflight_totals() {
    let document = document();
    let account = Account::new("test", "test");
    let request = account
        .build_create(
            IssuedKind::Invoice,
            &document,
            &OrderKey::parse("tickets").expect("key"),
            &ExternalId::new("test:tickets:invoice"),
            DocumentRefs::default(),
        )
        .expect("gross-first amounts");
    let prepared = document
        .monetary_preflight(&request.header.currency)
        .expect("preflight");
    assert_eq!(prepared.items, request.items);
    assert_eq!(
        (
            prepared.totals.net,
            prepared.totals.vat,
            prepared.totals.gross
        ),
        (dec!(23.62), dec!(6.38), dec!(30))
    );
    let wire = request
        .to_wire(&Credentials::agent_key("test"))
        .expect("XML");
    let xml = String::from_utf8(wire.body).expect("UTF-8");
    for element in [
        "<nettoEgysegar>7.873333</nettoEgysegar>",
        "<nettoErtek>23.62</nettoErtek>",
        "<afaErtek>6.38</afaErtek>",
        "<bruttoErtek>30.00</bruttoErtek>",
    ] {
        assert!(xml.contains(element), "{element}: {xml}");
    }
}

#[test]
fn explicit_amounts_preserve_the_documented_huf_example_and_signed_deductions() {
    // Vendor gross-first example: 3 × 500 gross, 393.66 net unit, 1181 + 319.
    // EUR examples include fractional quantity, half-away ties and zero VAT.
    for (currency, quantity, price, rate, net, vat, gross) in [
        ("HUF", "3", "393.66", "27", "1181", "319", "1500"),
        ("Ft", "3", "393.66", "27", "1181", "319", "1500"),
        ("EUR", "0.5", "10", "27", "5", "1.35", "6.35"),
        ("EUR", "1", "0.50", "5", "0.50", "0.03", "0.53"),
        ("HUF", "1", "2.5", "AAM", "3", "0", "3"),
        ("EUR", "3", "10", "0", "30", "0", "30"),
        ("EUR", "3", "10", "AAM", "30", "0", "30"),
        ("EUR", "1", "100", "5.5", "100", "5.5", "105.5"),
    ] {
        for sign in ["", "-"] {
            let mut doc = document();
            doc.expected_totals = None;
            doc.items[0] = serde_json::from_value(json!({
                "name":"Line", "unit":"db", "quantity":format!("{sign}{quantity}"),
                "unit_price":price, "vat_rate":rate,
                "amounts":{"net":format!("{sign}{net}"), "vat":format!("{sign}{vat}"), "gross":format!("{sign}{gross}")}
            })).expect("input");
            let result = doc
                .monetary_preflight(&currency.into())
                .expect("valid amounts");
            assert_eq!(
                result.totals,
                doc.items[0].amounts.clone().expect("asserted")
            );
            assert_eq!(
                result.items[0].quantity.to_string(),
                format!("{sign}{quantity}")
            );
        }
    }
}

#[test]
fn mixed_rates_and_negative_lines_have_exact_xml_and_document_totals() {
    let mut doc = document();
    doc.items.push(
        serde_json::from_value(json!({
            "name":"Deduction", "unit":"db", "quantity":"-1", "unit_price":"5",
            "vat_rate":"5", "amounts":{"net":"-5", "vat":"-0.25", "gross":"-5.25"}
        }))
        .expect("deduction"),
    );
    doc.items
        .push(restate_szamlazz::contract::LineItemInput::new(
            "Exempt",
            dec!(1),
            "db",
            dec!(2),
            "AAM",
        ));
    doc.expected_totals = Some(
        serde_json::from_value(json!({"net":"20.62", "vat":"6.13", "gross":"26.75"}))
            .expect("totals"),
    );
    let request = Account::new("test", "test")
        .build_create(
            IssuedKind::Final,
            &doc,
            &OrderKey::parse("tickets").expect("key"),
            &ExternalId::new("test:tickets:final"),
            DocumentRefs {
                prepayment: Some("ES-1"),
                ..DocumentRefs::default()
            },
        )
        .expect("mixed document");
    assert_eq!(
        doc.monetary_preflight(&request.header.currency)
            .expect("preflight")
            .totals,
        doc.expected_totals.expect("approved")
    );
    let xml = String::from_utf8(
        request
            .to_wire(&Credentials::agent_key("test"))
            .expect("XML")
            .body,
    )
    .expect("UTF-8");
    for fragment in [
        "<nettoErtek>-5</nettoErtek>",
        "<afaErtek>-0.25</afaErtek>",
        "<bruttoErtek>-5.25</bruttoErtek>",
        "<afakulcs>AAM</afakulcs>",
    ] {
        assert!(xml.contains(fragment), "{xml}");
    }
}

#[test]
fn inconsistent_or_unrepresentable_money_is_refused_by_preflight_and_construction() {
    use restate_szamlazz::contract::MonetaryError;
    for (pointer, value, expected) in [
        (
            "/items/0/amounts/net",
            "23.61",
            MonetaryError::GrossMismatch,
        ),
        (
            "/items/0/amounts/gross",
            "30.001",
            MonetaryError::AmountPrecision,
        ),
        ("/items/0/unit_price", "7.87", MonetaryError::NetMismatch),
        ("/items/0/quantity", "0", MonetaryError::ZeroQuantity),
        (
            "/items/0/vat_rate",
            "-27",
            MonetaryError::UnsupportedVatRate,
        ),
        (
            "/items/0/vat_rate",
            "101",
            MonetaryError::UnsupportedVatRate,
        ),
        (
            "/items/0/vat_rate",
            "K.AFA",
            MonetaryError::UnsupportedVatRate,
        ),
        (
            "/items/0/vat_rate",
            "future",
            MonetaryError::UnsupportedVatRate,
        ),
        (
            "/items/0/vat_rate",
            "1e-29",
            MonetaryError::UnsupportedVatRate,
        ),
        (
            "/items/0/quantity",
            "79228162514264337593543950335",
            MonetaryError::NetMismatch,
        ),
        (
            "/expected_totals/gross",
            "29.98",
            MonetaryError::TotalsMismatch,
        ),
        (
            "/overrides/currency",
            "KWD",
            MonetaryError::UnsupportedCurrency,
        ),
    ] {
        let mut value_json = serde_json::to_value(document()).expect("JSON");
        *value_json.pointer_mut(pointer).expect("field") = json!(value);
        let doc: DocumentInput = serde_json::from_value(value_json).expect("shape");
        let currency = doc.overrides.currency.as_deref().expect("currency").into();
        let error = doc.monetary_preflight(&currency).expect_err("refused");
        let expected = if pointer.starts_with("/expected_totals") {
            expected
        } else {
            MonetaryError::Item {
                index: 0,
                source: Box::new(expected),
            }
        };
        assert_eq!(error, expected, "{pointer}");
        assert!(
            Account::new("test", "test")
                .build_create(
                    IssuedKind::Invoice,
                    &doc,
                    &OrderKey::parse("tickets").expect("key"),
                    &ExternalId::new("test:tickets:invoice"),
                    DocumentRefs::default(),
                )
                .is_err()
        );
    }
    // The downstream unit-rounded split is deliberately not silently migrated.
    let mut doc = document();
    doc.items[0].unit_price = dec!(7.87);
    doc.items[0].amounts = Some(
        serde_json::from_value(json!({"net":"23.61", "vat":"6.39", "gross":"30"}))
            .expect("amounts"),
    );
    assert_eq!(
        doc.monetary_preflight(&szamlazz_agent::Currency::EUR)
            .expect_err("split requires migration"),
        MonetaryError::Item {
            index: 0,
            source: Box::new(MonetaryError::VatMismatch)
        }
    );
}

#[test]
fn legacy_calculation_stays_net_first_and_can_be_guarded_by_approved_totals() {
    let mut doc = document();
    doc.items[0].amounts = None;
    doc.items[0].unit_price = dec!(7.87);
    assert!(
        doc.monetary_preflight(&szamlazz_agent::Currency::EUR)
            .is_err(),
        "30.00 assertion guards a net-first request too"
    );
    doc.expected_totals = None;
    let result = doc
        .monetary_preflight(&szamlazz_agent::Currency::EUR)
        .expect("legacy");
    assert_eq!(
        (result.totals.net, result.totals.vat, result.totals.gross),
        (dec!(23.61), dec!(6.37), dec!(29.98))
    );
}

#[test]
fn exact_sums_and_midpoint_boundaries_never_round_on_success() {
    let mut doc = document();
    doc.expected_totals = None;
    // One exact product below the EUR net midpoint, beyond f64 precision.
    doc.items[0] = serde_json::from_value(json!({
        "name":"Boundary", "quantity":"1", "unit":"db",
        "unit_price":"0.0049999999999999999999999999", "vat_rate":"0",
        "amounts":{"net":"0", "vat":"0", "gross":"0"}
    }))
    .expect("boundary");
    assert_eq!(
        doc.monetary_preflight(&szamlazz_agent::Currency::EUR)
            .expect("exact")
            .totals
            .gross,
        dec!(0)
    );
    doc.items[0].unit_price = dec!(0.005);
    assert!(
        doc.monetary_preflight(&szamlazz_agent::Currency::EUR)
            .is_err(),
        "exact tie rounds away from zero"
    );
    // Each line fits, but their sum cannot fit Decimal exactly (checked_add alone
    // would discard the cent). The document is refused before constructing XML.
    doc.items = vec![
        restate_szamlazz::contract::LineItemInput::new(
            "Large",
            dec!(1),
            "db",
            rust_decimal::Decimal::MAX,
            "AAM",
        ),
        restate_szamlazz::contract::LineItemInput::new("Small", dec!(1), "db", dec!(0.01), "AAM"),
    ];
    assert!(
        doc.monetary_preflight(&szamlazz_agent::Currency::EUR)
            .is_err()
    );
}

#[test]
fn accepted_explicit_lines_are_sign_symmetric_and_assertions_cannot_drift() {
    // Metamorphic property over real calculator outputs: converting calculated
    // amounts to assertions preserves them, negating quantity and every amount
    // negates the wire, and changing only gross is always refused.
    use restate_szamlazz::contract::{Amounts, LineItemInput};
    use szamlazz_agent::Currency;
    for currency in [Currency::HUF, Currency::EUR] {
        for rate in ["0", "5", "18", "27", "5.5", "AAM"] {
            for quantity in [dec!(0.125), dec!(1), dec!(3), dec!(1000)] {
                for price in [dec!(0), dec!(0.005), dec!(7.87), dec!(12345.6789)] {
                    let mut input = LineItemInput::new("Item", quantity, "db", price, rate);
                    let calculated = input.to_line_item(&currency).expect("calculated");
                    input.amounts = Some(Amounts {
                        net: calculated.net_value,
                        vat: calculated.vat_value,
                        gross: calculated.gross_value,
                    });
                    assert_eq!(input.to_line_item(&currency).expect("explicit"), calculated);
                    input.quantity = -quantity;
                    let amounts = input.amounts.as_mut().expect("amounts");
                    amounts.net = -amounts.net;
                    amounts.vat = -amounts.vat;
                    amounts.gross = -amounts.gross;
                    let negative = input.to_line_item(&currency).expect("deduction");
                    assert_eq!(negative.net_value, -calculated.net_value);
                    assert_eq!(negative.vat_value, -calculated.vat_value);
                    assert_eq!(negative.gross_value, -calculated.gross_value);
                    input.amounts.as_mut().expect("amounts").gross += dec!(1);
                    assert!(input.to_line_item(&currency).is_err());
                }
            }
        }
    }
}

#[cfg(feature = "schemars")]
#[test]
fn schema_exposes_closed_exact_monetary_assertions() {
    let schema = serde_json::to_value(schemars::schema_for!(DocumentInput)).expect("schema");
    let amounts = &schema["$defs"]["Amounts"];
    assert_eq!(amounts["additionalProperties"], false);
    assert_eq!(amounts["required"], json!(["net", "vat", "gross"]));
    assert_eq!(
        amounts["properties"]["gross"]["type"],
        json!(["string", "number"])
    );
    assert!(schema["properties"].get("expected_totals").is_some());
    for value in [
        json!({"net":"23.62","vat":"6.38"}),
        json!({"net":"23.62","vat":"6.38","gross":"30", "round":true}),
        json!({"net":"23.62","vat":"6.38","gross":"1e-29"}),
    ] {
        assert!(serde_json::from_value::<restate_szamlazz::contract::Amounts>(value).is_err());
    }
}
