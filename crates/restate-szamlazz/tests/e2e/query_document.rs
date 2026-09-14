//! Document facts through the public query and retained Restate journal.

use serde_json::json;
use wiremock::ResponseTemplate;

use crate::harness::Harness;
use crate::harness::szamlazz::{Doc, number_query};

pub(crate) async fn query_preserves_buyer_and_vat_evidence(h: &Harness) {
    let xml = Doc::new("SZ-VERIFY-227", "SZ").xml()
        .replace("<nev>Buyer</nev>", "<nev>Observed Buyer</nev><adoszam>12345678-2-42</adoszam><adoszameu>HU12345678</adoszameu>")
        .replace("<totalossz>", "<afakulcsossz><afakulcs>27.0</afakulcs><netto>100.123456789012345678</netto><afa>27.03</afa><brutto>127.153456789012345678</brutto></afakulcsossz><afakulcsossz><afakulcs>0.0</afakulcs><afatipus>AAM</afatipus><netto>10.01</netto><afa>0</afa><brutto>10.01</brutto></afakulcsossz><totalossz>");
    number_query("SZ-VERIFY-227")
        .respond_with(ResponseTemplate::new(200).set_body_string(xml))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent(
            "query",
            &json!({
                "selector": {"invoice_number": "SZ-VERIFY-227"}
            }),
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "SZ-VERIFY-227");
    assert_eq!(reply.body["document_id"], 924_307_338);
    assert_eq!(
        reply.body["buyer"],
        json!({
            "name": "Observed Buyer",
            "tax_number": "12345678-2-42",
            "eu_tax_number": "HU12345678"
        })
    );
    assert!(reply.body.get("verification").is_none());
    assert_eq!(
        reply.body["by_vat_rate"],
        json!([
            {"vat_type": null, "vat_rate_code": "27.0", "net": "100.123456789012345678", "vat": "27.03", "gross": "127.153456789012345678"},
            {"vat_type": "AAM", "vat_rate_code": "0.0", "net": "10.01", "vat": "0", "gross": "10.01"}
        ])
    );
}

pub(crate) async fn omitted_evidence_and_equal_totals_do_not_fabricate_matches(h: &Harness) {
    let mut replies = Vec::new();
    for (number, breakdown) in [
        ("SZ-VERIFY-ABSENT", String::new()),
        ("SZ-VERIFY-ZERO", subtotal("0", "0", "0", "0")),
        ("SZ-VERIFY-27", subtotal("27", "100", "27", "127")),
        (
            "SZ-VERIFY-SPLIT",
            format!(
                "{}{}",
                subtotal("27", "50", "13.5", "63.5"),
                subtotal("5", "60.476190", "3.023810", "63.5")
            ),
        ),
    ] {
        let xml = Doc {
            net: "100",
            vat: "27",
            gross: "127",
            ..Doc::new(number, "SZ")
        }
        .xml()
        .replace("<totalossz>", &format!("{breakdown}<totalossz>"));
        number_query(number)
            .respond_with(ResponseTemplate::new(200).set_body_string(xml))
            .expect(1)
            .mount(&h.mock)
            .await;
        let reply = h
            .call_agent("query", &json!({"selector": {"invoice_number": number}}))
            .await;
        assert_eq!(reply.status, 200, "{}", reply.body);
        assert_eq!(reply.body["buyer"]["tax_number"], json!(null));
        assert_eq!(reply.body["buyer"]["eu_tax_number"], json!(null));
        replies.push(reply.body.clone());
    }
    assert_eq!(replies[0]["by_vat_rate"], json!([]));
    assert_eq!(replies[1]["by_vat_rate"][0]["gross"], "0");
    assert_eq!(replies[2]["gross_total"], replies[3]["gross_total"]);
    assert_ne!(replies[2]["by_vat_rate"], replies[3]["by_vat_rate"]);
}

fn subtotal(rate: &str, net: &str, vat: &str, gross: &str) -> String {
    format!(
        "<afakulcsossz><afakulcs>{rate}</afakulcs><netto>{net}</netto><afa>{vat}</afa><brutto>{gross}</brutto></afakulcsossz>"
    )
}

