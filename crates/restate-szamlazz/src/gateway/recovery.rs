//! Read-only evidence for retained Order writes.

use serde::{Deserialize, Serialize};

use super::{
    CreateOutcome, DeleteOutcome, FoundDocument, Gateway, QueryError, QueryOutcome,
    StornoLookupOutcome, StornoOutcome, Unanswered,
};
use crate::contract::Selector;
use crate::contract::recovery::{UnresolvedWrite, WriteOperation};
use crate::identity::{ExternalId, OrderKey};

enum StornoEvidence {
    Verified(String),
    Inconclusive(&'static str),
}

/// The corrective base is part of issuance intent, beyond external-id ownership.
fn matches_corrective_base(found: &FoundDocument, operation: &WriteOperation) -> bool {
    match operation {
        WriteOperation::Create {
            corrected_number: Some(base),
            ..
        } => found.is_corrective_of(base),
        _ => true,
    }
}

/// A protected write result; uncertainty is journaled data, never a send retry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum WriteResult {
    Create(CreateOutcome),
    Storno(StornoOutcome),
    Delete(DeleteOutcome),
    /// A recovery read answered a code rather than evidence.
    Answered {
        credentials: bool,
        answer: super::SzamlazzAnswer,
    },
    Unresolved(WriteDiagnostic),
}

/// Minimal, safe evidence retained with uncertainty. No upstream response body
/// or vendor free-text message is copied into recovery diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WriteDiagnostic {
    pub(crate) reason: String,
    pub(crate) candidate_number: Option<String>,
}

impl WriteDiagnostic {
    pub(crate) fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            candidate_number: None,
        }
    }

    fn unconfirmed(cause: super::Unconfirmed) -> Self {
        match cause {
            super::Unconfirmed::Open { code, .. } => Self::new(match code {
                Some(code) => format!("open vendor code {code}"),
                None => "success without a document number".into(),
            }),
            super::Unconfirmed::StornoVerification { number, .. } => Self {
                reason: "numbered storno reply needs positive reversal evidence".into(),
                candidate_number: Some(number),
            },
            super::Unconfirmed::Transport(message) | super::Unconfirmed::Unavailable(message) => {
                Self::new(message)
            }
            super::Unconfirmed::ReQueryFailed { .. } => Self::new("post-send query failed"),
        }
    }
}

impl WriteResult {
    pub(crate) fn unresolved(reason: impl Into<String>) -> Self {
        Self::Unresolved(WriteDiagnostic::new(reason))
    }

    pub(crate) fn delete(outcome: DeleteOutcome) -> Self {
        match outcome {
            outcome @ (DeleteOutcome::Deleted
            | DeleteOutcome::AlreadyGone
            | DeleteOutcome::Rejected(_)
            | DeleteOutcome::Paid
            | DeleteOutcome::TargetChanged
            | DeleteOutcome::GuardFailed(_)
            | DeleteOutcome::Api(_)
            | DeleteOutcome::CredentialsRejected(_)) => Self::Delete(outcome),
            DeleteOutcome::Lost(cause) => Self::unresolved(cause.to_string()),
            DeleteOutcome::Inconclusive(answer) => {
                Self::unresolved(format!("inconclusive deletion code {}", answer.code))
            }
        }
    }
}

impl Gateway {
    /// The protected Order lookup: only absence permits a send. A holder that
    /// cannot establish this order's reversal is an unanswered evidence read.
    /// The fresh original check stays inside the same durable lookup operation.
    pub(crate) async fn lookup_order_storno(
        &self,
        external_id: &ExternalId,
        order: &OrderKey,
        number: &str,
    ) -> Result<StornoLookupOutcome, Unanswered> {
        let result = match self
            .query_raw(szamlazz_agent::InvoiceSelector::ExternalId(
                external_id.as_str().to_owned(),
            ))
            .await
        {
            Ok(found) => self.verify_order_storno(&found, order, number).await,
            Err(QueryError::NotFound) => return Ok(StornoLookupOutcome::Absent),
            Err(error) => Err(error),
        };
        match result {
            Ok(StornoEvidence::Verified(storno_number)) => {
                Ok(StornoLookupOutcome::AlreadyReversed { storno_number })
            }
            Ok(StornoEvidence::Inconclusive(reason)) => Err(Unanswered::Transport(reason.into())),
            Err(error) => match error.answered()? {
                super::Answer::CredentialsRejected(answer) => {
                    Ok(StornoLookupOutcome::CredentialsRejected(answer))
                }
                super::Answer::Api(answer) => Ok(StornoLookupOutcome::Api(answer)),
                super::Answer::NotFound => Err(Unanswered::Transport(
                    "original absent; reversal is not established".into(),
                )),
            },
        }
    }

