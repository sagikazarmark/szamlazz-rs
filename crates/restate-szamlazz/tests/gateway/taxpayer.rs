//! The *Taxpayer lookup*: NAV's record as data (`valid: false` included),
//! another code as `Api`, no answer as `Unanswered`.

use super::common::{
    api_error, taxpayer_known, taxpayer_nav_error, taxpayer_query, taxpayer_unknown,
};
use super::harness::*;
use restate_szamlazz::gateway::{SzamlazzAnswer, TaxpayerOutcome, Unanswered};
use wiremock::ResponseTemplate;

/// A known taxpayer is `Found` with NAV's registered data projected onto the
/// crate-owned response: one `xmltaxpayer` request of the prefix, carrying
/// the account's agent key.
#[tokio::test]
async fn taxpayer_query_of_a_known_prefix_is_found_with_the_registered_data() {
    let h = Harness::start().await;
    taxpayer_query("12345678")
        .respond_with(taxpayer_known())
        .expect(1)
        .mount(&h.server)
        .await;

    let outcome = h
        .gateway
        .query_taxpayer(&prefix())
        .await
        .expect("szamlazz.hu answered");
    let TaxpayerOutcome::Found(taxpayer) = outcome else {
        panic!("expected Found, got {outcome:?}");
    };
    assert!(taxpayer.valid);
    assert_eq!(taxpayer.name.as_deref(), Some("SYNTHETIC SOFTWARE KFT."));
    assert_eq!(taxpayer.tax_number.as_deref(), Some("12345678"));
    assert_eq!(taxpayer.vat_code.as_deref(), Some("2"));
    assert_eq!(taxpayer.addresses.len(), 1);
    let address = &taxpayer.addresses[0];
    assert_eq!(address.kind.as_deref(), Some("HQ"));
    assert_eq!(address.country_code.as_deref(), Some("HU"));
    assert_eq!(address.postal_code.as_deref(), Some("1111"));
    assert_eq!(address.city.as_deref(), Some("TESTVAROS"));
    assert_eq!(address.street_name.as_deref(), Some("MINTA"));
    assert_eq!(address.public_place_category.as_deref(), Some("UTCA"));
    assert_eq!(address.number.as_deref(), Some("1."));

    let sent = h.bodies().await;
    assert_eq!(sent.len(), 1, "exactly one request: {sent:?}");
    assert!(
        sent[0].contains("name=\"action-szamla_agent_taxpayer\""),
        "a taxpayer query: {}",
        sent[0]
    );
    assert!(
        sent[0].contains("<szamlaagentkulcs>key</szamlaagentkulcs>"),
        "with the account's key"
    );
}

/// A well-formed prefix NAV knows no taxpayer under is a normal answer,
/// `Found` with `valid: false` and nothing else, not a fault: the caller
/// asked whether the number is valid, and the answer is no.
#[tokio::test]
async fn taxpayer_query_of_an_unknown_prefix_is_found_invalid_as_data() {
    let h = Harness::start().await;
    taxpayer_query("12345678")
        .respond_with(taxpayer_unknown())
        .expect(1)
        .mount(&h.server)
        .await;

    let outcome = h
        .gateway
        .query_taxpayer(&prefix())
        .await
        .expect("szamlazz.hu answered");
    let TaxpayerOutcome::Found(taxpayer) = outcome else {
        panic!("expected Found, got {outcome:?}");
    };
    assert!(!taxpayer.valid);
    assert_eq!(taxpayer.name, None);
    assert_eq!(taxpayer.tax_number, None);
    assert_eq!(taxpayer.vat_code, None);
    assert!(taxpayer.addresses.is_empty());
    assert_eq!(h.bodies().await.len(), 1);
}

/// Any other `funcCode ≠ OK` is szamlazz.hu's *answer* (NAV's relayed
/// `errorCode` in the body, or a szamlazz.hu code of its own in the headers),
/// and is `Api` data, never `Unanswered`: an answer is not retried.
#[tokio::test]
async fn taxpayer_query_answered_with_another_code_is_api_data() {
    let h = Harness::start().await;
    taxpayer_query("12345678")
        .respond_with(taxpayer_nav_error("57", "Synthetic XML parsing error"))
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.query_taxpayer(&prefix()).await,
        Ok(TaxpayerOutcome::Api(SzamlazzAnswer::new(
            "57",
            "Synthetic XML parsing error"
        )))
    );
    assert_eq!(h.bodies().await.len(), 1);

    let h = Harness::start().await;
    taxpayer_query("12345678")
        .respond_with(api_error("57", "Rendszerhiba"))
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.query_taxpayer(&prefix()).await,
        Ok(TaxpayerOutcome::Api(SzamlazzAnswer::new(
            "57",
            "Rendszerhiba"
        )))
    );
}

/// A failed exchange (a transport failure, an unparseable body,
/// `szlahu_down`) settles nothing: it is the read's retryable `Unanswered`,
/// not an outcome.
#[tokio::test]
async fn taxpayer_query_without_an_answer_is_unanswered() {
    let h = Harness::start().await;
    taxpayer_query("12345678")
        .respond_with(ResponseTemplate::new(503))
        .mount(&h.server)
        .await;
    assert!(matches!(
        h.gateway.query_taxpayer(&prefix()).await,
        Err(Unanswered::Transport(_))
    ));

    let h = Harness::start().await;
    taxpayer_query("12345678")
        .respond_with(ResponseTemplate::new(200).set_body_raw("<garbage/>", "application/xml"))
        .mount(&h.server)
        .await;
    assert!(matches!(
        h.gateway.query_taxpayer(&prefix()).await,
        Err(Unanswered::Transport(_))
    ));

    let h = Harness::start().await;
    taxpayer_query("12345678")
        .respond_with(ResponseTemplate::new(503).insert_header("szlahu_down", "maintenance"))
        .mount(&h.server)
        .await;
    assert!(matches!(
        h.gateway.query_taxpayer(&prefix()).await,
        Err(Unanswered::Unavailable(message)) if message.contains("szlahu_down")
    ));
}