pub(crate) async fn query_preserves_identity_and_fault_rules(h: &Harness) {
    use crate::harness::szamlazz::{api_error, not_found};
    for (number, response, status, code) in [
        (
            "SZ-VERIFY-MISMATCH",
            Doc::new("SZ-OTHER", "SZ").response(),
            503,
            "unavailable",
        ),
        ("SZ-VERIFY-NOTFOUND", not_found(), 404, "not_found"),
        (
            "SZ-VERIFY-KEY",
            api_error("3", "no"),
            503,
            "credentials_rejected",
        ),
        (
            "SZ-VERIFY-API",
            api_error("57", "no"),
            422,
            "szamlazz_error",
        ),
    ] {
        number_query(number)
            .respond_with(response)
            .expect(if status == 503 && code == "unavailable" {
                3
            } else {
                1
            })
            .mount(&h.mock)
            .await;
        let reply = h
            .call_agent("query", &json!({"selector": {"invoice_number": number}}))
            .await;
        assert_eq!(reply.status, status, "{}", reply.body);
        assert_eq!(reply.fault().code.as_str(), code);
        assert!(!reply.body.to_string().contains("SZ-OTHER"));
    }
    for value in [json!(true), json!(false), json!(null)] {
        let reply = h.call_agent("query", &json!({"selector": {"invoice_number": "SZ-VERIFY-NO-SEND"}, "include_verification": value})).await;
        assert_eq!(reply.status, 400, "{}", reply.body);
        assert!(h.admin().runs(reply.invocation_id()).await.is_empty());
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "one retention scenario comparing explicit queries, replay and Order observations"
)]
pub(crate) async fn query_retains_and_replays_only_document_facts(h: &Harness) {
    use restate_e2e_harness::{Call, run_result};
    use restate_szamlazz::contract::QueryResponse;
    use restate_szamlazz::gateway::QueryOutcome;

    let xml = Doc::of("SZ-VERIFY-PRIVATE", "SZ", "E2E-QUERY-227").xml()
        .replace("<nev>Buyer</nev>", "<nev>BUYER-227-RETAINED</nev><adoszam>227-TAX</adoszam><adoszameu>227-EU</adoszameu><email>PRIVATE-227-EMAIL</email><azonosito>PRIVATE-227-PARTNER</azonosito><cim><irsz>1234</irsz><telepules>PRIVATE-227-CITY</telepules><cim>PRIVATE-227-STREET</cim></cim>")
        .replace("<nev>Seller</nev>", "<nev>PRIVATE-227-SELLER</nev>")
        .replace("</szamla>", "<pdf>UFJJVkFURS0yMjctUERG</pdf><cimkek><cimke>PRIVATE-227-LABEL</cimke></cimkek></szamla>")
        .replace("<totalossz>", &format!("{}<totalossz>", subtotal("27", "1000", "270", "1270")));
    number_query("SZ-VERIFY-PRIVATE")
        .respond_with(ResponseTemplate::new(200).set_body_string(xml.clone()))
        .expect(1)
        .mount(&h.mock)
        .await;
    let key = "227-query";
    let body = json!({"selector": {"invoice_number": "SZ-VERIFY-PRIVATE"}});
    let call = Call::service("Szamlazz.Agent", "query");
    let reply = h.invoke(&call, Some(&body), Some(key)).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert!(reply.body.get("verification").is_none());
    let journal = h.admin().journal(reply.invocation_id()).await;
    let result = run_result(&journal, "query").expect("retained query result");
    for sentinel in ["BUYER-227-RETAINED", "227-TAX", "227-EU", "by_vat_rate"] {
        assert!(result.raw_contains(sentinel), "{key}: {sentinel}");
    }
    for entry in &journal {
        for private in [
            "PRIVATE-227-EMAIL",
            "PRIVATE-227-PARTNER",
            "PRIVATE-227-CITY",
            "PRIVATE-227-STREET",
            "PRIVATE-227-SELLER",
            "PRIVATE-227-LABEL",
            "UFJJVkFURS0yMjctUERG",
        ] {
            assert!(!entry.raw_contains(private), "{key}: retained {private}");
        }
    }
    // Read the actual stored JSON, then exercise its typed replay decoder.
    let start = result
        .raw
        .iter()
        .position(|byte| *byte == b'{')
        .expect("JSON result");
    let stored: serde_json::Value = serde_json::Deserializer::from_slice(&result.raw[start..])
        .into_iter()
        .next()
        .expect("JSON value")
        .expect("stored outcome");
    assert_eq!(
        stored["Found"], reply.body,
        "journal has only the response allowlist"
    );
    assert_eq!(
        stored["Found"]["buyer"],
        json!({
            "name": "BUYER-227-RETAINED",
            "tax_number": "227-TAX",
            "eu_tax_number": "227-EU"
        })
    );
    assert_eq!(
        stored["Found"]["by_vat_rate"],
        json!([{"vat_type": null, "vat_rate_code": "27", "net": "1000", "vat": "270", "gross": "1270"}])
    );
    let QueryOutcome::Found(response): QueryOutcome<QueryResponse> =
        serde_json::from_value(stored).expect("replay decode")
    else {
        panic!("found");
    };
    assert_eq!(
        serde_json::to_value(response).expect("response"),
        reply.body
    );
    let retained = h.invoke(&call, Some(&body), Some(key)).await;
    assert_eq!(retained.invocation_id(), reply.invocation_id());
    assert_eq!(retained.body, reply.body);
    crate::harness::query::replay_completed(h.admin(), reply.invocation_id(), &reply.body).await;

    // The explicit query projection must not widen Order observation journals.
    h.absent("E2E-QUERY-227", &["proforma", "prepayment", "final"])
        .await;
    crate::harness::szamlazz::external_id_query("acct:E2E-QUERY-227:invoice")
        .respond_with(ResponseTemplate::new(200).set_body_string(xml))
        .expect(1)
        .mount(&h.mock)
        .await;
    let observed = h
        .invoke(
            &Call::object("Szamlazz.Order", "E2E-QUERY-227", "get"),
            None,
            Some("227-get"),
        )
        .await;
    assert_eq!(observed.status, 200, "{}", observed.body);
    for sentinel in ["BUYER-227-RETAINED", "227-TAX", "227-EU", "by_vat_rate"] {
        assert!(!observed.body.to_string().contains(sentinel));
        for entry in h.admin().journal(observed.invocation_id()).await {
            assert!(
                !entry.raw_contains(sentinel),
                "Order get retained {sentinel}"
            );
        }
    }
}
