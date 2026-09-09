//! A credential code (3, 135, 136, 164) is `CredentialsRejected` on every
//! operation, one gateway per operation, read off the wire where szamlazz.hu
//! puts it.

use super::common::{
    Doc, api_error, body_error, create, credit, delete, external_id_query, not_found, order_query,
    storno, taxpayer_query,
};
use super::harness::*;
use jiff::civil::date;
use restate_szamlazz::contract::{PaymentEntry, PaymentMethod, Selector};
use restate_szamlazz::gateway::{
    CreateOutcome, DeleteOutcome, LookupOutcome, ProbeOutcome, QueryOutcome, SetPaymentsOutcome,
    StornoLookupOutcome, StornoOutcome, SzamlazzAnswer, TaxpayerOutcome,
};
use rust_decimal::dec;

/// A credential code (3, 135, 136, 164) is `CredentialsRejected` on **every**
/// operation, read off the wire where szamlazz.hu puts it: in the body alone
/// (the XML query's shape) or in the headers and the body (the create's, the
/// storno's, the delete's, the taxpayer query's). Which codes are credential
/// codes is `ErrorCode::is_credential_error`'s table, unit-tested in the
/// agent crate; that each step consults it before doing anything else is
/// pinned here with one code per operation and the operation's own shape:
/// `expect(1)` on the answering mock, `expect(0)` on the send that must not
/// follow (the hint after a rejected external-id query, the create after a
/// rejected leading query), and no re-query after a rejected send (settled
/// data, never `Unconfirmed`: the run retry policy is not spent on a wrong
/// key).
#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "one table: every operation, one gateway each"
)]
async fn a_credential_code_on_any_operation_is_credentials_rejected() {
    let entry = PaymentEntry {
        date: date(2026, 9, 3),
        method: PaymentMethod::Card,
        amount: dec!(1000),
        description: None,
    };
    let rejected = |code: &str| SzamlazzAnswer::new(code, "login");

    // The lookup's external-id query, in the body: no hint follows.
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(body_error("3", "login"))
        .expect(1)
        .mount(&h.server)
        .await;
    order_query("ORD-1")
        .respond_with(Doc::new("SZ-77", "SZ").response())
        .expect(0)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.lookup(&[]).await,
        LookupOutcome::CredentialsRejected(rejected("3")),
        "lookup, external id"
    );

    // The lookup's hint: conclusive, unlike a miss or another code on it.
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;
    order_query("ORD-1")
        .respond_with(body_error("135", "login"))
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.lookup(&[]).await,
        LookupOutcome::CredentialsRejected(rejected("135")),
        "lookup, hint"
    );

    // The create's send, in the headers: settled without a re-query.
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:invoice")
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;
    create()
        .respond_with(api_error("136", "login"))
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.create(None).await,
        Ok(CreateOutcome::CredentialsRejected(rejected("136"))),
        "create, send"
    );
    assert_eq!(h.bodies().await.len(), 2, "create: no re-query");

    // The three reads of a query, in the body.
    let h = Harness::start().await;
    super::common::op("action-szamla_agent_xml")
        .respond_with(body_error("164", "login"))
        .mount(&h.server)
        .await;
    let expected = Ok(QueryOutcome::CredentialsRejected(rejected("164")));
    assert_eq!(h.gateway.verify("SZ-1").await, expected, "verify");
    assert_eq!(h.gateway.hint(&order()).await, expected, "hint");
    assert_eq!(
        h.gateway
            .query(&Selector::ExternalId("acct:ORD-1:invoice".to_owned()))
            .await,
        expected,
        "query"
    );

    // The probe: data, one request.
    let h = Harness::start().await;
    external_id_query(probe_id().as_str())
        .respond_with(body_error("3", "login"))
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.probe(&probe_id()).await,
        Ok(ProbeOutcome::CredentialsRejected(rejected("3"))),
        "probe"
    );

    // The storno lookup.
    let h = Harness::start().await;
    external_id_query(storno_id().as_str())
        .respond_with(body_error("135", "login"))
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.lookup_storno(&storno_id(), "SZ-1").await,
        Ok(StornoLookupOutcome::CredentialsRejected(rejected("135"))),
        "storno lookup"
    );

    // The storno's send, in the headers: settled without a re-query. (The
    // storno's leading query is a row of its leading-query table.)
    let h = Harness::start().await;
    external_id_query(storno_id().as_str())
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;
    storno()
        .respond_with(api_error("136", "login"))
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.storno(storno_request(&storno_id())).await,
        Ok(StornoOutcome::CredentialsRejected(rejected("136"))),
        "storno, send"
    );
    assert_eq!(h.bodies().await.len(), 2, "storno: no re-query");

    // The delete and the credit entries.
    let h = Harness::start().await;
    delete()
        .respond_with(api_error("164", "login"))
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.delete_proforma("D-1").await,
        DeleteOutcome::CredentialsRejected(rejected("164")),
        "delete"
    );
    let h = Harness::start().await;
    credit()
        .respond_with(body_error("3", "login"))
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway
            .set_payments("SZ-1", std::slice::from_ref(&entry), false)
            .await,
        SetPaymentsOutcome::CredentialsRejected(rejected("3")),
        "set_payments"
    );

    // The taxpayer query, in the headers.
    let h = Harness::start().await;
    taxpayer_query("12345678")
        .respond_with(api_error("135", "login"))
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.query_taxpayer(&prefix()).await,
        Ok(TaxpayerOutcome::CredentialsRejected(rejected("135"))),
        "taxpayer"
    );
}