    /// One evidence rule for protected lookup, leading query and recovery.
    /// A candidate never establishes reversal without a fresh, matching original.
    async fn verify_order_storno(
        &self,
        found: &FoundDocument,
        order: &OrderKey,
        number: &str,
    ) -> Result<StornoEvidence, QueryError> {
        if !found.is_storno_of(number) || !found.carries_order(order) {
            return Ok(StornoEvidence::Inconclusive(
                "candidate is not the original's storno of this order",
            ));
        }
        let original = self
            .query_raw(szamlazz_agent::InvoiceSelector::InvoiceNumber(
                szamlazz_agent::InvoiceNumber::new(number),
            ))
            .await?;
        if !original.is_stornoable()
            || !original.carries_order(order)
            || original.reversed != Some(true)
        {
            return Ok(StornoEvidence::Inconclusive(
                "original reversal and order identity are not established",
            ));
        }
        Ok(StornoEvidence::Verified(found.number.clone()))
    }

    pub(crate) async fn protected_create(
        &self,
        request: super::CreateStepRequest<'_>,
        marker: &UnresolvedWrite,
    ) -> WriteResult {
        match self.settled_by_query(&request, true).await {
            Ok(Some(CreateOutcome::Found(found) | CreateOutcome::Reversed(found)))
                if !matches_corrective_base(&found, &marker.operation) =>
            {
                // This execution has not sent. A wrong-base holder is a
                // collision, not completion of the requested correction.
                tracing::warn!(number = %found.number, "corrective base collision");
                return WriteResult::Create(CreateOutcome::Collision(found));
            }
            Ok(Some(outcome)) => return WriteResult::Create(outcome),
            Ok(None) | Err(super::QueryError::NotFound) => {}
            Err(super::QueryError::Api(answer)) => {
                return WriteResult::Create(CreateOutcome::Api(answer));
            }
            Err(super::QueryError::CredentialsRejected(answer)) => {
                return WriteResult::Create(CreateOutcome::CredentialsRejected(answer));
            }
            Err(super::QueryError::Unavailable(message)) => {
                return WriteResult::Create(CreateOutcome::Unavailable { message });
            }
            Err(super::QueryError::Transport(message)) => return WriteResult::unresolved(message),
        }
        match self.create_send(&request, true).await {
            Ok(outcome) => WriteResult::Create(outcome),
            // Uncertain sends go through the marker's stronger identity checks.
            Err(cause) => WriteResult::Unresolved(WriteDiagnostic::unconfirmed(cause)),
        }
    }

    pub(crate) async fn protected_storno(
        &self,
        request: super::StornoStepRequest<'_>,
        marker: &UnresolvedWrite,
    ) -> WriteResult {
        match self
            .lookup_order_storno(request.external_id, &marker.order, request.invoice_number)
            .await
        {
            Ok(StornoLookupOutcome::AlreadyReversed { storno_number }) => {
                return WriteResult::Storno(StornoOutcome::AlreadyReversed { storno_number });
            }
            Ok(StornoLookupOutcome::Absent) => {}
            Ok(StornoLookupOutcome::Api(answer)) => {
                return WriteResult::Storno(StornoOutcome::Api(answer));
            }
            Ok(StornoLookupOutcome::CredentialsRejected(answer)) => {
                return WriteResult::Storno(StornoOutcome::CredentialsRejected(answer));
            }
            Err(Unanswered::Unavailable(message)) => {
                return WriteResult::Storno(StornoOutcome::Unavailable { message });
            }
            Err(cause) => return WriteResult::unresolved(cause.to_string()),
        }
        match self.storno_send(request, Some(marker)).await {
            Ok(
                outcome @ (StornoOutcome::Reversed(_)
                | StornoOutcome::Rejected(_)
                | StornoOutcome::NotStornoable
                | StornoOutcome::CredentialsRejected(_)),
            ) => WriteResult::Storno(outcome),
            Err(cause) => WriteResult::Unresolved(WriteDiagnostic::unconfirmed(cause)),
            Ok(_) => WriteResult::unresolved("storno requires positive reversal evidence"),
        }
    }

