//! Configuration refusals at the public static resolver boundary.
use restate_szamlazz::account::{StaticConfig, StaticResolver};
use serde_json::json;

#[test]
fn invalid_account_defaults_and_seller_text_fail_at_construction() {
    for (block, field, value) in [
        ("defaults", "language", "hhu"),
        ("defaults", "aggregator", "private\u{0}value"),
        ("defaults", "currency", "EUR\u{ffff}"),
        ("seller", "bank_account", "private\u{0}value"),
    ] {
        let mut wire = json!({"account":{"id":"acct","agent_key":"dummy"}});
        wire["account"][block] = json!({field:value});
        let config: StaticConfig = serde_json::from_value(wire).expect("configuration shape");
        let error = StaticResolver::try_from(config).expect_err("invalid account configuration");
        let diagnostic = error.to_string();
        assert!(diagnostic.contains(field), "{diagnostic}");
        assert!(!diagnostic.contains(value), "{diagnostic}");
    }
    let config: StaticConfig = serde_json::from_value(json!({"account":{
        "id":"acct","agent_key":"dummy","seller":{"email":{"body":"private\u{0}value"}}
    }}))
    .expect("configuration shape");
    let error = StaticResolver::try_from(config).expect_err("invalid email configuration");
    assert!(error.to_string().contains("seller.email.body"));
}

#[test]
fn account_defaults_do_not_require_an_automatic_exchange_rate() {
    let config: StaticConfig = serde_json::from_value(json!({"account":{
        "id":"acct","agent_key":"dummy","defaults":{"currency":"EUR","exchange_rate_bank":"OTP"}
    }}))
    .expect("configuration shape");
    StaticResolver::try_from(config).expect("callers may provide explicit exchange rates");
}
