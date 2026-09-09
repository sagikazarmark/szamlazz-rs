//! What szamlazz.hu says, once: the queried-document fixture ([`Doc`]) rendered
//! as szamlazz.hu's `<szamla>` response XML, the response templates of the
//! other operations (code 7, a created document, an API code in headers and
//! body or in the body alone, a credit entry's success, a proforma deletion,
//! NAV's taxpayer answers), the wiremock selector matchers (by number, order
//! number, external id; the create, storno, credit, delete and taxpayer
//! operations), and the HTTP client every test gateway is opened over.
//!
//! Shared by the three places a synthetic szamlazz.hu answer is stated: the
//! crate's unit tests (`src/test_support.rs` includes this file by path and
//! adds the parse into the worker's projection), the gateway's wiremock tests
//! (`tests/gateway/`) and the e2e suite (`tests/e2e/harness/szamlazz.rs`,
//! which adds the document-centric mount helpers). A `tests/common/` module
//! is what Cargo compiles into each integration-test binary that declares it
//! and never as a test of its own; each consumer uses a subset, hence the
//! `dead_code` allowance. Every test states the answer szamlazz.hu gives, byte
//! for byte (design §11), and this is the one place those bytes are shaped.
//!
//! The fixture's defaults: a live test-account document with the fulfillment
//! date of [`ORIGINAL_TELJ`], `eszamla` following the kind (`0` on a proforma,
//! `1`, paper, otherwise: what szamlazz.hu reports for a default create; `3`
//! is what it reports for `eszamla=true` and `2` was never observed, #73), and
//! the seller block's `id`, [`SUPPLIER`].

#![allow(
    dead_code,
    reason = "each consumer uses a subset of the shared fixtures"
)]

use jiff::civil::{Date, date};
use szamlazz_agent::client::REQUEST_TIMEOUT;
use szamlazz_agent::reqwest;
use wiremock::matchers::{body_string_contains, method};
use wiremock::{Mock, MockBuilder, ResponseTemplate};

/// The HTTP client every test gateway is opened over: the default client's
/// settings (a cookie jar of its own, the request timeout, no redirects) with
/// **no root certificates**, so building it never parses the system CA store
/// for a test whose every endpoint is plain `http://` (#136).
pub fn http_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .tls_certs_only(std::iter::empty())
        .cookie_store(true)
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
}

/// [`http_builder`], built.
pub fn http_client() -> reqwest::Client {
    http_builder().build().expect("http client")
}

/// The `szallito/id` the rendered documents carry unless a test says
/// otherwise: the `id` of the seller block as szamlazz.hu prints it in a query body
/// (972720 on the test account). Wire realism only: the worker holds no
/// account pin, and a test that renders another value asserts exactly that.
pub const SUPPLIER: i64 = 972_720;

/// The `telj` every document carries unless a test says otherwise: the
/// fulfillment date a storno of it must repeat.
pub const ORIGINAL_TELJ: Date = date(2026, 7, 15);

/// The `<teljesitesDatum>` element carrying [`ORIGINAL_TELJ`]: what every
/// storno of a fixture document must send.
pub fn original_telj_tag() -> String {
    format!("<teljesitesDatum>{ORIGINAL_TELJ}</teljesitesDatum>")
}