    /// Query positive evidence for the exact marker, never sending a mutation.
    /// Absence and every inconclusive answer preserve uncertainty.
    pub(crate) async fn reconcile_write(
        &self,
        marker: &UnresolvedWrite,
        candidate: Option<&str>,
    ) -> WriteResult {
        let mut checked = self.reconcile_write_checked(marker, candidate).await;
        warn_reconciliation_credentials(&checked, marker);
        // A reply's candidate is a hint, not a restriction on automatic recovery.
        // Operator document evidence calls the checked method directly and remains
        // strict about the number the operator submitted.
        if candidate.is_some()
            && matches!(marker.operation, WriteOperation::Storno { .. })
            && !matches!(&checked, Ok(WriteResult::Storno(_)))
        {
            checked = self.reconcile_write_checked(marker, None).await;
            warn_reconciliation_credentials(&checked, marker);
        }
        match checked {
            Ok(WriteResult::Answered {
                credentials,
                answer,
            }) => WriteResult::unresolved(format!(
                "{} code {}",
                if credentials {
                    "credentials rejected"
                } else {
                    "inconclusive vendor"
                },
                answer.code
            )),
            Err(cause) => WriteResult::unresolved(cause.to_string()),
            Ok(result) => result,
        }
    }

    pub(crate) async fn reconcile_write_checked(
        &self,
        marker: &UnresolvedWrite,
        candidate: Option<&str>,
    ) -> Result<WriteResult, super::Unanswered> {
        // Storno is idempotent by original number; a repeat does not attach our
        // external id. A supplied candidate is therefore verified by number.
        // Without a candidate, the order hint can name a matching reversal.
        let queried = if matches!(marker.operation, WriteOperation::Storno { .. }) {
            if let Some(number) = candidate {
                self.verify(number).await?
            } else {
                match self
                    .query(&Selector::ExternalId(marker.external_id.clone()))
                    .await?
                {
                    QueryOutcome::NotFound => self.hint(&marker.order).await?,
                    outcome => outcome,
                }
            }
        } else {
            self.query(&Selector::ExternalId(marker.external_id.clone()))
                .await?
        };
        let found = match queried {
            QueryOutcome::Found(found) => found,
            QueryOutcome::CredentialsRejected(answer) => {
                return Ok(WriteResult::Answered {
                    credentials: true,
                    answer,
                });
            }
            QueryOutcome::Api(answer) => {
                return Ok(WriteResult::Answered {
                    credentials: false,
                    answer,
                });
            }
            QueryOutcome::NotFound => {
                return Ok(WriteResult::unresolved(
                    "document absent; absence does not settle the write",
                ));
            }
        };
        if candidate.is_some_and(|number| found.number != number) {
            return Ok(WriteResult::unresolved(
                "candidate number does not match the queried document",
            ));
        }
        Ok(match &marker.operation {
            WriteOperation::Create {
                kind,
                expected_number,
                ..
            } => {
                if !found.is_ours(&marker.order, *kind)
                    || expected_number.as_deref() == Some(found.number.as_str())
                    || !matches_corrective_base(&found, &marker.operation)
                {
                    return Ok(WriteResult::unresolved(
                        "document does not match order, kind or expected issuance intent",
                    ));
                }
                WriteResult::Create(if found.is_live() {
                    CreateOutcome::Reconciled(found)
                } else {
                    CreateOutcome::Reversed(found)
                })
            }
            WriteOperation::Storno { number } => {
                match self
                    .verify_order_storno(&found, &marker.order, number)
                    .await
                {
                    Ok(StornoEvidence::Verified(storno_number)) => {
                        WriteResult::Storno(StornoOutcome::AlreadyReversed { storno_number })
                    }
                    Ok(StornoEvidence::Inconclusive(reason)) => WriteResult::unresolved(reason),
                    Err(error) => match error.answered()? {
                        super::Answer::CredentialsRejected(answer) => WriteResult::Answered {
                            credentials: true,
                            answer,
                        },
                        super::Answer::Api(answer) => WriteResult::Answered {
                            credentials: false,
                            answer,
                        },
                        super::Answer::NotFound => {
                            WriteResult::unresolved("original absent; reversal is not established")
                        }
                    },
                }
            }
            // A query cannot distinguish a deletion from consumption or hiding.
            WriteOperation::Delete { .. } => WriteResult::unresolved(
                "deletion requires independent settlement; document queries cannot establish completion",
            ),
        })
    }
}

