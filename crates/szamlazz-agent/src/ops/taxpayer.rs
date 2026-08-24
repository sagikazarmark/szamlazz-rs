//! Taxpayer query (`xmltaxpayer`): looks up a Hungarian taxpayer in the NAV
//! Online Invoice system by törzsszám and returns its registered data.

use std::str::FromStr;

use quick_xml::Reader;
use quick_xml::events::Event;

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
#[non_exhaustive]
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

/// A taxpayer as registered in the NAV Online Invoice system.
#[doc(alias = "adóalany")]
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[non_exhaustive]
pub struct TaxpayerInfo {
    /// Whether NAV says this is a valid taxpayer (`taxpayerValidity`).
    pub valid: bool,
    /// Registered name (`taxpayerName`), when valid.
    pub name: Option<String>,
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
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize)]
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
    /// Free-form address detail used by NAV simple addresses
    /// (`additionalAddressDetail`).
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
    taxpayer_id: Option<String>,
    vat_code: Option<String>,
    addresses: Vec<TaxpayerAddress>,
}

impl TaxpayerResponse {
    /// Parses the body with a pull parser matching on *local* element names:
    /// the document mixes a default API namespace with `ns2:`-prefixed data
    /// elements, which the serde deserializer cannot express. Both NAV OSA 2.0
    /// and 3.0 response namespaces are accepted; unknown elements are skipped.
    fn from_body(body: &[u8]) -> Result<Self, ParseError> {
        let text = match xml::response_text(
            body,
            "QueryTaxpayerResponse",
            "http://schemas.nav.gov.hu/OSA/2.0/api",
        ) {
            Ok(text) => text,
            Err(ParseError::UnexpectedBody(_)) => xml::response_text(
                body,
                "QueryTaxpayerResponse",
                "http://schemas.nav.gov.hu/OSA/3.0/api",
            )?,
            Err(error) => return Err(error),
        };
        let mut reader = Reader::from_str(text);
        let mut parsed = Self::default();
        let mut content = String::new();
        let mut in_address = false;
        let mut root_seen = false;

        loop {
            match reader.read_event().map_err(quick_xml::DeError::from)? {
                Event::Start(start) => {
                    if !root_seen {
                        root_seen = true;
                        debug_assert_eq!(start.local_name().as_ref(), b"QueryTaxpayerResponse");
                    }
                    content.clear();
                    if start.local_name().as_ref() == b"taxpayerAddressItem" {
                        in_address = true;
                        parsed.addresses.push(TaxpayerAddress::default());
                    }
                }
                Event::Text(text) => {
                    content.push_str(&text.xml10_content().map_err(quick_xml::DeError::from)?);
                }
                Event::CData(cdata) => {
                    content.push_str(&cdata.xml10_content().map_err(quick_xml::DeError::from)?);
                }
                Event::GeneralRef(reference) => {
                    let resolved = reference
                        .resolve_char_ref()
                        .map_err(quick_xml::DeError::from)?;
                    match resolved {
                        Some(ch) => content.push(ch),
                        None => match reference
                            .decode()
                            .map_err(quick_xml::DeError::from)?
                            .as_ref()
                        {
                            "amp" => content.push('&'),
                            "lt" => content.push('<'),
                            "gt" => content.push('>'),
                            "apos" => content.push('\''),
                            "quot" => content.push('"'),
                            _ => {}
                        },
                    }
                }
                Event::End(end) => {
                    let name = end.local_name();
                    let value = content.trim();

                    if !value.is_empty() {
                        parsed.set(name.as_ref(), value, in_address)?;
                    }
                    if name.as_ref() == b"taxpayerAddressItem" {
                        in_address = false;
                    }
                    content.clear();
                }
                Event::Eof => break,
                _ => {}
            }
        }

        if !root_seen {
            return Err(ParseError::UnexpectedBody(
                "empty taxpayer response".to_owned(),
            ));
        }

        Ok(parsed)
    }

    /// Records a leaf element's text content, keyed by local name.
    fn set(&mut self, element: &[u8], value: &str, in_address: bool) -> Result<(), ParseError> {
        if in_address {
            let Some(address) = self.addresses.last_mut() else {
                return Ok(());
            };

            match element {
                b"taxpayerAddressType" => address.kind = Some(value.to_owned()),
                b"countryCode" => address.country_code = Some(value.to_owned()),
                b"region" => address.region = Some(value.to_owned()),
                b"postalCode" => address.postal_code = Some(value.to_owned()),
                b"city" => address.city = Some(value.to_owned()),
                b"streetName" => address.street_name = Some(value.to_owned()),
                b"publicPlaceCategory" => {
                    address.public_place_category = Some(value.to_owned());
                }
                b"number" => address.number = Some(value.to_owned()),
                b"building" => address.building = Some(value.to_owned()),
                b"staircase" => address.staircase = Some(value.to_owned()),
                b"floor" => address.floor = Some(value.to_owned()),
                b"door" => address.door = Some(value.to_owned()),
                b"lotNumber" => address.lot_number = Some(value.to_owned()),
                b"additionalAddressDetail" => {
                    address.additional_address_detail = Some(value.to_owned());
                }
                _ => {}
            }
            return Ok(());
        }
        match element {
            b"funcCode" => self.func_code = Some(value.to_owned()),
            b"errorCode" => self.error_code = Some(value.to_owned()),
            b"message" => self.message = Some(value.to_owned()),
            b"taxpayerValidity" => {
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
            b"taxpayerName" => self.name = Some(value.to_owned()),
            b"taxpayerId" => self.taxpayer_id = Some(value.to_owned()),
            b"vatCode" => self.vat_code = Some(value.to_owned()),
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
                tax_number: self.taxpayer_id,
                vat_code: self.vat_code,
                addresses: self.addresses,
            });
        }
        let code = self
            .error_code
            .as_deref()
            .map_or_else(|| ErrorCode::Unknown("0".to_owned()), ErrorCode::from);
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
        let body = include_str!("../../tests/synthetic/taxpayer.xml").replace(
            "http://schemas.nav.gov.hu/OSA/2.0/",
            "http://schemas.nav.gov.hu/OSA/3.0/",
        );
        let response = RawResponse::new::<&str, &str>([], body.into_bytes());
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
            <taxpayerValidity>true</taxpayerValidity><taxpayerAddressItem>
            <taxpayerAddressType>SITE</taxpayerAddressType><taxpayerAddress>
            <countryCode>HU</countryCode><region>Pest</region><postalCode>1111</postalCode>
            <city>Budapest</city><streetName>Fo</streetName><publicPlaceCategory>UTCA</publicPlaceCategory>
            <number>1</number><building>A</building><staircase>2</staircase><floor>3</floor>
            <door>4</door><lotNumber>123/4</lotNumber></taxpayerAddress></taxpayerAddressItem>
            </QueryTaxpayerResponse>"#;
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
    fn parses_simple_address_detail() {
        let body = br#"<QueryTaxpayerResponse xmlns="http://schemas.nav.gov.hu/OSA/2.0/api"><result><funcCode>OK</funcCode></result>
            <taxpayerValidity>true</taxpayerValidity><taxpayerAddressItem>
            <taxpayerAddressType>HQ</taxpayerAddressType><taxpayerAddress>
            <countryCode>HU</countryCode><postalCode>1111</postalCode><city>Budapest</city>
            <additionalAddressDetail>Main road 1.</additionalAddressDetail>
            </taxpayerAddress></taxpayerAddressItem></QueryTaxpayerResponse>"#;
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
