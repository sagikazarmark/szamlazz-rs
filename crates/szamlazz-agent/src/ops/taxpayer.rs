//! Taxpayer query (`xmltaxpayer`): looks up a Hungarian taxpayer in the NAV
//! Online Invoice system by törzsszám and returns its registered data.

use std::str::FromStr;

use std::collections::HashSet;

use quick_xml::events::Event;
use quick_xml::reader::NsReader;

use crate::credentials::Credentials;
use crate::error::{ApiError, ErrorCode, ParseError, ResponseError};
use crate::wire::{AgentRequest, RawResponse};
use crate::xml;

/// An eight-digit Hungarian taxpayer törzsszám.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(transparent)]
pub struct TaxpayerPrefix(String);

impl TaxpayerPrefix {
    /// The validated eight-digit prefix.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(value: &str) -> Result<(), TaxpayerPrefixError> {
        if value.len() == 8 && value.bytes().all(|byte| byte.is_ascii_digit()) {
            Ok(())
        } else {
            Err(TaxpayerPrefixError)
        }
    }
}

impl FromStr for TaxpayerPrefix {
    type Err = TaxpayerPrefixError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::validate(value)?;
        Ok(Self(value.to_owned()))
    }
}

impl TryFrom<&str> for TaxpayerPrefix {
    type Error = TaxpayerPrefixError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

/// Validates without reallocating.
impl TryFrom<String> for TaxpayerPrefix {
    type Error = TaxpayerPrefixError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::validate(&value)?;
        Ok(Self(value))
    }
}

impl<'de> serde::Deserialize<'de> for TaxpayerPrefix {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::try_from(value).map_err(serde::de::Error::custom)
    }
}

/// Invalid taxpayer prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("taxpayer prefix must contain exactly eight ASCII digits")]
pub struct TaxpayerPrefixError;

/// The taxpayer-query operation (`xmltaxpayer`,
/// `action-szamla_agent_taxpayer`).
///
/// Asks NAV (via szamlazz.hu) whether a tax number belongs to a valid
/// taxpayer and returns the registered name and addresses. A well-formed but
/// nonexistent tax number is a *successful* query with
/// [`TaxpayerInfo::valid`] set to `false`.
#[doc(alias = "xmltaxpayer")]
#[doc(alias = "adószám")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QueryTaxpayer {
    /// The törzsszám: first 8 digits of the tax number.
    #[doc(alias = "törzsszám")]
    pub tax_number_prefix: TaxpayerPrefix,
}

impl QueryTaxpayer {
    /// A query for the given törzsszám (first 8 digits of the tax number).
    ///
    /// # Errors
    ///
    /// Returns an error unless `prefix` contains exactly eight ASCII digits.
    pub fn new(prefix: impl Into<String>) -> Result<Self, TaxpayerPrefixError> {
        Ok(Self {
            tax_number_prefix: TaxpayerPrefix::try_from(prefix.into())?,
        })
    }
}

/// A query for an already validated prefix; cannot fail.
impl From<TaxpayerPrefix> for QueryTaxpayer {
    fn from(tax_number_prefix: TaxpayerPrefix) -> Self {
        Self { tax_number_prefix }
    }
}

/// NAV's incorporation category, serialized as its open wire token.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Incorporation {
    /// `ORGANIZATION`: an organization.
    Organization,
    /// `SELF_EMPLOYED`: a self-employed individual.
    SelfEmployed,
    /// `TAXABLE_PERSON`: a private person with a tax number.
    TaxablePerson,
    /// A token this crate does not know, preserved verbatim.
    Other(String),
}

impl Incorporation {
    /// The exact wire token.
    #[must_use]
    pub fn as_wire(&self) -> &str {
        match self {
            Self::Organization => "ORGANIZATION",
            Self::SelfEmployed => "SELF_EMPLOYED",
            Self::TaxablePerson => "TAXABLE_PERSON",
            Self::Other(token) => token,
        }
    }
}

impl From<&str> for Incorporation {
    fn from(token: &str) -> Self {
        match token {
            "ORGANIZATION" => Self::Organization,
            "SELF_EMPLOYED" => Self::SelfEmployed,
            "TAXABLE_PERSON" => Self::TaxablePerson,
            other => Self::Other(other.to_owned()),
        }
    }
}

