//! What szamlazz.hu holds, as wiremock stubs for the crate's own tests
//! (mirroring `tests/gateway.rs` and the e2e harness's `szamlazz.rs`, which a
//! `#[cfg(test)]` module cannot share; see the parent module's docs): the
//! selector matchers (by number, order number, external id; the create,
//! storno, credit, delete and taxpayer operations), the response templates
//! (code 7, a created document, an API code, …) and [`holds`], which mounts
//! one [`Doc`] body on every selector the document is reachable by, so the
//! stubs cannot disagree.

use wiremock::matchers::{body_string_contains, method};
use wiremock::{Mock, MockBuilder, MockServer, ResponseTemplate};

use super::Doc;

impl Doc<'_> {
    /// The document as a wiremock response: the rendered `<szamla>` body.
    pub(crate) fn response(&self) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_raw(self.xml(), "application/xml")
    }
}

/// szamlazz.hu's code 7: no such document.
pub(crate) fn not_found() -> ResponseTemplate {
    api_error("7", "Hiányzó adat")
}

/// A created document: `number` with `net` and `gross`, the gross outstanding
/// in full.
pub(crate) fn created(number: &str, net: &str, gross: &str) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("szlahu_szamlaszam", number)
        .insert_header("szlahu_id", "924307747")
        .insert_header("szlahu_nettovegosszeg", net)
        .insert_header("szlahu_bruttovegosszeg", gross)
        .insert_header("szlahu_kintlevoseg", gross)
        .set_body_raw(
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><szamlaszam>{number}</szamlaszam><szamlanetto>{net}</szamlanetto><szamlabrutto>{gross}</szamlabrutto><kintlevoseg>{gross}</kintlevoseg></xmlszamlavalasz>"#
            ),
            "application/xml",
        )
}

/// szamlazz.hu's `code` with `message`, in the headers and the body as it
/// reports one.
pub(crate) fn api_error(code: &str, message: &str) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("szlahu_error_code", code)
        .insert_header("szlahu_error", message)
        .set_body_raw(
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod>{code}</hibakod><hibauzenet>{message}</hibauzenet></xmlszamlavalasz>"#
            ),
            "application/xml",
        )
}

/// The credit-entry operation's success: the invoice's totals after the
/// update, `outstanding` distinct from `gross` so the mapping is observable.
pub(crate) fn credited(number: &str, gross: &str, outstanding: &str) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("szlahu_szamlaszam", number)
        .insert_header("szlahu_bruttovegosszeg", gross)
        .insert_header("szlahu_kintlevoseg", outstanding)
        .set_body_raw(
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><szamlaszam>{number}</szamlaszam><szamlanetto>1000</szamlanetto><szamlabrutto>{gross}</szamlabrutto><kintlevoseg>{outstanding}</kintlevoseg></xmlszamlavalasz>"#
            ),
            "application/xml",
        )
}

/// The proforma deletion's success (`xmlszamladbkdelvalasz`).
pub(crate) fn proforma_deleted() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(
        r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamladbkdelvalasz xmlns="http://www.szamlazz.hu/xmlszamladbkdelvalasz"><sikeres>true</sikeres></xmlszamladbkdelvalasz>"#,
        "application/xml",
    )
}

/// NAV's answer for a known taxpayer (the agent crate's synthetic fixture).
pub(crate) fn taxpayer_known() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<QueryTaxpayerResponse xmlns="http://schemas.nav.gov.hu/OSA/2.0/api" xmlns:d="http://schemas.nav.gov.hu/OSA/2.0/data">
  <result><funcCode>OK</funcCode></result><taxpayerValidity>true</taxpayerValidity>
  <taxpayerData><taxpayerName>SYNTHETIC SOFTWARE KFT.</taxpayerName>
    <taxNumberDetail><d:taxpayerId>12345678</d:taxpayerId><d:vatCode>2</d:vatCode></taxNumberDetail>
    <taxpayerAddressList><taxpayerAddressItem><taxpayerAddressType>HQ</taxpayerAddressType><taxpayerAddress>
      <d:countryCode>HU</d:countryCode><d:postalCode>1111</d:postalCode><d:city>TESTVAROS</d:city>
      <d:streetName>MINTA</d:streetName><d:publicPlaceCategory>UTCA</d:publicPlaceCategory><d:number>1.</d:number>
    </taxpayerAddress></taxpayerAddressItem></taxpayerAddressList>
  </taxpayerData>
</QueryTaxpayerResponse>"#,
        "application/xml",
    )
}

/// A request of the operation whose multipart field is `action`.
fn op(action: &str) -> MockBuilder {
    Mock::given(method("POST")).and(body_string_contains(format!("name=\"{action}\"")))
}

/// A query by external id.
pub(crate) fn external_id_query(id: &str) -> MockBuilder {
    op("action-szamla_agent_xml").and(body_string_contains(format!(
        "<szamlaKulsoAzon>{id}</szamlaKulsoAzon>"
    )))
}

/// A query by order number (the order-number hint).
pub(crate) fn order_query(order: &str) -> MockBuilder {
    op("action-szamla_agent_xml").and(body_string_contains(format!(
        "<rendelesSzam>{order}</rendelesSzam>"
    )))
}

/// A query by document number (a verify).
pub(crate) fn number_query(number: &str) -> MockBuilder {
    op("action-szamla_agent_xml").and(body_string_contains(format!(
        "<szamlaszam>{number}</szamlaszam>"
    )))
}

/// A create (`xmlszamla`).
pub(crate) fn create() -> MockBuilder {
    op("action-xmlagentxmlfile")
}

/// A storno (`xmlszamlast`).
pub(crate) fn storno() -> MockBuilder {
    op("action-szamla_agent_st")
}

/// The credit-entry operation (`set_payments`).
pub(crate) fn credit() -> MockBuilder {
    op("action-szamla_agent_kifiz")
}

/// The proforma deletion of `number`.
pub(crate) fn delete_of(number: &str) -> MockBuilder {
    op("action-szamla_agent_dijbekero_torlese").and(body_string_contains(format!(
        "<szamlaszam>{number}</szamlaszam>"
    )))
}

/// The taxpayer query (`xmltaxpayer`) of `prefix`.
pub(crate) fn taxpayer_query(prefix: &str) -> MockBuilder {
    op("action-szamla_agent_taxpayer").and(body_string_contains(format!(
        "<torzsszam>{prefix}</torzsszam>"
    )))
}

/// szamlazz.hu holds `doc`: one body on every selector the document is
/// reachable by (its number, its order number when it carries one, and
/// `external_id` when the test states the id it sits under, which
/// szamlazz.hu never echoes in a body), so the stubs cannot disagree.
pub(crate) async fn holds(mock: &MockServer, doc: &Doc<'_>, external_id: Option<&str>) {
    let selectors = [
        Some(number_query(doc.number)),
        doc.order.map(order_query),
        external_id.map(external_id_query),
    ];
    for selector in selectors.into_iter().flatten() {
        selector.respond_with(doc.response()).mount(mock).await;
    }
}

/// szamlazz.hu holds nothing under `external_id`, nothing under `order`
/// (code 7 on both): a fresh order.
pub(crate) async fn absent(mock: &MockServer, external_ids: &[&str], order: &str) {
    for id in external_ids {
        external_id_query(id)
            .respond_with(not_found())
            .mount(mock)
            .await;
    }
    order_query(order)
        .respond_with(not_found())
        .mount(mock)
        .await;
}
