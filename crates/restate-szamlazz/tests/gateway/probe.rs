//! The `check_account` probe: one query of the sentinel external id, code 7
//! expected.

use super::common::{Doc, api_error, external_id_query, not_found};
use super::harness::*;
use restate_szamlazz::gateway::{ProbeOutcome, Unanswered};
use wiremock::ResponseTemplate;

/// The probe is one external-id query of the sentinel id and nothing else;
/// szamlazz.hu's code 7 is the expected answer and means the credentials
/// were accepted.
#[tokio::test]
async fn probe_sends_one_query_of_the_sentinel_id_and_accepts_not_found() {
    let h = Harness::start().await;
    external_id_query(probe_id().as_str())
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;

    assert_eq!(
        h.gateway.probe(&probe_id()).await,
        Ok(ProbeOutcome::Accepted)
    );

    let sent = h.bodies().await;
    assert_eq!(sent.len(), 1, "exactly one request: {sent:?}");
    assert!(
        sent[0].contains("name=\"action-szamla_agent_xml\""),
        "a query"
    );
    assert!(
        sent[0].contains("<szamlaKulsoAzon>acct:check-account</szamlaKulsoAzon>"),
        "of the sentinel id: {}",
        sent[0]
    );
    assert!(sent[0].contains("<szamlaagentkulcs>key</szamlaagentkulcs>"));
}

/// A document under the sentinel id (someone issued one by hand) still
/// proves the credentials; the probe issues nothing and reports `Accepted`.
#[tokio::test]
async fn probe_accepts_a_document_under_the_sentinel_id() {
    let h = Harness::start().await;
    external_id_query(probe_id().as_str())
        .respond_with(Doc::new("SZ-9", "SZ").response())
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.probe(&probe_id()).await,
        Ok(ProbeOutcome::Accepted)
    );
    assert_eq!(h.bodies().await.len(), 1);
}

/// A non-credential szamlazz.hu code on the probe still proves the key:
/// szamlazz.hu answers the credential codes before anything else.
#[tokio::test]
async fn probe_accepts_any_other_szamlazz_code() {
    let h = Harness::start().await;
    external_id_query(probe_id().as_str())
        .respond_with(api_error("57", "Rendszerhiba"))
        .expect(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.gateway.probe(&probe_id()).await,
        Ok(ProbeOutcome::Accepted)
    );
    assert_eq!(h.bodies().await.len(), 1);
}

/// A failed exchange (a transport failure, `szlahu_down`) settles nothing
/// about the credentials: it is the read's retryable error, not an outcome.
#[tokio::test]
async fn probe_without_an_answer_is_unanswered() {
    let h = Harness::start().await;
    external_id_query(probe_id().as_str())
        .respond_with(ResponseTemplate::new(503))
        .mount(&h.server)
        .await;
    assert!(matches!(
        h.gateway.probe(&probe_id()).await,
        Err(Unanswered::Transport(_))
    ));

    let h = Harness::start().await;
    external_id_query(probe_id().as_str())
        .respond_with(ResponseTemplate::new(503).insert_header("szlahu_down", "maintenance"))
        .mount(&h.server)
        .await;
    assert!(matches!(
        h.gateway.probe(&probe_id()).await,
        Err(Unanswered::Unavailable(message)) if message.contains("maintenance")
    ));
}