impl From<String> for Incorporation {
    fn from(token: String) -> Self {
        match Self::from(token.as_str()) {
            Self::Other(_) => Self::Other(token),
            known => known,
        }
    }
}

impl FromStr for Incorporation {
    type Err = std::convert::Infallible;

    fn from_str(token: &str) -> Result<Self, Self::Err> {
        Ok(Self::from(token))
    }
}

impl std::fmt::Display for Incorporation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_wire())
    }
}

impl serde::Serialize for Incorporation {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_wire())
    }
}

impl<'de> serde::Deserialize<'de> for Incorporation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from(String::deserialize(deserializer)?))
    }
}

/// A taxpayer as registered in the NAV Online Invoice system.
#[doc(alias = "adóalany")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct TaxpayerInfo {
    /// Whether NAV says this is a valid taxpayer (`taxpayerValidity`).
    pub valid: bool,
    /// Registered name (`taxpayerName`), when valid.
    pub name: Option<String>,
    /// Registered short name (`taxpayerData/taxpayerShortName`).
    #[doc(alias = "taxpayerShortName")]
    #[serde(default)]
    pub short_name: Option<String>,
    /// County code (`taxpayerData/taxNumberDetail/countyCode`), including leading zeroes.
    #[doc(alias = "countyCode")]
    #[serde(default)]
    pub county_code: Option<String>,
    /// VAT group identifier (`taxpayerData/vatGroupMembership`), not a boolean
    /// or an assembled full tax number.
    #[doc(alias = "vatGroupMembership")]
    #[serde(default)]
    pub vat_group_membership: Option<String>,
    /// NAV incorporation category (`taxpayerData/incorporation`).
    #[serde(default)]
    pub incorporation: Option<Incorporation>,
    /// Last data change, as the root-child `infoDate`'s decoded source text.
    /// This is **not a validated datetime** or the lookup time/cache expiry.
    /// Nonblank text is retained with its offset, precision or lack of zone;
    /// malformed advisory text is retained too. Absent, empty or XML-blank
    /// text is `None`. No timezone, temporal validation or TTL is inferred.
    #[doc(alias = "infoDate")]
    #[serde(default)]
    pub info_date: Option<String>,
    /// The 8-digit `taxpayerId`, when provided.
    #[doc(alias = "törzsszám")]
    pub tax_number: Option<String>,
    /// The VAT code digit (`vatCode`), when provided.
    #[doc(alias = "áfakód")]
    pub vat_code: Option<String>,
    /// Registered addresses (`taxpayerAddressItem` entries).
    pub addresses: Vec<TaxpayerAddress>,
}

/// A registered address of a taxpayer (`taxpayerAddressItem`).
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct TaxpayerAddress {
    /// Address type (`taxpayerAddressType`), e.g. `HQ`.
    pub kind: Option<String>,
    /// Country code (`countryCode`).
    pub country_code: Option<String>,
    /// Region (`region`).
    pub region: Option<String>,
    /// Postal code (`postalCode`).
    pub postal_code: Option<String>,
    /// City (`city`).
    pub city: Option<String>,
    /// Street name (`streetName`).
    pub street_name: Option<String>,
    /// `publicPlaceCategory`, e.g. `UTCA`.
    pub public_place_category: Option<String>,
    /// House number (`number`).
    pub number: Option<String>,
    /// Building (`building`).
    pub building: Option<String>,
    /// Staircase (`staircase`).
    pub staircase: Option<String>,
    /// Floor (`floor`).
    pub floor: Option<String>,
    /// Door (`door`).
    pub door: Option<String>,
    /// Lot number (`lotNumber`).
    pub lot_number: Option<String>,
    /// Tolerated extension (`additionalAddressDetail`). NAV declares this on
    /// simple addresses, but taxpayer addresses use `DetailedAddressType` in
    /// both 2.0 and 3.0, where this field is not declared. Retained when sent;
    /// it does not establish a supported taxpayer simple-address alternative.
    pub additional_address_detail: Option<String>,
}

impl AgentRequest for QueryTaxpayer {
    const ACTION: &'static str = "action-szamla_agent_taxpayer";
    type Response = TaxpayerInfo;

