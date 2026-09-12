//! Actual Restate → actual szamlazz.hu journeys; opt in with `cargo live`.
mod live_support;

use futures_util::FutureExt;
use live_support::{Run, assert_document, key, previous_month, today};
use restate_e2e_harness::gate::{
    PROTOCOL_V7, ReusePolicy, SCOPED_VIRTUAL_OBJECTS, VQUEUES, launcher_or_skip,
};
use restate_e2e_harness::{Call, Restate, ServerSpec};
use restate_sdk::prelude::Endpoint;
use restate_szamlazz::account::{Accounts, StaticConfig, StaticResolver};
use restate_szamlazz::config::WorkerConfig;
use restate_szamlazz::contract::{
    BuyerInput, DocumentInput, LineItemInput, PaymentMethod, QueryResponse,
};
use restate_szamlazz::{Agent, Order};
use rust_decimal::{Decimal, dec};
use serde_json::{Value, json};
use std::{panic::AssertUnwindSafe, sync::Arc};
use szamlazz_agent::{Currency, DocumentType, InvoiceNumber, InvoiceSelector, VatRate};

const SERVER: ServerSpec = ServerSpec {
    name: "vendor-live",
    features: &[
        (VQUEUES, true),
        (PROTOCOL_V7, true),
        (SCOPED_VIRTUAL_OBJECTS, true),
    ],
    env: &[],
};

async fn start() -> Restate {
    let key = key();
    let launcher = launcher_or_skip(ReusePolicy::Allowed).expect("selected live journey requires RESTATE_SERVER_BIN or RESTATE_ADMIN_URL and RESTATE_INGRESS_URL");
    let restate = launcher.launch(&SERVER).await;
    let config: StaticConfig = serde_json::from_value(json!({"account": {
        "id": "live", "agent_key": key, "defaults": {"e_invoice": false}
    }}))
    .expect("static account");
    let resolver = Arc::new(StaticResolver::try_from(config).expect("resolver"));
    let accounts = Accounts::new(resolver.clone(), resolver);
    let config = WorkerConfig::new("live".parse().expect("namespace"))
        .validate()
        .expect("production policy validation");
    restate
        .deploy(
            Endpoint::builder()
                .bind(Order::from_parts(accounts.clone(), config.clone()))
                .bind(Agent::from_parts(accounts, config))
                .build(),
        )
        .await;
    restate
}

fn document(price: Decimal, currency: &str) -> Value {
    let mut doc = DocumentInput::new(
        BuyerInput::new("Teszt Vevő", "1010", "Budapest", "Teszt utca 1."),
        vec![LineItemInput::new(
            "Integrációs teszt",
            dec!(2),
            "db",
            price,
            "27",
        )],
        previous_month(),
        today(),
        PaymentMethod::Transfer,
    );
    doc.overrides.currency = Some(currency.into());
    // Opposite to the account default: storno must derive appearance from the
    // verified original, rather than accidentally succeeding with the default.
    doc.overrides.e_invoice = Some(true);
    if currency == "EUR" {
        doc.overrides.exchange_rate = Some(restate_szamlazz::contract::ExchangeRateInput {
            bank: "MNB".into(),
            rate: Some(dec!(400)),
        });
    }
    json!({"document": doc, "options": {}})
}

async fn call(
    restate: &Restate,
    run: &mut Run,
    handler: &str,
    body: Option<&Value>,
    identity: &str,
) -> Value {
    let idempotency = format!("{}:{identity}", run.order);
    let mutation = handler != "get";
    if mutation {
        run.sending(format!(
            "Order.{} order={} ingress_key={idempotency}",
            handler, run.order
        ));
    }
    let reply = restate
        .invoke(
            &Call::object("Szamlazz.Order", &run.order, handler),
            body,
            Some(&idempotency),
        )
        .await;
    eprintln!(
        "LIVE ingress handler={handler} key={idempotency} invocation={} status={} outcome={} number={} storno={}",
        reply.invocation_id(),
        reply.status,
        reply.body["outcome"],
        reply.body["invoice_number"],
        reply.body["storno_number"]
    );
    assert_eq!(
        reply.status, 200,
        "worker fault: {}; reconcile this invocation before renewing its key",
        reply.body
    );
    if let Some(number) = reply.body["invoice_number"].as_str() {
        run.record(InvoiceNumber::new(number));
    }
    // A 200 is settled; faults/timeouts leave the pending diagnostic intact.
    if mutation {
        run.unresolved = None;
    }
    reply.body
}

async fn probe(restate: &Restate) {
    let reply = restate
        .invoke(
            &Call::service("Szamlazz.Agent", "check_account"),
            None,
            None,
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["credentials"]["state"], "ok", "{}", reply.body);
    assert_eq!(reply.body["namespace"], "live");
}

fn issued(reply: &Value) -> InvoiceNumber {
    assert_eq!(reply["outcome"], "issued", "{reply}");
    InvoiceNumber::new(reply["invoice_number"].as_str().expect("issued number"))
}

