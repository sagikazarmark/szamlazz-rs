//! Opt-in evidence through the public query and retained Restate journal.

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
                "selector": {"invoice_number": "SZ-VERIFY-227"},
                "include_verification": true
            }),
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "SZ-VERIFY-227");
    assert_eq!(reply.body["document_id"], 924_307_338);
    assert_eq!(
        reply.body["verification"],
        json!({
            "buyer_name": "Observed Buyer",
            "buyer_tax_number": "12345678-2-42",
            "buyer_eu_tax_number": "HU12345678",
            "by_vat_rate": [
                {"vat_type": null, "vat_rate_code": "27.0", "net": "100.123456789012345678", "vat": "27.03", "gross": "127.153456789012345678"},
                {"vat_type": "AAM", "vat_rate_code": "0.0", "net": "10.01", "vat": "0", "gross": "10.01"}
            ]
        })
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
            .call_agent(
                "query",
                &json!({"selector": {"invoice_number": number}, "include_verification": true}),
            )
            .await;
        assert_eq!(reply.status, 200, "{}", reply.body);
        assert_eq!(reply.body["verification"]["buyer_tax_number"], json!(null));
        assert_eq!(
            reply.body["verification"]["buyer_eu_tax_number"],
            json!(null)
        );
        replies.push(reply.body.clone());
    }
    assert_eq!(replies[0]["verification"]["by_vat_rate"], json!([]));
    assert_eq!(replies[1]["verification"]["by_vat_rate"][0]["gross"], "0");
    assert_eq!(replies[2]["gross_total"], replies[3]["gross_total"]);
    assert_ne!(
        replies[2]["verification"]["by_vat_rate"],
        replies[3]["verification"]["by_vat_rate"]
    );
}

fn subtotal(rate: &str, net: &str, vat: &str, gross: &str) -> String {
    format!(
        "<afakulcsossz><afakulcs>{rate}</afakulcs><netto>{net}</netto><afa>{vat}</afa><brutto>{gross}</brutto></afakulcsossz>"
    )
}

pub(crate) async fn verification_uses_the_query_identity_and_fault_rules(h: &Harness) {
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
            .call_agent(
                "query",
                &json!({"selector": {"invoice_number": number}, "include_verification": true}),
            )
            .await;
        assert_eq!(reply.status, status, "{}", reply.body);
        assert_eq!(reply.fault().code.as_str(), code);
        assert!(!reply.body.to_string().contains("SZ-OTHER"));
    }
    for value in [json!(null), json!("true"), json!(1)] {
        let reply = h.call_agent("query", &json!({"selector": {"invoice_number": "SZ-VERIFY-NO-SEND"}, "include_verification": value})).await;
        assert_eq!(reply.status, 400, "{}", reply.body);
        assert!(h.admin().runs(reply.invocation_id()).await.is_empty());
    }
}