/// A queried document, rendered as szamlazz.hu's `<szamla>` response XML.
///
/// [`Doc::new`] is a live test-account document of `ORD-1` (the gateway and
/// unit tests' order), [`Doc::of`] one of a named order (the e2e's) and
/// [`Doc::unmanaged`] one carrying no order number; override fields with
/// struct-update syntax and render with [`Doc::xml`] or [`Doc::response`].
#[derive(Debug, Clone)]
pub struct Doc<'a> {
    /// `szamlaszam`.
    pub number: &'a str,
    /// `tipus`: `SZ`, `D`, `ES`, `VS`, `HS`, `SS`, …
    pub tipus: &'a str,
    /// `rendelesszam`; `None` renders no element: a document issued outside
    /// any order.
    pub order: Option<&'a str>,
    /// `teszt`: whether a test account issued the document. Parsed and
    /// projected by `query`, compared with nothing. `None` renders no element:
    /// szamlazz.hu breaking its schema, a document that does not say (the
    /// agent crate reports it as `None`).
    pub test: Option<bool>,
    /// `szallito/id`: the `id` of the seller block (`<szallito>`).
    /// Parsed, compared with nothing.
    pub supplier_id: i64,
    /// `<sztornozott>true</sztornozott>`: the document is reversed (as
    /// observed); `false` renders no element, as on a live document and on
    /// the storno invoice itself.
    pub reversed: bool,
    /// `hivszamlaszam`: the invoice a storno or a corrective references.
    pub referenced_invoice: Option<&'a str>,
    /// `hivdijbekszam`: the proforma an invoice or prepayment consumed.
    pub referenced_proforma: Option<&'a str>,
    /// `eszamla`; `None` follows `tipus`: `0` on a proforma, `1` (paper) on
    /// anything else. szamlazz.hu reports `1` for a default create and `3`
    /// for one created with `eszamla=true`; `2` was never observed (#73).
    pub eszamla: Option<i64>,
    /// `kelt`; `None` renders no element.
    pub issue_date: Option<Date>,
    /// `telj`; `None` renders no element: szamlazz.hu breaking its schema.
    pub fulfillment_date: Option<Date>,
    /// `osszegek/totalossz/netto`.
    pub net: &'a str,
    /// `osszegek/totalossz/afa`.
    pub vat: &'a str,
    /// `osszegek/totalossz/brutto`: what a document with no credit entries
    /// owes in full.
    pub gross: &'a str,
    /// `kifizetesek`: the credit entries registered against the document;
    /// empty renders no element. What makes a proforma *paid* for
    /// `delete_proforma`.
    pub credit_entries: &'a [CreditRecord<'a>],
    /// Further `<alap>` children, verbatim (`<fizh>…</fizh><devizanem>HUF</devizanem>`),
    /// for what no field covers; appended after the fields' elements, which
    /// the parser does not mind. Must not repeat an element a field renders.
    pub alap_extra: &'a str,
    /// The external id the document sits under, when a test states it. Not
    /// part of the body (szamlazz.hu never echoes `szamlaKulsoAzon`) but a
    /// selector the document is reachable by: the e2e harness's `holds`
    /// mounts the body on it.
    pub external_id: Option<&'a str>,
}

/// A credit entry (`kifizetes`) on a [`Doc`].
#[derive(Debug, Clone)]
pub struct CreditRecord<'a> {
    /// `datum`.
    pub date: Date,
    /// `jogcim`: the credit entry's title, e.g. `átutalás`.
    pub title: &'a str,
    /// `osszeg`.
    pub amount: &'a str,
    /// `megjegyzes`.
    pub comment: Option<&'a str>,
    /// `bankszamlaszam`.
    pub bank_account: Option<&'a str>,
}

impl<'a> CreditRecord<'a> {
    /// A credit entry of `amount` on `date` under `title`, with no comment
    /// and no bank account.
    pub const fn new(date: Date, title: &'a str, amount: &'a str) -> Self {
        Self {
            date,
            title,
            amount,
            comment: None,
            bank_account: None,
        }
    }

    /// A transfer of `amount` on the fixture's issue date: the shape a test
    /// that only counts or sums the entries needs.
    pub const fn transfer(amount: &'a str) -> Self {
        Self::new(date(2026, 9, 3), "transfer", amount)
    }
}

impl<'a> Doc<'a> {
    /// A live test-account document of `ORD-1` from [`SUPPLIER`].
    pub const fn new(number: &'a str, tipus: &'a str) -> Self {
        Self::of(number, tipus, "ORD-1")
    }

    /// A live test-account document of `order`.
    pub const fn of(number: &'a str, tipus: &'a str, order: &'a str) -> Self {
        Self {
            order: Some(order),
            ..Self::unmanaged(number, tipus)
        }
    }

    /// A live test-account document carrying no order number: issued outside
    /// the worker, reachable by number only.
    pub const fn unmanaged(number: &'a str, tipus: &'a str) -> Self {
        Self {
            number,
            tipus,
            order: None,
            test: Some(true),
            supplier_id: SUPPLIER,
            reversed: false,
            referenced_invoice: None,
            referenced_proforma: None,
            eszamla: None,
            issue_date: Some(date(2026, 9, 3)),
            fulfillment_date: Some(ORIGINAL_TELJ),
            net: "1000",
            vat: "270",
            gross: "1270",
            credit_entries: &[],
            alap_extra: "",
            external_id: None,
        }
    }