/// Alert before a fallback can replace the answer. This does not settle the
/// earlier write or change the retryable reconciliation result.
pub(super) fn warn_reconciliation_credentials(
    checked: &Result<WriteResult, Unanswered>,
    marker: &UnresolvedWrite,
) {
    if let Ok(WriteResult::Answered {
        credentials: true,
        answer,
    }) = checked
    {
        answer.warn_credentials_rejected(&marker.namespace);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::{Account, Endpoint};
    use crate::contract::recovery::MarkerVersion;
    use crate::identity::IssuedKind;
    use crate::test_support::{LogCapture, api_error, open_gateway};
    use szamlazz_agent::Credentials;
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::body_string_contains};

    #[tokio::test]
    async fn protected_duplicate_diagnostics_alert_without_changing_refusal() {
        use crate::gateway::CreateStepRequest;
        use szamlazz_agent::ops::invoice::{Buyer, CreateInvoice, InvoiceHeader, InvoiceKind};
        let capture = LogCapture::default();
        let _guard = capture.subscribe();
        LogCapture::rebuild_interest();
        for hint in [false, true] {
            let server = MockServer::start().await;
            let mut account = Account::new("account", "reference");
            account.endpoint = Endpoint::parse(&server.uri()).expect("endpoint");
            let gateway = open_gateway(account, Credentials::agent_key("PRIVATE-KEY"));
            let external_id = ExternalId::new("acct:ORD-1:invoice");
            let order = OrderKey::parse("ORD-1").expect("order");
            let header = InvoiceHeader::new(
                jiff::civil::date(2026, 9, 11),
                jiff::civil::date(2026, 9, 11),
                szamlazz_agent::PaymentMethod::Transfer,
                szamlazz_agent::Currency::HUF,
                szamlazz_agent::Language::Hungarian,
            );
            let mut create = CreateInvoice::new(
                InvoiceKind::invoice(),
                header,
                Buyer::new("Buyer", "1000", "City", "Street"),
                vec![
                    szamlazz_agent::LineItem::try_calculated(
                        "item",
                        rust_decimal::dec!(1),
                        "db",
                        rust_decimal::dec!(1),
                        szamlazz_agent::VatRate::Aam,
                        szamlazz_agent::Rounding::Exact,
                    )
                    .expect("item"),
                ],
            );
            create.header.order_number = Some("ORD-1".into());
            Mock::given(body_string_contains("action-xmlagentxmlfile"))
                .respond_with(api_error("152", "duplicate"))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(body_string_contains(
                "<szamlaKulsoAzon>acct:ORD-1:invoice</szamlaKulsoAzon>",
            ))
            .respond_with(if hint {
                api_error("7", "absent")
            } else {
                api_error("135", "PRIVATE-DIAGNOSTIC")
            })
            .expect(1)
            .mount(&server)
            .await;
            if hint {
                Mock::given(body_string_contains("<rendelesSzam>ORD-1</rendelesSzam>"))
                    .respond_with(api_error("135", "PRIVATE-DIAGNOSTIC"))
                    .expect(1)
                    .mount(&server)
                    .await;
            }
            let before = capture
                .logs()
                .matches("fix the account's agent key")
                .count();
            let result = gateway
                .create_send(
                    &CreateStepRequest {
                        external_id: &external_id,
                        kind: IssuedKind::Invoice,
                        order: &order,
                        create: &create,
                        reversed: None,
                    },
                    true,
                )
                .await
                .expect("settled refusal");
            assert!(
                matches!(result, CreateOutcome::DuplicateOrderNumber { .. }),
                "{result:?}"
            );
            let logs = capture.logs();
            assert_eq!(
                logs.matches("fix the account's agent key").count(),
                before + 1,
                "{logs}"
            );
            assert!(!logs.contains("PRIVATE-"), "{logs}");
            server.verify().await;
        }
    }

    #[tokio::test]
    async fn reconciliation_alerts_before_fallback_and_keeps_uncertainty() {
        let capture = LogCapture::default();
        let _guard = capture.subscribe();
        LogCapture::rebuild_interest();
        for candidate in [None, Some("SS-CANDIDATE")] {
            let server = MockServer::start().await;
            let mut account = Account::new("account", "reference");
            account.endpoint = Endpoint::parse(&server.uri()).expect("endpoint");
            let gateway = open_gateway(account, Credentials::agent_key("PRIVATE-KEY"));
            let marker = UnresolvedWrite {
                version: MarkerVersion,
                token: "owner".into(),
                owner_invocation: "owner".into(),
                created_at: "2026-09-11T12:00:00Z".into(),
                scope: None,
                order: OrderKey::parse("ORD-1").expect("order"),
                namespace: "acct".parse().expect("namespace"),
                external_id: if candidate.is_some() {
                    "acct:ORD-1:storno:SZ-1"
                } else {
                    "acct:ORD-1:invoice"
                }
                .into(),
                account_id: "account".into(),
                endpoint: server.uri(),
                credential_ref: "reference".into(),
                operation: if candidate.is_some() {
                    WriteOperation::Storno {
                        number: "SZ-1".into(),
                    }
                } else {
                    WriteOperation::Create {
                        kind: IssuedKind::Invoice,
                        expected_number: None,
                        corrected_number: None,
                    }
                },
            };
            Mock::given(body_string_contains(
                candidate.unwrap_or(&marker.external_id),
            ))
            .respond_with(api_error("135", "PRIVATE-VENDOR-CREDENTIALS"))
            .expect(1)
            .mount(&server)
            .await;
            if candidate.is_some() {
                Mock::given(body_string_contains(&marker.external_id))
                    .respond_with(ResponseTemplate::new(503))
                    .expect(1)
                    .mount(&server)
                    .await;
            }
            let before = capture
                .logs()
                .matches("fix the account's agent key")
                .count();
            let result = gateway.reconcile_write(&marker, candidate).await;
            assert!(matches!(result, WriteResult::Unresolved(_)), "{result:?}");
            let logs = capture.logs();
            assert_eq!(
                logs.matches("fix the account's agent key").count(),
                before + 1,
                "{logs}"
            );
            assert!(
                logs.contains("code=135") && logs.contains("namespace=acct"),
                "{logs}"
            );
            assert!(!logs.contains("PRIVATE-"), "{logs}");
            server.verify().await;
        }
    }

    #[tokio::test]
    async fn numberless_protected_storno_stays_unresolved_and_reconciliation_never_resends() {
        let server = MockServer::start().await;
        let mut account = Account::new("account", "reference");
        account.endpoint = Endpoint::parse(&server.uri()).expect("endpoint");
        let gateway = open_gateway(account, Credentials::agent_key("PRIVATE-KEY"));
        let external_id = ExternalId::new("acct:ORD-1:storno:SZ-1");
        let marker = UnresolvedWrite {
            version: MarkerVersion,
            token: "owner".into(),
            owner_invocation: "owner".into(),
            created_at: "2026-09-11T12:00:00Z".into(),
            scope: None,
            order: OrderKey::parse("ORD-1").expect("order"),
            namespace: "acct".parse().expect("namespace"),
            external_id: external_id.to_string(),
            account_id: "account".into(),
            endpoint: server.uri(),
            credential_ref: "reference".into(),
            operation: WriteOperation::Storno {
                number: "SZ-1".into(),
            },
        };
        Mock::given(body_string_contains("action-szamla_agent_xml"))
            .respond_with(api_error("7", "not found"))
            .expect(5)
            .mount(&server)
            .await;
        Mock::given(body_string_contains("action-szamla_agent_st"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><szamlabrutto>-1270</szamlabrutto><vevoifiokurl>PRIVATE-URL</vevoifiokurl><pdf>JVBERi0=</pdf></xmlszamlavalasz>"#,
            ))
            .expect(1)
            .mount(&server)
            .await;
        let result = gateway
            .protected_storno(
                crate::gateway::StornoStepRequest {
                    invoice_number: "SZ-1",
                    external_id: &external_id,
                    comment: None,
                    e_invoice: false,
                    fulfillment_date: jiff::civil::date(2026, 9, 11),
                },
                &marker,
            )
            .await;
        assert!(matches!(&result, WriteResult::Unresolved(_)), "{result:?}");
        let journal = serde_json::to_string(&result).expect("serialize");
        assert!(journal.contains("without a document number"));
        assert!(!journal.contains("PRIVATE-") && !journal.contains("JVBERi0="));
        // This is the continuation used after an uncertain protected send:
        // repeated absence retains the marker, never authorizes another send.
        for _ in 0..2 {
            let result = gateway.reconcile_write(&marker, None).await;
            assert!(matches!(result, WriteResult::Unresolved(_)), "{result:?}");
        }
        server.verify().await;
    }

    #[tokio::test]
    async fn immediate_storno_verification_alerts_and_retains_candidate() {
        use crate::gateway::StornoStepRequest;
        let capture = LogCapture::default();
        let _guard = capture.subscribe();
        LogCapture::rebuild_interest();
        let server = MockServer::start().await;
        let mut account = Account::new("account", "reference");
        account.endpoint = Endpoint::parse(&server.uri()).expect("endpoint");
        let gateway = open_gateway(account, Credentials::agent_key("PRIVATE-KEY"));
        let external_id = ExternalId::new("acct:ORD-1:storno:SZ-1");
        let marker = UnresolvedWrite {
            version: MarkerVersion,
            token: "owner".into(),
            owner_invocation: "owner".into(),
            created_at: "2026-09-11T12:00:00Z".into(),
            scope: None,
            order: OrderKey::parse("ORD-1").expect("order"),
            namespace: "acct".parse().expect("namespace"),
            external_id: external_id.to_string(),
            account_id: "account".into(),
            endpoint: server.uri(),
            credential_ref: "reference".into(),
            operation: WriteOperation::Storno {
                number: "SZ-1".into(),
            },
        };
        Mock::given(body_string_contains("action-szamla_agent_st"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                crate::test_support::numbered_reply_body("SS-CANDIDATE", None),
            ))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(body_string_contains(
            "<szamlaszam>SS-CANDIDATE</szamlaszam>",
        ))
        .respond_with(api_error("135", "PRIVATE-DIAGNOSTIC"))
        .expect(1)
        .mount(&server)
        .await;
        let result = gateway
            .storno_send(
                StornoStepRequest {
                    invoice_number: "SZ-1",
                    external_id: &external_id,
                    comment: None,
                    e_invoice: false,
                    fulfillment_date: jiff::civil::date(2026, 9, 11),
                },
                Some(&marker),
            )
            .await;
        assert!(
            matches!(result, Err(super::super::Unconfirmed::StornoVerification { ref number, .. }) if number == "SS-CANDIDATE"),
            "{result:?}"
        );
        let logs = capture.logs();
        assert_eq!(
            logs.matches("fix the account's agent key").count(),
            1,
            "{logs}"
        );
        assert!(
            logs.contains("code=135") && logs.contains("namespace=acct"),
            "{logs}"
        );
        assert!(!logs.contains("PRIVATE-"), "{logs}");
        server.verify().await;
    }
}