async fn consumed(
    restate: &Restate,
    run: &mut Run,
    number: &InvoiceNumber,
    by: &InvoiceNumber,
    identity: &str,
) -> Value {
    let state = call(restate, run, "get", None, identity).await;
    assert_eq!(state["proforma"]["state"], "consumed", "{state}");
    assert_eq!(state["proforma"]["number"], number.as_str());
    assert_eq!(state["proforma"]["by"], by.as_str());
    state
}

async fn repeated(
    restate: &Restate,
    run: &mut Run,
    handler: &str,
    body: &Value,
    identity: &str,
    first: &Value,
) {
    let replay = call(restate, run, handler, Some(body), identity).await;
    assert_eq!(
        replay, *first,
        "same ingress key replays the retained completion"
    );
    let fresh = call(
        restate,
        run,
        handler,
        Some(body),
        &format!("{identity}-fresh"),
    )
    .await;
    assert_eq!(fresh["outcome"], "already_issued", "{fresh}");
    assert_eq!(fresh["invoice_number"], first["invoice_number"]);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires test-mode SZAMLAZZ_AGENT_KEY and actual Restate; issues e-invoices"]
#[allow(clippy::too_many_lines)] // One ordered business lifecycle, including cleanup.
async fn ordinary_order_journey() {
    let mut run = Run::new();
    let restate = start().await;
    let result = AssertUnwindSafe(async {
        probe(&restate).await;
        let body = document(dec!(1234.25), "HUF");
        let proforma = issued(
            &call(
                &restate,
                &mut run,
                "create_proforma",
                Some(&body),
                "proforma",
            )
            .await,
        );
        let first = call(&restate, &mut run, "create_invoice", Some(&body), "invoice").await;
        let number = issued(&first);
        repeated(
            &restate,
            &mut run,
            "create_invoice",
            &body,
            "invoice",
            &first,
        )
        .await;
        let state = consumed(&restate, &mut run, &proforma, &number, "observe-consumed").await;
        assert_eq!(state["invoice"]["state"], "live", "{state}");
        assert_eq!(state["invoice"]["number"], number.as_str(), "{state}");
        let original = run.by_number(&number).await;
        assert_document(
            &original,
            &number,
            &run.order,
            DocumentType::Invoice,
            Currency::HUF,
            (dec!(2469), dec!(667), dec!(3136)),
        );
        assert!(original.info.appearance.is_e_invoice());
        assert_eq!(
            json!(original.info.fulfillment_date),
            body["document"]["fulfillment_date"],
            "original must retain the requested fulfillment date"
        );
        assert_eq!(original.info.referenced_proforma_number, Some(proforma));
        let reply = restate
            .invoke(
                &Call::service("Szamlazz.Agent", "query"),
                Some(&json!({"selector": {"invoice_number": number}})),
                None,
            )
            .await;
        eprintln!(
            "LIVE ingress handler=Agent.query invocation={} status={} number={number}",
            reply.invocation_id(),
            reply.status
        );
        assert_eq!(reply.status, 200, "{}", reply.body);
        let queried: QueryResponse =
            serde_json::from_value(reply.body).expect("worker query response");
        assert_eq!(queried.invoice_number, number.as_str());
        assert_eq!(
            queried.document_type,
            original.info.document_type.to_string()
        );
        assert_eq!(queried.order_number.as_deref(), Some(run.order.as_str()));
        assert_eq!(queried.test, Some(true));
        assert_eq!(queried.currency.as_deref(), Some("HUF"));
        assert_eq!(queried.net_total, Some(original.totals.total.net));
        assert_eq!(queried.vat_total, Some(original.totals.total.vat));
        assert_eq!(queried.gross_total, Some(original.totals.total.gross));
        assert_eq!(queried.fulfillment_date, original.info.fulfillment_date);
        assert_eq!(
            queried.referenced_proforma_number.as_deref(),
            original
                .info
                .referenced_proforma_number
                .as_ref()
                .map(InvoiceNumber::as_str)
        );
        let reversal = call(
            &restate,
            &mut run,
            "storno_invoice",
            Some(&json!({"invoice_number": number})),
            "storno",
        )
        .await;
        assert_eq!(reversal["outcome"], "reversed", "{reversal}");
        let storno_number =
            InvoiceNumber::new(reversal["storno_number"].as_str().expect("storno number"));
        let storno = run.by_number(&storno_number).await;
        assert_eq!(storno.info.document_type, DocumentType::Storno);
        assert_eq!(storno.info.referenced_invoice_number, Some(number.clone()));
        eprintln!(
            "LIVE original appearance={:?} storno appearance={:?}",
            original.info.appearance, storno.info.appearance
        );
        assert!(storno.info.appearance.is_e_invoice());
        assert_eq!(storno.info.fulfillment_date, original.info.fulfillment_date);
        assert_eq!(run.by_number(&number).await.info.reversed, Some(true));
        let ordinary = call(
            &restate,
            &mut run,
            "create_invoice",
            Some(&body),
            "after-storno",
        )
        .await;
        assert_eq!(ordinary["outcome"], "reversed");
        let mut reissue = body.clone();
        reissue["options"]["reissue"] = json!({"expected_number": number});
        let replacement = issued(
            &call(
                &restate,
                &mut run,
                "create_invoice",
                Some(&reissue),
                "reissue",
            )
            .await,
        );
        assert_ne!(replacement, number);
        let holder = run
            .query(InvoiceSelector::ExternalId(run.external_id("invoice")))
            .await;
        assert_document(
            &holder,
            &replacement,
            &run.order,
            DocumentType::Invoice,
            Currency::HUF,
            (dec!(2469), dec!(667), dec!(3136)),
        );
        let stale_intent = call(
            &restate,
            &mut run,
            "create_invoice",
            Some(&reissue),
            "stale-intent",
        )
        .await;
        assert_eq!(stale_intent["outcome"], "conflict");
        assert_eq!(stale_intent["conflict_reason"], "target_changed");
    })
    .catch_unwind()
    .await;
    run.finish(result).await;
    restate.finish().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires test-mode SZAMLAZZ_AGENT_KEY, EUR support and actual Restate"]
#[allow(clippy::too_many_lines)] // Keep the linked-document lifecycle in one scenario.
async fn prepayment_final_journey() {
    let mut run = Run::new();
    let restate = start().await;
    let result = AssertUnwindSafe(async {
        probe(&restate).await;
        // 2 × 12.345 = 24.69; VAT 6.6663 → 6.67; gross 31.36 EUR.
        let mut advance = document(dec!(12.345), "EUR");
        let proforma = issued(
            &call(
                &restate,
                &mut run,
                "create_proforma",
                Some(&advance),
                "proforma",
            )
            .await,
        );
        advance["options"]["proforma"] = json!({"number": proforma});
        let first = call(
            &restate,
            &mut run,
            "create_prepayment",
            Some(&advance),
            "prepayment",
        )
        .await;
        let prepayment = issued(&first);
        repeated(
            &restate,
            &mut run,
            "create_prepayment",
            &advance,
            "prepayment",
            &first,
        )
        .await;
        let stored = run.by_number(&prepayment).await;
        assert_document(
            &stored,
            &prepayment,
            &run.order,
            DocumentType::Prepayment,
            Currency::EUR,
            (dec!(24.69), dec!(6.67), dec!(31.36)),
        );
        assert_eq!(
            stored.info.referenced_proforma_number,
            Some(proforma.clone())
        );
        assert_eq!(stored.info.exchange_rate, Some(dec!(400)));
        let state = consumed(
            &restate,
            &mut run,
            &proforma,
            &prepayment,
            "observe-prepayment",
        )
        .await;
        assert_eq!(state["prepayment"]["state"], "live", "{state}");
        assert_eq!(
            state["prepayment"]["number"],
            prepayment.as_str(),
            "{state}"
        );
        let mut final_body = document(dec!(24.69), "EUR");
        final_body["document"]["items"]
            .as_array_mut()
            .expect("items")
            .push(json!(LineItemInput::new(
                "Előleg levonása",
                dec!(-1),
                "db",
                dec!(24.69),
                "27"
            )));
        let final_reply = call(
            &restate,
            &mut run,
            "create_final",
            Some(&final_body),
            "final",
        )
        .await;
        let final_number = issued(&final_reply);
        repeated(
            &restate,
            &mut run,
            "create_final",
            &final_body,
            "final",
            &final_reply,
        )
        .await;
        let stored = run.by_number(&final_number).await;
        // Full performance 49.38 + 13.33, minus 24.69 + 6.67.
        assert_document(
            &stored,
            &final_number,
            &run.order,
            DocumentType::Final,
            Currency::EUR,
            (dec!(24.69), dec!(6.66), dec!(31.35)),
        );
        assert_eq!(
            stored.info.referenced_invoice_number.as_ref(),
            Some(&prepayment)
        );
        assert_eq!(stored.items.len(), 2);
        for (item, (quantity, net, vat, gross)) in stored.items.iter().zip([
            (dec!(2), dec!(49.38), dec!(13.33), dec!(62.71)),
            (dec!(-1), dec!(-24.69), dec!(-6.67), dec!(-31.36)),
        ]) {
            assert_eq!(
                (
                    item.quantity,
                    item.net_value,
                    item.vat_value,
                    item.gross_value
                ),
                (quantity, net, vat, gross),
                "stored performance and prepayment deduction"
            );
            assert_eq!(item.vat_rate(), VatRate::percent(27));
        }
        assert_eq!(stored.info.exchange_rate, Some(dec!(400)));
        let state = consumed(&restate, &mut run, &proforma, &prepayment, "observe-final").await;
        assert_eq!(state["prepayment"]["state"], "live", "{state}");
        assert_eq!(
            state["prepayment"]["number"],
            prepayment.as_str(),
            "{state}"
        );
        assert_eq!(state["final"]["state"], "live", "{state}");
        assert_eq!(state["final"]["number"], final_number.as_str(), "{state}");
    })
    .catch_unwind()
    .await;
    run.finish(result).await;
    restate.finish().await;
}