    /// A document of `ORD-1` that carries `<sztornozott>true</sztornozott>`.
    pub const fn reversed(number: &'a str, tipus: &'a str) -> Self {
        Self {
            reversed: true,
            ..Self::new(number, tipus)
        }
    }

    /// The document as szamlazz.hu's `<szamla>` response body.
    pub fn xml(&self) -> String {
        let opt = |tag: &str, value: Option<&str>| {
            value.map_or_else(String::new, |value| format!("<{tag}>{value}</{tag}>"))
        };
        // `0` on a proforma (not an invoice), `1` (paper) on anything else.
        let eszamla = self.eszamla.unwrap_or(match self.tipus {
            "D" => 0,
            _ => 1,
        });
        let kelt = self.issue_date.map(|date| date.to_string());
        let telj = self.fulfillment_date.map(|date| date.to_string());
        let teszt = self.test.map(|test| test.to_string());
        let credit_entries = if self.credit_entries.is_empty() {
            String::new()
        } else {
            let entries = self.credit_entries.iter().fold(String::new(), |xml, entry| {
                format!(
                    "{xml}<kifizetes><datum>{}</datum><jogcim>{}</jogcim><osszeg>{}</osszeg>{}{}</kifizetes>",
                    entry.date,
                    entry.title,
                    entry.amount,
                    opt("megjegyzes", entry.comment),
                    opt("bankszamlaszam", entry.bank_account),
                )
            });
            format!("<kifizetesek>{entries}</kifizetesek>")
        };
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<szamla xmlns="http://www.szamlazz.hu/szamla">
  <szallito><id>{supplier}</id><nev>Seller</nev><cim><irsz>1111</irsz><telepules>Budapest</telepules><cim>Fő u. 1.</cim></cim></szallito>
  <alap><id>924307338</id><szamlaszam>{number}</szamlaszam><tipus>{tipus}</tipus><eszamla>{eszamla}</eszamla>{hivszamlaszam}{hivdijbekszam}{kelt}{telj}{rendelesszam}{teszt}{sztornozott}{alap_extra}</alap>
  <vevo><nev>Buyer</nev></vevo>
  <tetelek></tetelek>
  <osszegek><totalossz><netto>{net}</netto><afa>{vat}</afa><brutto>{gross}</brutto></totalossz></osszegek>
  {credit_entries}
</szamla>"#,
            supplier = self.supplier_id,
            number = self.number,
            tipus = self.tipus,
            hivszamlaszam = opt("hivszamlaszam", self.referenced_invoice),
            hivdijbekszam = opt("hivdijbekszam", self.referenced_proforma),
            kelt = opt("kelt", kelt.as_deref()),
            telj = opt("telj", telj.as_deref()),
            rendelesszam = opt("rendelesszam", self.order),
            teszt = opt("teszt", teszt.as_deref()),
            sztornozott = if self.reversed {
                "<sztornozott>true</sztornozott>"
            } else {
                ""
            },
            alap_extra = self.alap_extra,
            net = self.net,
            vat = self.vat,
            gross = self.gross,
        )
    }

    /// The document as a wiremock response: szamlazz.hu's 200 with the XML.
    pub fn response(&self) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_raw(self.xml(), "application/xml")
    }
}

impl Default for Doc<'_> {
    fn default() -> Self {
        Self::new("SZ-1", "SZ")
    }
}

// ----- response templates ------------------------------------------------------

/// The body-only code 7 of the XML query: nothing under the selector.
pub fn not_found() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(
        r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod><![CDATA[7]]></hibakod><hibauzenet><![CDATA[Hiányzó adat: számla xml (ismeretlen számlaszám, rendelésszám vagy külső azonosító).]]></hibauzenet></xmlszamlavalasz>"#,
        "application/xml",
    )
}

/// A successful create / storno response (`xmlszamlavalasz`): the number and
/// the totals in the headers and the body, `kintlevoseg` equal to `gross`.
pub fn created(number: &str, net: &str, gross: &str) -> ResponseTemplate {
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

/// A success without a number: `xmlszamlavalasz` without `<szamlaszam>`,
/// which the agent crate refuses to parse for a create.
pub fn created_without_a_number() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(
        r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres></xmlszamlavalasz>"#,
        "application/xml",
    )
}

