//! Check approved gross amounts without credentials or a vendor operation.
use restate_szamlazz::{
    account::Account,
    contract::DocumentInput,
    gateway::DocumentRefs,
    identity::{ExternalId, IssuedKind, OrderKey},
};
use szamlazz_agent::{Credentials, Currency, wire::AgentRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let document: DocumentInput = serde_json::from_str(include_str!("gross_invoice.json"))?;
    let preflight = document.monetary_preflight(&Currency::EUR)?;
    println!(
        "Approved totals: {}",
        serde_json::to_string(&preflight.totals)?
    );
    // Production Account::build_create uses the same preflight with its resolved
    // defaults. Here the example pins currency/rate in the document overrides.
    let request = Account::new("example", "example").build_create(
        IssuedKind::Invoice,
        &document,
        &OrderKey::parse("tickets")?,
        &ExternalId::new("example:tickets:invoice"),
        DocumentRefs::default(),
    )?;
    assert_eq!(request.items, preflight.items);
    // Exercise complete wire validation with dummy XML-safe credentials; no send.
    request.to_wire(&Credentials::agent_key("validation"))?;
    Ok(())
}
