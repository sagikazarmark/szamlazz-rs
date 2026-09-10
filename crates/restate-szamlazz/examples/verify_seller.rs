//! Deploy-side go-live check, outside Restate and its journal.
//!
//! Run `cargo run -p restate-szamlazz --example verify_seller < deploy-check.json`.
//! The input is private deployment configuration, never logged:
//! ```json
//! {
//!   "resolver": {"accounts": {"acme": {
//!     "id": "acme", "agent_key": "<from your deployment's credential source>"
//!   }}},
//!   "checks": [{"scope": "acme", "invoice_number": "E-2026-1",
//!     "test": false, "seller_name": "Acme Kft.", "seller_tax_number": "12345678-2-42"}]
//! }
//! ```
//! Supply a known document and independently established expectations for
//! every deployed scope (null for the unscoped account). Use the same resolver
//! configuration and credential source as the host, not a separately copied
//! agent key. For a custom host, call `verified_endpoint` with its actual
//! `Accounts` bundle before serving the returned endpoint. Repeat after key
//! rotation. A successful check is point-in-time, not a permanent account pin.
//!
//! Separately call `Szamlazz.Agent.check_account` through each deployed Restate
//! scope to check routing/protocol v7. `Szamlazz.Agent.query` deliberately
//! omits the seller block; this direct Számla Agent query is the seller check.

use std::time::Duration;

use restate_szamlazz::account::{StaticConfig, StaticResolver};
use restate_szamlazz::restate_sdk::prelude::Endpoint;
use restate_szamlazz::szamlazz_agent::ops::query_xml::QueryInvoiceXml;
use restate_szamlazz::szamlazz_agent::{Client, InvoiceSelector};
use restate_szamlazz::{Accounts, Agent, Order, ValidatedWorkerConfig};
use serde::Deserialize;

/// Expectations come from the operator's records, not the queried account.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownSeller {
    /// The scope served by the host; `None` for its unscoped account.
    pub scope: Option<String>,
    /// An independently known document on this account.
    pub invoice_number: String,
    /// Whether that document was issued on a test account.
    pub test: bool,
    /// The seller's exact name as printed on the known document.
    pub seller_name: String,
    /// The seller's exact Hungarian tax number on the known document.
    pub seller_tax_number: String,
}

/// Resolve and fetch exactly as the host does, using the resolved endpoint
/// and a fresh Számla Agent client for every check. No `ctx.run`, projection,
/// document serialisation, credential exposure, or shared cookie jar.
///
/// Errors identify the failed check without reflecting credential-store or
/// transport error text, which can contain deployment secrets.
///
/// # Errors
///
/// A failed resolution, credential fetch, query, or expectation check.
pub async fn verify_seller(
    accounts: &Accounts,
    expected: &KnownSeller,
) -> Result<(), &'static str> {
    if expected.invoice_number.trim().is_empty()
        || expected.seller_name.trim().is_empty()
        || expected.seller_tax_number.trim().is_empty()
    {
        return Err("the known invoice, expected seller name and tax number must be non-empty");
    }
    let account = tokio::time::timeout(
        Duration::from_secs(10),
        accounts.resolve(expected.scope.as_deref()),
    )
    .await
    .map_err(|_| "account resolution timed out")?
    .map_err(|_| "account resolution failed")?;
    let credentials = tokio::time::timeout(Duration::from_secs(10), accounts.fetch(&account))
        .await
        .map_err(|_| "credential fetch timed out")?
        .map_err(|_| "credential fetch failed")?;
    let builder = Client::builder()
        .credentials(credentials)
        .endpoint(account.endpoint.as_str());
    // Unit tests use plain-HTTP loopback only: keep a fresh cookie jar without
    // consulting the host's CA store. Production uses the default TLS client.
    #[cfg(test)]
    let builder = builder.http_client(common::http_client());
    let client = builder
        .build()
        .map_err(|_| "Számla Agent client could not be opened")?;
    let document = client
        .send(&QueryInvoiceXml::new(InvoiceSelector::InvoiceNumber(
            expected.invoice_number.clone().into(),
        )))
        .await
        .map_err(|_| "known-document query failed")?;
    if document.info.invoice_number.as_str() != expected.invoice_number {
        return Err("szamlazz.hu returned a different invoice number");
    }
    if document.info.test != Some(expected.test) {
        return Err("test/live mismatch or missing teszt on the known document");
    }
    if document.supplier.name != expected.seller_name {
        return Err("seller name mismatch on the known document");
    }
    if document.supplier.tax_number.as_deref() != Some(expected.seller_tax_number.as_str()) {
        return Err("seller tax number mismatch or missing adoszam on the known document");
    }
    Ok(())
}

/// Host wiring: verify with the exact resolver/store bundle bound to the
/// services, then return an endpoint the host can serve normally.
///
/// # Errors
///
/// Any failed check, an empty check list, or an invalid request identity key.
pub async fn verified_endpoint(
    accounts: Accounts,
    worker: ValidatedWorkerConfig,
    checks: &[KnownSeller],
    identity_key: &str,
) -> Result<Endpoint, &'static str> {
    verify_all(&accounts, checks).await?;
    Ok(Endpoint::builder()
        .bind(Order::from_parts(accounts.clone(), worker.clone()))
        .bind(Agent::from_parts(accounts, worker))
        .identity_key(identity_key)
        .map_err(|_| "invalid Restate request identity key")?
        .build())
}