    fn write_xml(&self, credentials: &Credentials) -> Vec<u8> {
        xml::document(
            "xmltaxpayer",
            "http://www.szamlazz.hu/xmltaxpayer",
            |root| {
                root.node("beallitasok", |s| {
                    s.credentials(credentials);
                });
                root.text("torzsszam", self.tax_number_prefix.as_str());
            },
        )
    }

    fn parse(&self, response: &RawResponse) -> Result<Self::Response, ResponseError> {
        response.check()?;
        TaxpayerResponse::from_body(response.body())?.into_info()
    }
}

/// The NAV Online Invoice `QueryTaxpayerResponse` document, reduced to the
/// fields this crate surfaces.
#[derive(Debug, Default)]
struct TaxpayerResponse {
    func_code: Option<String>,
    error_code: Option<String>,
    message: Option<String>,
    validity: Option<bool>,
    name: Option<String>,
    short_name: Option<String>,
    county_code: Option<String>,
    vat_group_membership: Option<String>,
    incorporation: Option<Incorporation>,
    info_date: Option<String>,
    taxpayer_id: Option<String>,
    vat_code: Option<String>,
    addresses: Vec<TaxpayerAddress>,
}

/// Only recognized direct children can contribute data. An unknown frame
/// remains unknown all the way down, even if a descendant has a familiar name.
#[derive(Default)]
struct Frame {
    name: &'static str,
    scalar: bool,
    address: bool,
    seen: HashSet<&'static str>,
    text: String,
}

struct Layout {
    api: &'static str,
    result: &'static str,
    component: &'static str,
}

impl Layout {
    fn for_root_index(root_index: usize) -> Self {
        if root_index == 0 {
            Self {
                api: "http://schemas.nav.gov.hu/OSA/2.0/api",
                result: "http://schemas.nav.gov.hu/OSA/2.0/api",
                component: "http://schemas.nav.gov.hu/OSA/2.0/data",
            }
        } else {
            Self {
                api: "http://schemas.nav.gov.hu/OSA/3.0/api",
                result: "http://schemas.nav.gov.hu/NTCA/1.0/common",
                component: "http://schemas.nav.gov.hu/OSA/3.0/base",
            }
        }
    }

    fn child(&self, parent: &str, namespace: Option<&str>, name: &str) -> (&'static str, bool) {
        let (ns, containers, leaves): (&str, &[&'static str], &[&'static str]) = match parent {
            "QueryTaxpayerResponse" if name == "result" => (self.result, &["result"], &[]),
            "QueryTaxpayerResponse" => (
                self.api,
                &["taxpayerData"],
                &["taxpayerValidity", "infoDate"],
            ),
            "result" => (self.result, &[], &["funcCode", "errorCode", "message"]),
            "taxpayerData" => (
                self.api,
                &["taxNumberDetail", "taxpayerAddressList"],
                &[
                    "taxpayerName",
                    "taxpayerShortName",
                    "vatGroupMembership",
                    "incorporation",
                ],
            ),
            "taxNumberDetail" => (
                self.component,
                &[],
                &["taxpayerId", "vatCode", "countyCode"],
            ),
            "taxpayerAddressList" => (self.api, &["taxpayerAddressItem"], &[]),
            "taxpayerAddressItem" => (self.api, &["taxpayerAddress"], &["taxpayerAddressType"]),
            "taxpayerAddress" => (
                self.component,
                &[],
                &[
                    "countryCode",
                    "region",
                    "postalCode",
                    "city",
                    "streetName",
                    "publicPlaceCategory",
                    "number",
                    "building",
                    "staircase",
                    "floor",
                    "door",
                    "lotNumber",
                    "additionalAddressDetail",
                ],
            ),
            _ => return ("", false),
        };
        if namespace != Some(ns) {
            return ("", false);
        }
        if let Some(name) = containers.iter().find(|&&candidate| candidate == name) {
            return (name, false);
        }
        leaves
            .iter()
            .find(|&&candidate| candidate == name)
            .map_or(("", false), |name| (*name, true))
    }
}

