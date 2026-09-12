//! Read-only migration inventory. An empty inventory never settles vendor effects.
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Deserialize)]
struct Rows {
    rows: Vec<Value>,
}

/// All unfinished worker invocations and all Order state, across every scope.
#[derive(Debug, Serialize)]
pub struct Inventory {
    unfinished_invocations: Vec<Value>,
    order_state: Vec<Value>,
}

impl Inventory {
    /// State blocks without decoding its value, including unknown state keys.
    #[must_use]
    pub fn is_clear(&self) -> bool {
        self.unfinished_invocations.is_empty() && self.order_state.is_empty()
    }
}

/// Query Restate without following redirects or interpreting marker contents.
///
/// # Errors
/// Returns inspection failures; callers must not expose their sensitive details.
pub async fn inventory(admin: &str, token: Option<&str>) -> Result<Inventory> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()?;
    let query = async |sql: &str| -> Result<Vec<Value>> {
        let mut request = client
            .post(format!("{}/query", admin.trim_end_matches('/')))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(serde_json::to_vec(&json!({"query": sql}))?);
        if let Some(token) = token.filter(|value| !value.is_empty()) {
            request = request.bearer_auth(token);
        }
        let response = request.send().await?;
        ensure!(response.status().is_success(), "admin query failed");
        let rows: Rows = serde_json::from_slice(&response.bytes().await?)
            .context("admin query did not return rows")?;
        Ok(rows.rows)
    };
    Ok(Inventory {
        unfinished_invocations: query("SELECT id, scope, target_service_name, target_service_key, status FROM sys_invocation WHERE target_service_name IN ('Szamlazz.Order', 'Szamlazz.Agent') AND status <> 'completed'").await?,
        order_state: query("SELECT scope, service_name, service_key, key FROM state WHERE service_name = 'Szamlazz.Order'").await?,
    })
}

/// Print the inventory; exit 0 means clear, 1 blocked, 2 inspection failed.
pub async fn run(admin: &str) -> u8 {
    let token = std::env::var("RESTATE_ADMIN_TOKEN").ok();
    let result = async {
        let result = inventory(admin, token.as_deref()).await?;
        let json = serde_json::to_string_pretty(&result)?;
        Ok::<_, anyhow::Error>((result, json))
    }
    .await;
    if let Ok((result, json)) = result {
        println!("{json}");
        if result.is_clear() {
            eprintln!(
                "Inventory clear. Independently settle external uncertainty before switching."
            );
            0
        } else {
            eprintln!("BLOCKED: drain invocations and recover state under its original scope");
            1
        }
    } else {
        // Authenticated URLs, tokens and remote error bodies must never appear.
        eprintln!("INVENTORY FAILED; migration remains blocked");
        2
    }
}
