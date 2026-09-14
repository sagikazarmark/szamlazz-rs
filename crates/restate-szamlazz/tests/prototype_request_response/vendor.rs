//! Opt-in, bounded vendor experiment; two concurrent sends per tested kind.

use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};
use szamlazz_agent::{
    Credentials, DocumentType, InvoiceNumber, InvoiceSelector,
    ops::{
        invoice::{CreateInvoice, InvoiceKind},
        query_xml::{InvoiceDocument, QueryInvoiceXml},
        storno::StornoInvoice,
    },
    reqwest,
    wire::{AgentRequest, ENDPOINT, RawResponse},
};

struct Evidence {
    path: PathBuf,
    rows: Mutex<Vec<Value>>,
    start: Instant,
}

impl Evidence {
    fn record(&self, mut row: Value) {
        row["elapsed_ms"] = json!(self.start.elapsed().as_millis());
        eprintln!("VENDOR-PROTOTYPE {row}");
        let mut rows = self.rows.lock().expect("evidence");
        rows.push(row);
        std::fs::write(&self.path, serde_json::to_vec_pretty(&*rows).expect("JSON"))
            .expect("persist evidence");
    }
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .cookie_store(true)
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .timeout(szamlazz_agent::client::REQUEST_TIMEOUT)
        .build()
        .expect("HTTP client")
}