pub(crate) async fn only_opted_in_evidence_is_retained_and_replayed(h: &Harness) {
    use restate_e2e_harness::{Call, run_result};
    use restate_szamlazz::contract::QueryResponse;
    use restate_szamlazz::gateway::{FoundDocument, QueryOutcome};

    let xml = Doc::new("SZ-VERIFY-PRIVATE", "SZ").xml()
        .replace("<nev>Buyer</nev>", "<nev>BUYER-227-RETAINED</nev><adoszam>227-TAX</adoszam><adoszameu>227-EU</adoszameu><email>PRIVATE-227-EMAIL</email><azonosito>PRIVATE-227-PARTNER</azonosito><cim><irsz>1234</irsz><telepules>PRIVATE-227-CITY</telepules><cim>PRIVATE-227-STREET</cim></cim>")
        .replace("<nev>Seller</nev>", "<nev>PRIVATE-227-SELLER</nev>")
        .replace("</szamla>", "<pdf>UFJJVkFURS0yMjctUERG</pdf><cimkek><cimke>PRIVATE-227-LABEL</cimke></cimkek></szamla>")
        .replace("<totalossz>", &format!("{}<totalossz>", subtotal("27", "1000", "270", "1270")));
    number_query("SZ-VERIFY-PRIVATE")
        .respond_with(ResponseTemplate::new(200).set_body_string(xml))
        .expect(3)
        .mount(&h.mock)
        .await;
    for (key, include) in [
        ("227-default", None),
        ("227-false", Some(false)),
        ("227-true", Some(true)),
    ] {
        let mut body = json!({"selector": {"invoice_number": "SZ-VERIFY-PRIVATE"}});
        if let Some(include) = include {
            body["include_verification"] = json!(include);
        }
        let call = Call::service("Szamlazz.Agent", "query");
        let reply = h.invoke(&call, Some(&body), Some(key)).await;
        assert_eq!(reply.status, 200, "{}", reply.body);
        let opted_in = include == Some(true);
        assert_eq!(reply.body.get("verification").is_some(), opted_in);
        let journal = h.admin().journal(reply.invocation_id()).await;
        let result = run_result(&journal, "query").expect("retained query result");
        for sentinel in ["BUYER-227-RETAINED", "227-TAX", "227-EU", "by_vat_rate"] {
            assert_eq!(result.raw_contains(sentinel), opted_in, "{key}: {sentinel}");
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
        let projected = if opted_in {
            assert_eq!(
                stored["Found"], reply.body,
                "journal has only the response allowlist"
            );
            assert_eq!(
                stored["Found"]["verification"],
                json!({
                    "buyer_name": "BUYER-227-RETAINED",
                    "buyer_tax_number": "227-TAX",
                    "buyer_eu_tax_number": "227-EU",
                    "by_vat_rate": [{"vat_type": null, "vat_rate_code": "27", "net": "1000", "vat": "270", "gross": "1270"}]
                })
            );
            let QueryOutcome::Found(response): QueryOutcome<QueryResponse> =
                serde_json::from_value(stored).expect("replay decode")
            else {
                panic!("found");
            };
            *response
        } else {
            let QueryOutcome::Found(found): QueryOutcome<FoundDocument> =
                serde_json::from_value(stored).expect("replay decode")
            else {
                panic!("found");
            };
            QueryResponse::from(&*found)
        };
        assert_eq!(
            serde_json::to_value(projected).expect("response"),
            reply.body
        );
        let retained = h.invoke(&call, Some(&body), Some(key)).await;
        assert_eq!(retained.invocation_id(), reply.invocation_id());
        assert_eq!(retained.body, reply.body);
        replay_query_prefix(h, reply.invocation_id(), &reply.body).await;
    }
}

async fn replay_query_prefix(h: &Harness, id: &str, expected: &serde_json::Value) {
    let journal = h.admin().journal(id).await;
    let query = journal
        .iter()
        .find(|entry| entry.is_run() && entry.name.as_deref() == Some("query"))
        .expect("query command");
    // Copy through the completed read, excluding Output: the same deployment
    // must decode the Run and produce Output again without querying the provider.
    let response = crate::common::http_client()
        .patch(format!(
            "{}/invocations/{id}/restart-as-new?from={}&deployment=keep",
            h.admin().base(),
            query.index
        ))
        .send()
        .await
        .expect("restart query prefix");
    let status = response.status();
    let restarted: serde_json::Value = response.json().await.expect("restart response");
    assert!(status.is_success(), "{restarted}");
    let new_id = restarted["new_invocation_id"]
        .as_str()
        .expect("new invocation id");
    h.admin().await_status(new_id, &["completed"]).await;
    assert!(
        h.admin()
            .invocation(new_id)
            .await
            .completion_failure
            .is_none()
    );
    let replayed = h.admin().journal(new_id).await;
    let output = replayed
        .iter()
        .find(|entry| entry.entry_type == "Command: Output")
        .expect("new output");
    let start = output
        .raw
        .iter()
        .position(|byte| *byte == b'{')
        .expect("JSON output");
    let actual: serde_json::Value = serde_json::Deserializer::from_slice(&output.raw[start..])
        .into_iter()
        .next()
        .expect("JSON value")
        .expect("output");
    assert_eq!(&actual, expected);
    let original = restate_e2e_harness::run_result(&journal, "query").expect("original read");
    let copied = restate_e2e_harness::run_result(&replayed, "query").expect("copied read");
    assert_eq!(
        copied.raw, original.raw,
        "replay retains exactly the same allowlist"
    );
}