/// Code 56 with a number: szamlazz.hu issued `number` but could not deliver
/// its notification. The error code in the headers and the body, the number
/// and the totals in the headers: the shape the agent crate accepts;
/// szamlazz.hu's own shape for 56 (header or body, with or without the
/// number) is unverified, since the test account never produced the code.
pub fn created_but_notification_failed(number: &str, net: &str, gross: &str) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("szlahu_error_code", "56")
        .insert_header("szlahu_error", "notification failed")
        .insert_header("szlahu_szamlaszam", number)
        .insert_header("szlahu_id", "924307747")
        .insert_header("szlahu_nettovegosszeg", net)
        .insert_header("szlahu_bruttovegosszeg", gross)
        .insert_header("szlahu_kintlevoseg", gross)
        .set_body_raw(
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod>56</hibakod><hibauzenet>notification failed</hibauzenet><szamlaszam>{number}</szamlaszam></xmlszamlavalasz>"#
            ),
            "application/xml",
        )
}

/// An error of an operation that reports in headers and body.
pub fn api_error(code: &str, message: &str) -> ResponseTemplate {
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

/// An error of an operation that reports in the body only.
pub fn body_error(code: &str, message: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod>{code}</hibakod><hibauzenet>{message}</hibauzenet></xmlszamlavalasz>"#
        ),
        "application/xml",
    )
}

/// szamlazz.hu's `szlahu_down`: the maintenance answer, a 503 with the
/// header and no body the agent crate can read.
pub fn szlahu_down() -> ResponseTemplate {
    ResponseTemplate::new(503).insert_header("szlahu_down", "maintenance")
}

/// szamlazz.hu's 152 on a create: the order number already exists on another
/// document, naming the order and never the existing document's number.
pub fn duplicate_order_number(order: &str) -> ResponseTemplate {
    api_error(
        "152",
        &format!(
            "Már létező rendelésszám: {order}. Az ismétlődés engedélyezhető a Beállítások oldalon."
        ),
    )
}

/// The credit-entry operation's success: the invoice's totals after the
/// update, with `outstanding` (`kintlevoseg`) distinct from `gross` so that
/// the response's field mapping is observable.
pub fn credited(number: &str, gross: &str, outstanding: &str) -> ResponseTemplate {
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
pub fn proforma_deleted() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(
        r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamladbkdelvalasz xmlns="http://www.szamlazz.hu/xmlszamladbkdelvalasz"><sikeres>true</sikeres></xmlszamladbkdelvalasz>"#,
        "application/xml",
    )
}

/// The proforma deletion's code 335 (no such proforma: deleted already), in
/// headers and body as szamlazz.hu reports it.
pub fn proforma_gone() -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("szlahu_error_code", "335")
        .insert_header("szlahu_error", "Nincs+ilyen+d%C3%ADjbek%C3%A9r%C5%91")
        .set_body_raw(
            r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamladbkdelvalasz xmlns="http://www.szamlazz.hu/xmlszamladbkdelvalasz"><sikeres>false</sikeres><hibakod>335</hibakod><hibauzenet>Nincs ilyen díjbekérő</hibauzenet></xmlszamladbkdelvalasz>"#,
            "application/xml",
        )
}

/// NAV's answer for a known taxpayer: `taxpayerValidity` true with the
/// registered name, tax number detail and one `HQ` address (the agent
/// crate's synthetic fixture).
pub fn taxpayer_known() -> ResponseTemplate {
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

/// NAV's answer for a well-formed prefix it knows no taxpayer under:
/// `funcCode OK`, `taxpayerValidity` false, nothing else.
pub fn taxpayer_unknown() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<QueryTaxpayerResponse xmlns="http://schemas.nav.gov.hu/OSA/2.0/api"><result><funcCode>OK</funcCode></result><taxpayerValidity>false</taxpayerValidity></QueryTaxpayerResponse>"#,
        "application/xml",
    )
}

/// A NAV-side failure szamlazz.hu relays in the taxpayer response body:
/// `funcCode ERROR` with NAV's `errorCode` and `message`.
pub fn taxpayer_nav_error(code: &str, message: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<QueryTaxpayerResponse xmlns="http://schemas.nav.gov.hu/OSA/2.0/api"><result><funcCode>ERROR</funcCode><errorCode>{code}</errorCode><message>{message}</message></result></QueryTaxpayerResponse>"#
        ),
        "application/xml",
    )
}

// ----- selector matchers ---------------------------------------------------------