async fn exchange<R: AgentRequest>(
    http: &reqwest::Client,
    credentials: &Credentials,
    evidence: &Evidence,
    label: &str,
    request: &R,
) -> Result<R::Response, String> {
    let wire = request
        .to_wire(credentials)
        .map_err(|_| "local wire validation failed".to_owned())?;
    evidence.record(json!({"event":"sending", "label":label, "request_bytes":wire.body.len()}));
    let response = match http
        .post(ENDPOINT)
        .header("content-type", wire.content_type)
        .body(wire.body)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            evidence
                .record(json!({"event":"unanswered", "label":label,"timeout":error.is_timeout()}));
            return Err(format!(
                "{label}: unanswered; no automatic resend or cleanup"
            ));
        }
    };
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .map(|(n, v)| {
            (
                n.to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect::<Vec<_>>();
    let body = response
        .bytes()
        .await
        .map_err(|_| format!("{label}: incomplete response"))?;
    let raw = RawResponse::new(headers, body.to_vec()).with_status(status);
    evidence.record(
        json!({"event":"received", "label":label,"http_status":status,"body_bytes":body.len(),
        "reported_number":raw.szlahu("szlahu_szamlaszam"),"reported_id":raw.header("szlahu_id"),
        "reported_code":raw.header("szlahu_error_code")}),
    );
    request.parse(&raw).map_err(|error| {
        // Never persist response bodies, session cookies, or source diagnostics.
        let class = error.outcome_class();
        let code = match &error {
            szamlazz_agent::ResponseError::Api(api) => Some(api.code.to_string()),
            _ => None,
        };
        evidence.record(
            json!({"event":"parsed_error","label":label,"class":format!("{class:?}"),"code":code}),
        );
        format!("{label}: {class:?} code={code:?}")
    })
}

fn request(order: &str, external: &str, kind: InvoiceKind) -> CreateInvoice {
    let mut account =
        restate_szamlazz::account::Account::new("vendor-prototype", "vendor-prototype");
    account.defaults.e_invoice = false;
    let mut doc = super::document();
    let today = jiff::Zoned::now()
        .with_time_zone(jiff::tz::TimeZone::get("Europe/Budapest").expect("zone"))
        .date();
    doc.fulfillment_date = today;
    doc.due_date = today;
    let order = order.parse().expect("order");
    let mut result = account
        .build_create(
            restate_szamlazz::identity::IssuedKind::Invoice,
            &doc,
            &order,
            &restate_szamlazz::identity::ExternalId::new(external),
            restate_szamlazz::gateway::DocumentRefs::default(),
        )
        .expect("document");
    result.header.issue_date = Some(today);
    result.kind = kind;
    result
}

async fn inspect(
    http: &reqwest::Client,
    credentials: &Credentials,
    evidence: &Evidence,
    selector: InvoiceSelector,
    label: &str,
) -> Result<InvoiceDocument, String> {
    let doc = exchange(
        http,
        credentials,
        evidence,
        label,
        &QueryInvoiceXml::new(selector),
    )
    .await?;
    evidence.record(json!({"event":"document", "label":label,
        "number":doc.info.invoice_number.as_str(),"id":doc.info.id,"document_type":doc.info.document_type.to_string(),
        "order":doc.info.order_number,"test":doc.info.test,"reversed":doc.info.reversed,
        "base":doc.info.referenced_invoice_number.as_ref().map(InvoiceNumber::as_str),
        "net":doc.totals.total.net.to_string(),"gross":doc.totals.total.gross.to_string()}));
    if doc.info.test != Some(true) {
        return Err("queried document is not confirmed test-mode; stopped".into());
    }
    Ok(doc)
}

async fn pair(
    credentials: &Credentials,
    evidence: &Evidence,
    label: &str,
    request: &CreateInvoice,
) -> Result<Vec<InvoiceNumber>, String> {
    let a = http();
    let b = http();
    // Independent fresh jars and connections, identical serialized body, one poll
    // starts each exchange before waiting on either. This cannot control server scheduling.
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let left = async {
        barrier.wait().await;
        exchange(&a, credentials, evidence, &format!("{label}-a"), request).await
    };
    let right = async {
        barrier.wait().await;
        exchange(&b, credentials, evidence, &format!("{label}-b"), request).await
    };
    evidence.record(
        json!({"event":"pair_intent","label":label,"order":request.header.order_number,
        "external_id":request.external_id,"planned_sends":2,"independent_sessions":true}),
    );
    let (left, right) = tokio::join!(left, right);
    let mut numbers = BTreeSet::new();
    let mut errors = Vec::new();
    for (side, result) in [("a", left), ("b", right)] {
        match result {
            Ok(result) => match result.into_issued() {
                Some(issued) => {
                    evidence.record(json!({"event":"issued","label":label,"side":side,
                        "number":issued.invoice_number.as_str(),"id":issued.document_id}));
                    numbers.insert(issued.invoice_number.to_string());
                }
                None => errors.push(format!("{label}-{side}: unnumbered acknowledgement")),
            },
            Err(error) => errors.push(error),
        }
    }
    evidence.record(json!({"event":"pair_result","label":label,"distinct_reported_numbers":numbers,"errors":errors}));
    // These strings are our own class labels, never vendor message text. A
    // conclusive refusal of either of the two explicit sends is an observation,
    // not a reason to repeat it. Unknown exchanges stop the whole investigation.
    if numbers.is_empty()
        || errors.iter().any(|e| {
            !(e.contains(": Rejected code=") || e.contains(": DuplicateOrderNumber code="))
        })
    {
        return Err(format!(
            "pair stopped: {errors:?}; known numbers retained in evidence"
        ));
    }
    Ok(numbers.into_iter().map(InvoiceNumber::new).collect())
}

async fn cleanup(
    http: &reqwest::Client,
    credentials: &Credentials,
    evidence: &Evidence,
    number: &InvoiceNumber,
) -> Result<(), String> {
    let original = inspect(
        http,
        credentials,
        evidence,
        InvoiceSelector::InvoiceNumber(number.clone()),
        "cleanup-original",
    )
    .await?;
    if original.info.reversed == Some(true) {
        return Ok(());
    }
    let mut storno = StornoInvoice::new(number.clone());
    storno.fulfillment_date = original.info.fulfillment_date;
    storno.e_invoice = original.info.appearance.is_e_invoice();
    evidence.record(json!({"event":"cleanup_intent","number":number.as_str()}));
    let answer = exchange(http, credentials, evidence, "cleanup-storno", &storno).await?;
    let reversal = answer
        .into_numbered()
        .map_err(|_| "unnumbered cleanup; stop".to_owned())?;
    let doc = inspect(
        http,
        credentials,
        evidence,
        InvoiceSelector::InvoiceNumber(reversal.invoice_number.clone()),
        "cleanup-reversal",
    )
    .await?;
    let refreshed = inspect(
        http,
        credentials,
        evidence,
        InvoiceSelector::InvoiceNumber(number.clone()),
        "cleanup-refreshed-original",
    )
    .await?;
    if doc.info.document_type != DocumentType::Storno
        || doc.info.referenced_invoice_number.as_ref() != Some(number)
        || refreshed.info.reversed != Some(true)
    {
        return Err("cleanup not verified".into());
    }
    evidence.record(json!({"event":"cleanup_verified","number":number.as_str(),"storno":reversal.invoice_number.as_str()}));
    Ok(())
}

#[tokio::test]
#[ignore = "manual vendor prototype; test account with duplicate checking enabled; at most five create sends plus cleanup"]
async fn vendor_overlap() {
    let credentials =
        Credentials::agent_key(std::env::var("SZAMLAZZ_AGENT_KEY").expect("test account key"));
    let path = PathBuf::from(
        std::env::var("PROTOTYPE_VENDOR_REPORT_PATH").expect("absolute report path required"),
    );
    assert!(path.is_absolute());
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .expect("new evidence file; never overwrite a prior run");
    let evidence = Evidence {
        path,
        rows: Mutex::new(Vec::new()),
        start: Instant::now(),
    };
    let run = uuid::Uuid::new_v4().simple().to_string();
    let ordinary_order = format!("vp-{run}");
    let base_order = format!("vb-{run}");
    evidence.record(json!({"event":"start","timestamp":jiff::Timestamp::now().to_string(),"run":run,
        "ordinary_order":ordinary_order,"base_order":base_order,"test_account_and_duplicate_toggle":"user confirmed",
        "transport":"native reqwest; no retries or redirects; 60s timeout","planned_create_sends":5}));
    let observer = http();
    let result: Result<(), String> = async {
        let external = format!("vendor-prototype:{ordinary_order}:invoice");
        let ordinary = request(&ordinary_order, &external, InvoiceKind::invoice());
        let numbers = pair(&credentials, &evidence, "ordinary", &ordinary).await?;
        for number in &numbers {
            let doc = inspect(
                &observer,
                &credentials,
                &evidence,
                InvoiceSelector::InvoiceNumber(number.clone()),
                "ordinary-by-number",
            )
            .await?;
            if doc.info.document_type != DocumentType::Invoice
                || doc.info.order_number.as_deref() != Some(&ordinary_order)
            {
                return Err("ordinary identity mismatch".into());
            }
        }
        inspect(
            &observer,
            &credentials,
            &evidence,
            InvoiceSelector::ExternalId(external),
            "ordinary-by-external-id",
        )
        .await?;
        for number in &numbers {
            cleanup(&observer, &credentials, &evidence, number).await?;
        }

        let base_external = format!("vendor-prototype:{base_order}:invoice");
        let base = request(&base_order, &base_external, InvoiceKind::invoice());
        evidence
            .record(json!({"event":"base_intent","order":base_order,"external_id":base_external}));
        let base = exchange(&observer, &credentials, &evidence, "base-create", &base)
            .await?
            .into_issued()
            .ok_or_else(|| "unnumbered base".to_owned())?;
        evidence.record(json!({"event":"base_issued","number":base.invoice_number.as_str()}));
        inspect(
            &observer,
            &credentials,
            &evidence,
            InvoiceSelector::InvoiceNumber(base.invoice_number.clone()),
            "base-by-number",
        )
        .await?;
        let external = format!("vendor-prototype:{base_order}:corrective:c1");
        let corrective = request(
            &base_order,
            &external,
            InvoiceKind::Corrective {
                corrected_number: base.invoice_number.clone(),
            },
        );
        let numbers = pair(&credentials, &evidence, "corrective", &corrective).await?;
        for number in &numbers {
            let doc = inspect(
                &observer,
                &credentials,
                &evidence,
                InvoiceSelector::InvoiceNumber(number.clone()),
                "corrective-by-number",
            )
            .await?;
            if doc.info.document_type != DocumentType::Corrective
                || doc.info.referenced_invoice_number.as_ref() != Some(&base.invoice_number)
            {
                return Err("corrective identity mismatch".into());
            }
        }
        inspect(
            &observer,
            &credentials,
            &evidence,
            InvoiceSelector::ExternalId(external),
            "corrective-by-external-id",
        )
        .await?;
        // Reverse known correctives before their base. A refusal is recorded and
        // ends cleanup; no second reversal attempt or unsupported force action.
        for number in numbers.iter().rev() {
            cleanup(&observer, &credentials, &evidence, number).await?;
        }
        cleanup(&observer, &credentials, &evidence, &base.invoice_number).await?;
        Ok(())
    }
    .await;
    evidence.record(json!({"event":"finish","result":result.as_ref().map(|()| "complete").map_err(String::as_str)}));
    if let Err(error) = result {
        panic!("{error}; inspect persisted evidence before any further mutation");
    }
}

#[tokio::test]
#[ignore = "read-only follow-up for exact reported vendor prototype numbers"]
async fn vendor_observe_numbers() {
    let credentials =
        Credentials::agent_key(std::env::var("SZAMLAZZ_AGENT_KEY").expect("test key"));
    let path = PathBuf::from(
        std::env::var("PROTOTYPE_VENDOR_REPORT_PATH").expect("absolute evidence path"),
    );
    assert!(path.is_absolute());
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .expect("new evidence file");
    let evidence = Evidence {
        path,
        rows: Mutex::new(Vec::new()),
        start: Instant::now(),
    };
    let http = http();
    for number in std::env::var("PROTOTYPE_VENDOR_NUMBERS")
        .expect("exact numbers")
        .split(',')
    {
        inspect(
            &http,
            &credentials,
            &evidence,
            InvoiceSelector::InvoiceNumber(InvoiceNumber::new(number)),
            "follow-up",
        )
        .await
        .expect("read-only observation");
    }
}
