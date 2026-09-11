//! The fixtures every family shares: the gateway opened as an operation opens
//! one (over `tests/common`'s HTTP client, against a wiremock server), the
//! order, external ids and document of `ORD-1`, and the [`Harness`] whose
//! methods run the lookup and create steps with their request structs filled
//! in.

use super::common::{ORIGINAL_TELJ, http_client};
use jiff::civil::date;
use restate_szamlazz::account::{Account, Endpoint};
use restate_szamlazz::contract::{
    BuyerInput, DocumentInput, IssuedKind, LineItemInput, PaymentMethod,
};
use restate_szamlazz::gateway::{
    CreateOutcome, CreatePermission, CreateStepRequest, DocumentRefs, Gateway, LookupOutcome,
    LookupRequest, OwnershipOutcome, StornoStepRequest, Unanswered, Unconfirmed,
};
use restate_szamlazz::{ExternalId, OrderKey};
use rust_decimal::dec;
use szamlazz_agent::Credentials;
use szamlazz_agent::ops::taxpayer::TaxpayerPrefix;
use wiremock::MockServer;

/// A gateway for the test account, opened as an executing operation would open it.
pub fn gateway(server: &MockServer) -> Gateway {
    open(server, "acct", "key")
}

pub fn order() -> OrderKey {
    OrderKey::parse("ORD-1").expect("order")
}

pub fn external_id() -> ExternalId {
    ExternalId::new("acct:ORD-1:invoice")
}

pub fn storno_id() -> ExternalId {
    ExternalId::new("acct:ORD-1:storno:SZ-1")
}

pub fn document() -> DocumentInput {
    DocumentInput::new(
        BuyerInput::new("Kovács Bt.", "2030", "Érd", "Tárnoki út 23."),
        vec![LineItemInput::new(
            "Elado izé",
            dec!(1),
            "db",
            dec!(1000),
            "27",
        )],
        date(2026, 9, 3),
        date(2026, 9, 11),
        PaymentMethod::Transfer,
    )
}

pub struct Harness {
    pub server: MockServer,
    pub gateway: Gateway,
}

impl Harness {
    /// Delete a pinned unpaid proforma whose fresh number query agrees.
    pub async fn delete(&self, number: &str) -> restate_szamlazz::gateway::DeleteOutcome {
        let doc = super::common::Doc::new(number, "D");
        super::common::number_query(number)
            .respond_with(doc.response())
            .mount(&self.server)
            .await;
        self.gateway
            .delete_proforma(&project(&doc), &order(), false)
            .await
    }
    pub async fn start() -> Self {
        let server = MockServer::start().await;
        let gateway = gateway(&server);
        Self { server, gateway }
    }

    /// The lookup step for an invoice of `ORD-1`, answered.
    pub async fn lookup(&self, our_numbers: &[String]) -> LookupOutcome {
        self.lookup_kind(IssuedKind::Invoice, &external_id(), our_numbers)
            .await
    }

    /// The lookup step for an invoice of `ORD-1`, as the run sees it:
    /// `Err(Unanswered)` is what the read policy re-executes.
    pub async fn try_lookup(&self, our_numbers: &[String]) -> Result<LookupOutcome, Unanswered> {
        self.gateway
            .lookup(LookupRequest {
                external_id: &external_id(),
                kind: IssuedKind::Invoice,
                order: &order(),
                our_numbers,
            })
            .await
    }

    pub async fn lookup_kind(
        &self,
        kind: IssuedKind,
        external_id: &ExternalId,
        our_numbers: &[String],
    ) -> LookupOutcome {
        self.gateway
            .lookup(LookupRequest {
                external_id,
                kind,
                order: &order(),
                our_numbers,
            })
            .await
            .expect("szamlazz.hu answered")
    }