/// A request of the Számla Agent operation whose multipart field is `action`.
pub fn op(action: &str) -> MockBuilder {
    Mock::given(method("POST")).and(body_string_contains(format!("name=\"{action}\"")))
}

/// The XML query (`xmlszamlaxml`) by external id.
pub fn external_id_query(id: &str) -> MockBuilder {
    op("action-szamla_agent_xml").and(body_string_contains(format!(
        "<szamlaKulsoAzon>{id}</szamlaKulsoAzon>"
    )))
}

/// The XML query by order number: the lookup step's hint.
pub fn order_query(order: &str) -> MockBuilder {
    op("action-szamla_agent_xml").and(body_string_contains(format!(
        "<rendelesSzam>{order}</rendelesSzam>"
    )))
}

/// The XML query by invoice number: a verify.
pub fn number_query(number: &str) -> MockBuilder {
    op("action-szamla_agent_xml").and(body_string_contains(format!(
        "<szamlaszam>{number}</szamlaszam>"
    )))
}

/// A create (`xmlszamla`) of any document.
pub fn create() -> MockBuilder {
    op("action-xmlagentxmlfile")
}

/// A create of a document of `order`: the `<rendelesSzam>` the worker puts on
/// every create tells one order's sends from another's, so concurrent
/// scenarios never share a create stub.
pub fn create_for(order: &str) -> MockBuilder {
    create().and(body_string_contains(format!(
        "<rendelesSzam>{order}</rendelesSzam>"
    )))
}

/// The `<szamlaagentkulcs>` element carrying `agent_key`: what tells one
/// account's traffic from another's on the wire.
pub fn agent_key_tag(agent_key: &str) -> String {
    format!("<szamlaagentkulcs>{agent_key}</szamlaagentkulcs>")
}

/// A create request carrying `agent_key`.
pub fn create_with_key(agent_key: &str) -> MockBuilder {
    create().and(body_string_contains(agent_key_tag(agent_key)))
}

/// A create request whose seller block carries `bank_account`.
pub fn create_with_bank_account(bank_account: &str) -> MockBuilder {
    create().and(body_string_contains(format!(
        "<bankszamlaszam>{bank_account}</bankszamlaszam>"
    )))
}

/// A storno (`xmlszamlast`) of any invoice.
pub fn storno() -> MockBuilder {
    op("action-szamla_agent_st")
}

/// A storno of `number`: the `<szamlaszam>` of the original tells one
/// scenario's storno from another's.
pub fn storno_of_number(number: &str) -> MockBuilder {
    storno().and(body_string_contains(format!(
        "<szamlaszam>{number}</szamlaszam>"
    )))
}

/// A storno of `number` carrying the fixture's `telj` ([`ORIGINAL_TELJ`]) as
/// its `teljesitesDatum`: what every storno of a fixture document must send.
pub fn storno_of_number_repeating_telj(number: &str) -> MockBuilder {
    storno_of_number(number).and(body_string_contains(original_telj_tag()))
}

/// The credit-entry operation (`xmlszamlakifiz`).
pub fn credit() -> MockBuilder {
    op("action-szamla_agent_kifiz")
}

/// The credit-entry operation on `number`.
pub fn credit_of(number: &str) -> MockBuilder {
    credit().and(body_string_contains(format!(
        "<szamlaszam>{number}</szamlaszam>"
    )))
}

/// The proforma deletion (`xmlszamladbkdel`) of any proforma.
pub fn delete() -> MockBuilder {
    op("action-szamla_agent_dijbekero_torlese")
}

/// The proforma deletion of `number`.
pub fn delete_of(number: &str) -> MockBuilder {
    delete().and(body_string_contains(format!(
        "<szamlaszam>{number}</szamlaszam>"
    )))
}

/// The taxpayer query (`xmltaxpayer`) of the eight-digit `prefix`.
pub fn taxpayer_query(prefix: &str) -> MockBuilder {
    op("action-szamla_agent_taxpayer").and(body_string_contains(format!(
        "<torzsszam>{prefix}</torzsszam>"
    )))
}

/// The taxpayer query of `prefix` carrying `agent_key`: which account's key
/// NAV was asked with.
pub fn taxpayer_query_with_key(prefix: &str, agent_key: &str) -> MockBuilder {
    taxpayer_query(prefix).and(body_string_contains(agent_key_tag(agent_key)))
}