impl TaxpayerResponse {
    /// Read the root-selected NAV layout by expanded name and parent path.
    #[expect(clippy::too_many_lines, reason = "one pull-parser event loop")]
    fn from_body(body: &[u8]) -> Result<Self, ParseError> {
        let (root_index, text) = xml::response_root(
            body,
            &[
                (
                    "QueryTaxpayerResponse",
                    "http://schemas.nav.gov.hu/OSA/2.0/api",
                ),
                (
                    "QueryTaxpayerResponse",
                    "http://schemas.nav.gov.hu/OSA/3.0/api",
                ),
            ],
        )?;
        let layout = Layout::for_root_index(root_index);
        let mut reader = NsReader::from_str(text);
        let mut parsed = Self::default();
        let mut stack: Vec<Frame> = Vec::new();

        loop {
            let (namespace, event) = reader
                .read_resolved_event()
                .map_err(quick_xml::DeError::from)?;
            let namespace = xml::namespace_uri(&namespace)?;
            let empty = matches!(event, Event::Empty(_));
            match event {
                Event::Start(start) | Event::Empty(start) => {
                    let (name, scalar, address) = if let Some(parent) = stack.last_mut() {
                        if parent.scalar {
                            return Err(ParseError::Invalid {
                                field: parent.name,
                                message: "child element in scalar".into(),
                            });
                        }
                        let (name, scalar) = layout.child(
                            parent.name,
                            namespace.as_deref(),
                            start.local_name().as_ref(),
                        );
                        if !name.is_empty()
                            && name != "taxpayerAddressItem"
                            && !parent.seen.insert(name)
                        {
                            return Err(ParseError::Invalid {
                                field: name,
                                message: "duplicate singleton".into(),
                            });
                        }
                        (
                            name,
                            scalar,
                            parent.address || name == "taxpayerAddressItem",
                        )
                    } else {
                        ("QueryTaxpayerResponse", false, false)
                    };
                    if name == "taxpayerAddressItem" {
                        parsed.addresses.push(TaxpayerAddress::default());
                    }
                    let frame = Frame {
                        name,
                        scalar,
                        address,
                        ..Frame::default()
                    };
                    if empty {
                        parsed.finish(&frame)?;
                    } else {
                        stack.push(frame);
                    }
                }
                Event::Text(text) => {
                    if let Some(frame) = stack.last_mut().filter(|f| f.scalar) {
                        frame.text.push_str(&text.xml10_content());
                    }
                }
                Event::CData(cdata) => {
                    if let Some(frame) = stack.last_mut().filter(|f| f.scalar) {
                        frame.text.push_str(&cdata.xml10_content());
                    }
                }
                Event::GeneralRef(reference) => {
                    let resolved = reference
                        .resolve_char_ref()
                        .map_err(quick_xml::DeError::from)?;
                    let ch = match resolved {
                        Some(ch) => ch,
                        None => match reference.as_ref() {
                            "amp" => '&',
                            "lt" => '<',
                            "gt" => '>',
                            "apos" => '\'',
                            "quot" => '"',
                            _ => {
                                return Err(ParseError::Invalid {
                                    field: "taxpayer response",
                                    message: format!("undefined entity: {}", reference.as_ref()),
                                });
                            }
                        },
                    };
                    if let Some(frame) = stack.last_mut().filter(|f| f.scalar) {
                        frame.text.push(ch);
                    }
                }
                Event::End(_) => {
                    if let Some(frame) = stack.pop() {
                        parsed.finish(&frame)?;
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }

        Ok(parsed)
    }

    fn finish(&mut self, frame: &Frame) -> Result<(), ParseError> {
        if frame.scalar {
            let value = if matches!(frame.name, "funcCode" | "errorCode" | "taxpayerValidity") {
                frame.text.trim()
            } else {
                &frame.text
            };
            if !value.chars().all(xml::is_xml_space) {
                self.set(frame.name, value, frame.address)?;
            }
        }
        Ok(())
    }

    /// Records a leaf element's text content, keyed by local name.
    fn set(&mut self, element: &str, value: &str, in_address: bool) -> Result<(), ParseError> {
        if in_address {
            let Some(address) = self.addresses.last_mut() else {
                return Ok(());
            };

            match element {
                "taxpayerAddressType" => address.kind = Some(value.to_owned()),
                "countryCode" => address.country_code = Some(value.to_owned()),
                "region" => address.region = Some(value.to_owned()),
                "postalCode" => address.postal_code = Some(value.to_owned()),
                "city" => address.city = Some(value.to_owned()),
                "streetName" => address.street_name = Some(value.to_owned()),
                "publicPlaceCategory" => {
                    address.public_place_category = Some(value.to_owned());
                }
                "number" => address.number = Some(value.to_owned()),
                "building" => address.building = Some(value.to_owned()),
                "staircase" => address.staircase = Some(value.to_owned()),
                "floor" => address.floor = Some(value.to_owned()),
                "door" => address.door = Some(value.to_owned()),
                "lotNumber" => address.lot_number = Some(value.to_owned()),
                "additionalAddressDetail" => {
                    address.additional_address_detail = Some(value.to_owned());
                }
                _ => {}
            }
            return Ok(());
        }
        match element {
            "funcCode" => self.func_code = Some(value.to_owned()),
            "errorCode" => self.error_code = Some(value.to_owned()),
            "message" => self.message = Some(value.to_owned()),
            "taxpayerValidity" => {
                self.validity = Some(match value {
                    "true" | "1" => true,
                    "false" | "0" => false,
                    other => {
                        return Err(ParseError::Invalid {
                            field: "taxpayerValidity",
                            message: format!("invalid XML boolean {other}"),
                        });
                    }
                });
            }
            "taxpayerName" => self.name = Some(value.to_owned()),
            "taxpayerShortName" => self.short_name = Some(value.to_owned()),
            "countyCode" => self.county_code = Some(value.to_owned()),
            "vatGroupMembership" => self.vat_group_membership = Some(value.to_owned()),
            "incorporation" => self.incorporation = Some(Incorporation::from(value)),
            "infoDate" => self.info_date = Some(value.to_owned()),
            "taxpayerId" => self.taxpayer_id = Some(value.to_owned()),
            "vatCode" => self.vat_code = Some(value.to_owned()),
            _ => {}
        }
        Ok(())
    }

    /// Converts a `funcCode` other than `OK` into the reported [`ApiError`].
    fn into_info(self) -> Result<TaxpayerInfo, ResponseError> {
        let func_code = self.func_code.ok_or(ParseError::Missing("funcCode"))?;

        if func_code == "OK" {
            return Ok(TaxpayerInfo {
                valid: self
                    .validity
                    .ok_or(ParseError::Missing("taxpayerValidity"))?,
                name: self.name,
                short_name: self.short_name,
                county_code: self.county_code,
                vat_group_membership: self.vat_group_membership,
                incorporation: self.incorporation,
                info_date: self.info_date,
                tax_number: self.taxpayer_id,
                vat_code: self.vat_code,
                addresses: self.addresses,
            });
        }
        let code = self
            .error_code
            .as_deref()
            .map_or(ErrorCode::Absent, ErrorCode::from);
        let message = match (self.error_code, self.message) {
            (_, Some(message)) => message,
            (Some(raw_code), None) => raw_code,
            (None, None) => format!("NAV funcCode {func_code}"),
        };

        Err(ApiError { code, message }.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> QueryTaxpayer {
        QueryTaxpayer::new("12345678").expect("valid prefix")
    }

    #[test]
    fn writes_canonical_taxpayer_xml() {
        let xml = QueryTaxpayer::new("12345678")
            .expect("valid prefix")
            .write_xml(&Credentials::agent_key("key"));
        let expected = include_str!("../../tests/golden/xmltaxpayer.xml").trim_end();
        assert_eq!(String::from_utf8(xml).expect("utf-8"), expected);
    }

    #[test]
    fn parses_valid_taxpayer_response() {
        let body = include_bytes!("../../tests/synthetic/taxpayer.xml");
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let info = sample().parse(&response).expect("success");
        assert!(info.valid);
        assert_eq!(info.name.as_deref(), Some("SYNTHETIC SOFTWARE KFT."));
        assert_eq!(info.tax_number.as_deref(), Some("12345678"));
        assert_eq!(info.vat_code.as_deref(), Some("2"));
        assert_eq!(info.addresses.len(), 1);
        let address = &info.addresses[0];
        assert_eq!(address.kind.as_deref(), Some("HQ"));
        assert_eq!(address.country_code.as_deref(), Some("HU"));
        assert_eq!(address.postal_code.as_deref(), Some("1111"));
        assert_eq!(address.city.as_deref(), Some("TESTVAROS"));
        assert_eq!(address.street_name.as_deref(), Some("MINTA"));
        assert_eq!(address.public_place_category.as_deref(), Some("UTCA"));
        assert_eq!(address.number.as_deref(), Some("1."));
    }

    #[test]
    fn parses_nav_3_taxpayer_response() {
        let body = include_bytes!("../../tests/synthetic/taxpayer_v3.xml");
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let info = sample().parse(&response).expect("success");
        assert!(info.valid);
        assert_eq!(info.tax_number.as_deref(), Some("12345678"));
    }

    #[test]
    fn rejects_invalid_taxpayer_prefixes() {
        assert_eq!(
            QueryTaxpayer::new("1234567").expect_err("short"),
            TaxpayerPrefixError
        );
        assert_eq!(
            QueryTaxpayer::new("1234567A").expect_err("nondigit"),
            TaxpayerPrefixError
        );
    }

    #[test]
    fn taxpayer_prefix_parses_from_str() {
        let prefix: TaxpayerPrefix = "12345678".parse().expect("valid");
        assert_eq!(prefix.as_str(), "12345678");
        assert_eq!(TaxpayerPrefix::try_from("12345678"), Ok(prefix.clone()));
        assert_eq!(
            TaxpayerPrefix::try_from(String::from("12345678")),
            Ok(prefix)
        );
        assert_eq!(
            "1234567".parse::<TaxpayerPrefix>(),
            Err(TaxpayerPrefixError)
        );
    }

    /// A validated prefix builds the query without a second validation.
    #[test]
    fn query_is_built_from_a_validated_prefix() {
        let prefix: TaxpayerPrefix = "12345678".parse().expect("valid");
        assert_eq!(QueryTaxpayer::from(prefix), sample());
    }

    #[test]
    fn invalid_tax_number_is_success_with_valid_false() {
        let body = include_bytes!("../../tests/synthetic/taxpayer_invalid_taxnumber.xml");
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let info = sample().parse(&response).expect("success");
        assert!(!info.valid);
        assert!(info.name.is_none());
        assert!(info.tax_number.is_none());
        assert!(info.addresses.is_empty());
    }

    #[test]
    fn parses_detailed_address_fields() {
        let body = br#"<QueryTaxpayerResponse xmlns="http://schemas.nav.gov.hu/OSA/2.0/api"><result><funcCode>OK</funcCode></result>
            <taxpayerValidity>true</taxpayerValidity><taxpayerData><taxpayerAddressList><taxpayerAddressItem>
            <taxpayerAddressType>SITE</taxpayerAddressType><api:taxpayerAddress xmlns:api="http://schemas.nav.gov.hu/OSA/2.0/api" xmlns="http://schemas.nav.gov.hu/OSA/2.0/data">
            <countryCode>HU</countryCode><region>Pest</region><postalCode>1111</postalCode>
            <city>Budapest</city><streetName>Fo</streetName><publicPlaceCategory>UTCA</publicPlaceCategory>
            <number>1</number><building>A</building><staircase>2</staircase><floor>3</floor>
            <door>4</door><lotNumber>123/4</lotNumber></api:taxpayerAddress></taxpayerAddressItem>
            </taxpayerAddressList></taxpayerData></QueryTaxpayerResponse>"#;
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let info = sample().parse(&response).expect("success");
        let address = &info.addresses[0];
        assert_eq!(address.region.as_deref(), Some("Pest"));
        assert_eq!(address.building.as_deref(), Some("A"));
        assert_eq!(address.staircase.as_deref(), Some("2"));
        assert_eq!(address.floor.as_deref(), Some("3"));
        assert_eq!(address.door.as_deref(), Some("4"));
        assert_eq!(address.lot_number.as_deref(), Some("123/4"));
    }

    /// The taxpayer data is journal-safe: it round-trips through JSON.
    #[test]
    fn taxpayer_info_round_trips_through_json() {
        let body = include_bytes!("../../tests/synthetic/taxpayer.xml");
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let info = sample().parse(&response).expect("success");

        let json = serde_json::to_value(&info).expect("serialize");
        assert_eq!(json["valid"], true);
        assert_eq!(json["tax_number"], "12345678");
        assert_eq!(json["addresses"][0]["kind"], "HQ");
        assert_eq!(json["addresses"][0]["public_place_category"], "UTCA");

        let restored: TaxpayerInfo = serde_json::from_value(json).expect("deserialize");
        assert_eq!(restored, info);
    }

    #[test]
    fn parses_xml_boolean_lexical_forms() {
        let body = br#"<QueryTaxpayerResponse xmlns="http://schemas.nav.gov.hu/OSA/2.0/api"><result><funcCode>OK</funcCode></result>
            <taxpayerValidity>1</taxpayerValidity></QueryTaxpayerResponse>"#;
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        assert!(sample().parse(&response).expect("success").valid);

        let body = br#"<QueryTaxpayerResponse xmlns="http://schemas.nav.gov.hu/OSA/2.0/api"><result><funcCode>OK</funcCode></result>
            <taxpayerValidity>invalid</taxpayerValidity></QueryTaxpayerResponse>"#;
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        assert!(sample().parse(&response).is_err());
    }

    #[test]
    fn preserves_additional_address_detail_as_a_tolerated_extension() {
        let body = br#"<QueryTaxpayerResponse xmlns="http://schemas.nav.gov.hu/OSA/2.0/api"><result><funcCode>OK</funcCode></result>
            <taxpayerValidity>true</taxpayerValidity><taxpayerData><taxpayerAddressList><taxpayerAddressItem>
            <taxpayerAddressType>HQ</taxpayerAddressType><api:taxpayerAddress xmlns:api="http://schemas.nav.gov.hu/OSA/2.0/api" xmlns="http://schemas.nav.gov.hu/OSA/2.0/data">
            <countryCode>HU</countryCode><postalCode>1111</postalCode><city>Budapest</city>
            <additionalAddressDetail>Main road 1.</additionalAddressDetail>
            </api:taxpayerAddress></taxpayerAddressItem></taxpayerAddressList></taxpayerData></QueryTaxpayerResponse>"#;
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let info = sample().parse(&response).expect("success");
        assert_eq!(
            info.addresses[0].additional_address_detail.as_deref(),
            Some("Main road 1.")
        );
    }

    #[test]
    fn rejects_unrelated_xml_root() {
        let response = RawResponse::new::<&str, &str>(
            [],
            b"<other><funcCode>OK</funcCode><taxpayerValidity>true</taxpayerValidity></other>"
                .to_vec(),
        );
        assert!(sample().parse(&response).is_err());
    }

    #[test]
    fn parses_error_response() {
        let body = include_bytes!("../../tests/synthetic/taxpayer_error.xml");
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let error = sample().parse(&response).expect_err("error");
        match error {
            ResponseError::Api(api) => {
                assert_eq!(api.code, crate::ErrorCode::MalformedXml);
                assert!(api.message.contains("Synthetic XML parsing error"));
            }
            other => panic!("expected api error, got {other:?}"),
        }
    }

    #[test]
    fn parses_cdata_error_message() {
        let body = br#"<QueryTaxpayerResponse xmlns="http://schemas.nav.gov.hu/OSA/2.0/api">
            <result><funcCode>ERROR</funcCode><errorCode>57</errorCode>
            <message><![CDATA[XML <hiba>]]></message></result></QueryTaxpayerResponse>"#;
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let error = sample().parse(&response).expect_err("error");
        match error {
            ResponseError::Api(api) => {
                assert_eq!(api.code, crate::ErrorCode::MalformedXml);
                assert_eq!(api.message, "XML <hiba>");
            }
            other => panic!("expected api error, got {other:?}"),
        }
    }

    #[test]
    fn preserves_nonnumeric_nav_error_code() {
        let body = br#"<QueryTaxpayerResponse xmlns="http://schemas.nav.gov.hu/OSA/2.0/api">
            <result><funcCode>ERROR</funcCode><errorCode>INVALID_REQUEST</errorCode>
            <message>Bad request</message></result></QueryTaxpayerResponse>"#;
        let response = RawResponse::new::<&str, &str>([], body.to_vec());
        let error = sample().parse(&response).expect_err("error");
        match error {
            ResponseError::Api(api) => {
                assert_eq!(
                    api.code,
                    crate::ErrorCode::Unknown("INVALID_REQUEST".to_owned())
                );
                assert_eq!(api.message, "Bad request");
            }
            other => panic!("expected api error, got {other:?}"),
        }
    }
}