    /// Authorize one isolated test create for `ORD-1`; `reversed` is the
    /// expected reversed holder. Tests must use reads after uncertainty, not
    /// call this helper again to manufacture another permission.
    pub async fn create(&self, reversed: Option<&str>) -> Result<CreateOutcome, Unconfirmed> {
        self.create_kind(IssuedKind::Invoice, &external_id(), reversed)
            .await
    }

    pub async fn create_kind(
        &self,
        kind: IssuedKind,
        external_id: &ExternalId,
        reversed: Option<&str>,
    ) -> Result<CreateOutcome, Unconfirmed> {
        let refs = if kind == IssuedKind::Corrective {
            DocumentRefs {
                corrected: Some("SZ-1"),
                ..DocumentRefs::default()
            }
        } else {
            DocumentRefs::default()
        };
        self.create_with_refs(kind, external_id, reversed, refs)
            .await
    }

    /// The create step for a document of `ORD-1` carrying `refs`: what the
    /// handler's steps 1–2 resolved.
    pub async fn create_with_refs(
        &self,
        kind: IssuedKind,
        external_id: &ExternalId,
        reversed: Option<&str>,
        refs: DocumentRefs<'_>,
    ) -> Result<CreateOutcome, Unconfirmed> {
        let order = order();
        let create = self
            .gateway
            .build_create(kind, &document(), &order, external_id, refs)
            .expect("build");
        self.gateway
            .create_once(
                CreateStepRequest {
                    external_id,
                    kind,
                    order: &order,
                    create: &create,
                    reversed,
                },
                CreatePermission::grant(),
            )
            .await
    }

    /// Observe the invoice holder without granting permission for a mutation.
    pub async fn observe_create(&self) -> Result<OwnershipOutcome, Unanswered> {
        self.gateway
            .lookup_ours(&external_id(), &order(), IssuedKind::Invoice)
            .await
    }

    pub async fn bodies(&self) -> Vec<String> {
        self.server
            .received_requests()
            .await
            .expect("requests")
            .iter()
            .map(|request| String::from_utf8_lossy(&request.body).into_owned())
            .collect()
    }
}

/// Parse the shared wire fixture into the Gateway's projection.
pub fn project(doc: &super::common::Doc<'_>) -> restate_szamlazz::gateway::FoundDocument {
    use szamlazz_agent::ops::query_xml::QueryInvoiceXml;
    use szamlazz_agent::wire::{AgentRequest, RawResponse};
    use szamlazz_agent::{InvoiceNumber, InvoiceSelector};
    QueryInvoiceXml::new(InvoiceSelector::InvoiceNumber(InvoiceNumber::new(
        doc.number,
    )))
    .parse(&RawResponse::new::<&str, &str>([], doc.xml().into_bytes()))
    .expect("fixture parses")
    .into()
}

/// An [`Account`] on `server`, and the gateway opened for it with `key` over
/// a fresh [`http_client`], as an executing operation opens one per execution.
pub fn open(server: &MockServer, id: &str, key: &str) -> Gateway {
    let mut account = Account::new(id, id);
    account.endpoint = Endpoint::parse(&server.uri()).expect("endpoint");
    Gateway::open_with_http(account, Credentials::agent_key(key), http_client()).expect("gateway")
}

/// The storno step request of `SZ-1`, repeating the original's `telj`
/// ([`ORIGINAL_TELJ`]) as its `fulfillment_date`.
pub fn storno_request(external_id: &ExternalId) -> StornoStepRequest<'_> {
    StornoStepRequest {
        invoice_number: "SZ-1",
        external_id,
        comment: Some("wrong buyer"),
        e_invoice: true,
        fulfillment_date: ORIGINAL_TELJ,
    }
}

/// The sentinel id the probe queries: `{namespace}:check-account`.
pub fn probe_id() -> ExternalId {
    ExternalId::for_probe(&"acct".parse().expect("namespace"))
}

/// The taxpayer prefix the *Taxpayer lookup* tests query.
pub fn prefix() -> TaxpayerPrefix {
    "12345678".parse().expect("prefix")
}