async fn verify_all(accounts: &Accounts, checks: &[KnownSeller]) -> Result<(), &'static str> {
    if checks.is_empty() {
        return Err("provide a known-document check for every deployed scope");
    }
    for expected in checks {
        verify_seller(accounts, expected).await?;
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    resolver: StaticConfig,
    checks: Vec<KnownSeller>,
}

#[tokio::main]
async fn main() -> Result<(), &'static str> {
    let input: Input = serde_json::from_reader(std::io::stdin())
        .map_err(|_| "invalid deployment check JSON on stdin")?;
    // The static resolver exposes its scope list in configuration; enforce
    // coverage here. A custom resolver's host supplies its own complete list.
    let scopes: Vec<Option<&str>> = if input.resolver.account.is_some() {
        vec![None]
    } else {
        input
            .resolver
            .accounts
            .keys()
            .map(|scope| Some(scope.as_str()))
            .collect()
    };
    if scopes.iter().any(|scope| {
        !input
            .checks
            .iter()
            .any(|check| check.scope.as_deref() == *scope)
    }) {
        return Err("a deployed scope has no known-document check");
    }
    let resolver = StaticResolver::try_from(input.resolver)
        .map_err(|_| "invalid static resolver configuration")?;
    verify_all(&Accounts::from(resolver), &input.checks).await?;
    println!("All configured seller checks passed.");
    Ok(())
}

#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod common;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::body_string_contains;
    use wiremock::{MockServer, ResponseTemplate};

    #[tokio::test]
    async fn actual_scoped_resolver_store_and_endpoint_verify_all_three_expectations() {
        let mock = MockServer::start().await;
        let config: StaticConfig = serde_json::from_value(json!({"accounts": {"acme": {
            "id": "acct", "agent_key": "seller-check-secret", "endpoint": mock.uri()
        }}}))
        .expect("config");
        let accounts = Accounts::from(StaticResolver::try_from(config).expect("resolver"));
        let xml = common::Doc::default()
            .xml()
            .replace("</szallito>", "<adoszam>12345678-2-42</adoszam></szallito>");
        common::number_query("SZ-1")
            .and(body_string_contains(common::agent_key_tag(
                "seller-check-secret",
            )))
            .and(|request: &wiremock::Request| !request.headers.contains_key("cookie"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("set-cookie", "JSESSIONID=seller-check; Path=/")
                    .set_body_string(xml),
            )
            .expect(4)
            .mount(&mock)
            .await;
        let mut expected = KnownSeller {
            scope: Some("acme".to_owned()),
            invoice_number: "SZ-1".to_owned(),
            test: true,
            seller_name: "Seller".to_owned(),
            seller_tax_number: "12345678-2-42".to_owned(),
        };
        verify_seller(&accounts, &expected)
            .await
            .expect("matching seller");
        expected.test = false;
        assert!(
            verify_seller(&accounts, &expected)
                .await
                .expect_err("wrong mode")
                .contains("test/live")
        );
        expected.test = true;
        expected.seller_name = "Wrong Kft.".to_owned();
        assert!(
            verify_seller(&accounts, &expected)
                .await
                .expect_err("wrong name")
                .contains("seller name")
        );
        expected.seller_name = "Seller".to_owned();
        expected.seller_tax_number = "87654321-2-42".to_owned();
        assert!(
            verify_seller(&accounts, &expected)
                .await
                .expect_err("wrong tax")
                .contains("seller tax number")
        );
        expected.scope = Some("unknown".to_owned());
        assert_eq!(
            verify_seller(&accounts, &expected).await,
            Err("account resolution failed")
        );
        mock.verify().await;
    }

    #[tokio::test]
    async fn missing_test_or_tax_number_cannot_pass_the_go_live_check() {
        let mock = MockServer::start().await;
        let config: StaticConfig = serde_json::from_value(json!({"account": {
            "id": "acct", "agent_key": "seller-check-secret", "endpoint": mock.uri()
        }}))
        .expect("config");
        let accounts = Accounts::from(StaticResolver::try_from(config).expect("resolver"));
        let expected = KnownSeller {
            scope: None,
            invoice_number: "SZ-1".to_owned(),
            test: true,
            seller_name: "Seller".to_owned(),
            seller_tax_number: "12345678-2-42".to_owned(),
        };
        for (test, failure) in [(None, "missing teszt"), (Some(true), "missing adoszam")] {
            common::number_query("SZ-1")
                .respond_with(
                    common::Doc {
                        test,
                        ..common::Doc::default()
                    }
                    .response(),
                )
                .expect(1)
                .mount(&mock)
                .await;
            assert!(
                verify_seller(&accounts, &expected)
                    .await
                    .expect_err("missing fact")
                    .contains(failure)
            );
            mock.verify().await;
            mock.reset().await;
        }
    }
}
